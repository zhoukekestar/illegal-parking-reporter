// YOLOv8 车辆检测推理流水线
//
// 对应 DEVELOPMENT_PLAN.md P0 任务清单的 4 个子任务:
//   - preprocess: Letterbox + 归一化 + CHW
//   - inference: ort Session
//   - postprocess: 解析 [1, 84, 8400] 输出
//   - NMS

use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

use anyhow::{Context, Result};
use image::{imageops::FilterType, RgbImage};
use ndarray::{Array, Array4};
use once_cell::sync::OnceCell;
use ort::execution_providers::CoreMLExecutionProvider;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::TensorRef;

use crate::models::detection::{Detection, DetectionResult};

/// YOLOv8 模型输入边长 (固定 640)
pub const INPUT_SIZE: u32 = 640;
/// 置信度阈值: 低于此值的框直接丢弃
pub const CONF_THRESHOLD: f32 = 0.25;
/// NMS IoU 阈值: 同类别 IoU 高于此值视为重复
pub const NMS_IOU_THRESHOLD: f32 = 0.45;

// ========== Letterbox 参数 ==========

/// Letterbox 缩放参数, postprocess 反变换需要
#[derive(Debug, Clone, Copy)]
struct LetterboxInfo {
    /// 缩放比例 (新尺寸 / 原尺寸)
    ratio: f32,
    /// 横向 padding (左侧)
    pad_x: f32,
    /// 纵向 padding (顶部)
    pad_y: f32,
}

// ========== 1. Preprocess (MVU 5) ==========

/// Letterbox: 保持宽高比缩放到 INPUT_SIZE × INPUT_SIZE,
/// 不足部分用 (114, 114, 114) 灰色填充
fn letterbox(img: &RgbImage) -> (RgbImage, LetterboxInfo) {
    let (w, h) = img.dimensions();
    let target = INPUT_SIZE as f32;
    let ratio = (target / w as f32).min(target / h as f32);

    let new_w = (w as f32 * ratio).round() as u32;
    let new_h = (h as f32 * ratio).round() as u32;

    let resized = image::imageops::resize(img, new_w, new_h, FilterType::Triangle);

    // 居中 padding
    let pad_x = (INPUT_SIZE - new_w) / 2;
    let pad_y = (INPUT_SIZE - new_h) / 2;

    let mut canvas = RgbImage::from_pixel(INPUT_SIZE, INPUT_SIZE, image::Rgb([114, 114, 114]));
    image::imageops::overlay(&mut canvas, &resized, pad_x as i64, pad_y as i64);

    (
        canvas,
        LetterboxInfo {
            ratio,
            pad_x: pad_x as f32,
            pad_y: pad_y as f32,
        },
    )
}

/// 把 RGB 图像归一化到 [0, 1] 并转 CHW 排布,
/// 输出形状 [1, 3, INPUT_SIZE, INPUT_SIZE]
fn to_chw_tensor(img: &RgbImage) -> Array4<f32> {
    let (w, h) = img.dimensions();
    debug_assert_eq!(w, INPUT_SIZE);
    debug_assert_eq!(h, INPUT_SIZE);

    let mut tensor = Array::zeros((1, 3, h as usize, w as usize));
    for y in 0..h {
        for x in 0..w {
            let p = img.get_pixel(x, y);
            tensor[[0, 0, y as usize, x as usize]] = p[0] as f32 / 255.0;
            tensor[[0, 1, y as usize, x as usize]] = p[1] as f32 / 255.0;
            tensor[[0, 2, y as usize, x as usize]] = p[2] as f32 / 255.0;
        }
    }
    tensor
}

// ========== 2. Postprocess (MVU 6) ==========

/// 把 letterbox 坐标系下的 (cx, cy, w, h) 映射回原始图像 (x1, y1, x2, y2)
fn unletterbox(cx: f32, cy: f32, w: f32, h: f32, info: &LetterboxInfo, img_w: u32, img_h: u32) -> [f32; 4] {
    // 先转 (x1, y1, x2, y2)
    let x1 = cx - w / 2.0;
    let y1 = cy - h / 2.0;
    let x2 = cx + w / 2.0;
    let y2 = cy + h / 2.0;

    // 反 letterbox: 减 padding, 除 ratio
    let inv = 1.0 / info.ratio;
    let x1 = ((x1 - info.pad_x) * inv).clamp(0.0, img_w as f32);
    let y1 = ((y1 - info.pad_y) * inv).clamp(0.0, img_h as f32);
    let x2 = ((x2 - info.pad_x) * inv).clamp(0.0, img_w as f32);
    let y2 = ((y2 - info.pad_y) * inv).clamp(0.0, img_h as f32);

    [x1, y1, x2, y2]
}

/// 单个候选框 (NMS 内部用)
#[derive(Debug, Clone)]
struct Candidate {
    bbox: [f32; 4],
    score: f32,
    class_id: u32,
}

fn box_iou(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let inter_x1 = a[0].max(b[0]);
    let inter_y1 = a[1].max(b[1]);
    let inter_x2 = a[2].min(b[2]);
    let inter_y2 = a[3].min(b[3]);

    let inter_w = (inter_x2 - inter_x1).max(0.0);
    let inter_h = (inter_y2 - inter_y1).max(0.0);
    let inter = inter_w * inter_h;

    let area_a = (a[2] - a[0]).max(0.0) * (a[3] - a[1]).max(0.0);
    let area_b = (b[2] - b[0]).max(0.0) * (b[3] - b[1]).max(0.0);
    let union = area_a + area_b - inter;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// 标准 NMS: 同类别内, 按置信度降序贪心保留
fn nms(mut cands: Vec<Candidate>, iou_thr: f32) -> Vec<Candidate> {
    cands.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let mut keep: Vec<Candidate> = Vec::new();
    'outer: for c in cands.into_iter() {
        for k in keep.iter() {
            if k.class_id == c.class_id && box_iou(&k.bbox, &c.bbox) > iou_thr {
                continue 'outer;
            }
        }
        keep.push(c);
    }
    keep
}

// ========== 3. Detector (持有 ort Session) ==========

/// 全局共享的检测器实例 (模型加载耗时, 加载一次重复使用)
static DETECTOR: OnceCell<Mutex<Detector>> = OnceCell::new();

pub struct Detector {
    session: Session,
}

impl Detector {
    /// 加载模型并构建 Session
    /// 会优先尝试 CoreML EP, 失败时 ort 自动回退到 CPU
    pub fn load(model_path: &Path) -> Result<Self> {
        tracing::info!(?model_path, "加载 YOLOv8 模型");

        let mut builder = Session::builder()
            .context("创建 SessionBuilder 失败")?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .context("设置图优化级别失败")?;

        // macOS 启用 CoreML; 非 macOS 走 CPU
        #[cfg(target_os = "macos")]
        {
            builder = builder
                .with_execution_providers([CoreMLExecutionProvider::default().build()])
                .context("注册 CoreML EP 失败")?;
        }
        // 在非 macOS 平台 builder 直接保持 CPU, 以下变量用于消除 unused warning
        #[cfg(not(target_os = "macos"))]
        {
            let _ = CoreMLExecutionProvider::default();
        }

        let session = builder
            .commit_from_file(model_path)
            .with_context(|| format!("加载模型失败: {}", model_path.display()))?;

        Ok(Self { session })
    }

    /// 单图推理 (preprocess + infer + postprocess + NMS)
    pub fn detect(&mut self, img: &RgbImage) -> Result<DetectionResult> {
        let (img_w, img_h) = img.dimensions();
        let (lb_img, lb_info) = letterbox(img);
        let input = to_chw_tensor(&lb_img);

        let t0 = Instant::now();

        // 构造输入张量 (零拷贝引用)
        let input_value = TensorRef::from_array_view(input.view())
            .context("构造 ort 输入张量失败")?;

        // 推理: outputs[0] 形状 [1, 84, 8400] (4 + 80 类)
        let outputs = self
            .session
            .run(ort::inputs![input_value])
            .context("ort 推理失败")?;

        // 注意: outputs.iter().next() 链式调用会产生临时元组,
        // 必须 let-bind 让借用活到下面 shape() / view() 调用之后
        let first_pair = outputs
            .iter()
            .next()
            .context("模型无输出")?;
        let output = first_pair
            .1
            .try_extract_array::<f32>()
            .context("解析输出张量失败")?;

        let inference_ms = t0.elapsed().as_millis() as u64;

        // YOLOv8 输出布局: [batch=1, 4 + num_classes, num_anchors]
        let shape = output.shape().to_vec();
        anyhow::ensure!(
            shape.len() == 3 && shape[0] == 1,
            "YOLOv8 输出形状异常: {:?}",
            shape
        );
        let total_channels = shape[1];
        let num_anchors = shape[2];
        let num_classes = total_channels.saturating_sub(4);
        anyhow::ensure!(num_classes > 0, "YOLOv8 输出通道数 < 5: {:?}", shape);

        // 提取候选框
        let mut cands: Vec<Candidate> = Vec::with_capacity(64);
        let view = output.view();
        for a in 0..num_anchors {
            // 在 4..total_channels 中找最大类别得分
            let mut best_cls = 0usize;
            let mut best_score = 0.0f32;
            for c in 0..num_classes {
                let s = view[[0, 4 + c, a]];
                if s > best_score {
                    best_score = s;
                    best_cls = c;
                }
            }
            if best_score < CONF_THRESHOLD {
                continue;
            }
            let cx = view[[0, 0, a]];
            let cy = view[[0, 1, a]];
            let w = view[[0, 2, a]];
            let h = view[[0, 3, a]];
            let bbox = unletterbox(cx, cy, w, h, &lb_info, img_w, img_h);
            cands.push(Candidate {
                bbox,
                score: best_score,
                class_id: best_cls as u32,
            });
        }

        let kept = nms(cands, NMS_IOU_THRESHOLD);

        let detections = kept
            .into_iter()
            .map(|c| Detection {
                class_name: coco_class_name(c.class_id).to_string(),
                class_id: c.class_id,
                score: c.score,
                bbox: c.bbox,
            })
            .collect();

        Ok(DetectionResult {
            inference_ms,
            image_width: img_w,
            image_height: img_h,
            detections,
        })
    }
}

/// 获取(或惰性初始化)全局检测器
pub fn detector() -> Result<&'static Mutex<Detector>> {
    if let Some(d) = DETECTOR.get() {
        return Ok(d);
    }
    let path = crate::ai::model_path::models_dir().join(crate::ai::model_path::YOLOV8_FILENAME);
    let det = Detector::load(&path)?;
    let _ = DETECTOR.set(Mutex::new(det));
    Ok(DETECTOR.get().expect("DETECTOR 已初始化"))
}

// ========== COCO 类别 ==========

/// 我们关心的 COCO 类别 ID (DEVELOPMENT_PLAN.md §五 P0)
pub const RELEVANT_CLASSES: [u32; 4] = [2, 3, 5, 7]; // car / motorcycle / bus / truck

/// COCO 80 类的中文名查表 (P0 demo 仅翻译关心的类, 其他用英文)
fn coco_class_name(id: u32) -> &'static str {
    match id {
        0 => "person",
        1 => "bicycle",
        2 => "car (汽车)",
        3 => "motorcycle (摩托车)",
        4 => "airplane",
        5 => "bus (公交)",
        6 => "train",
        7 => "truck (卡车)",
        8 => "boat",
        9 => "traffic light",
        10 => "fire hydrant",
        11 => "stop sign",
        12 => "parking meter",
        13 => "bench",
        // 其余类别 P0 demo 不展示中文, 用类别 ID
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    #[test]
    fn letterbox_keeps_aspect_ratio() {
        // 1920x1080 横屏 -> 应缩放到 640x360, padding 在上下
        let img = RgbImage::from_pixel(1920, 1080, Rgb([10, 20, 30]));
        let (lb, info) = letterbox(&img);
        assert_eq!(lb.dimensions(), (INPUT_SIZE, INPUT_SIZE));
        // 横屏 ratio = 640/1920 = 1/3, new_h = 360, pad_y = (640-360)/2 = 140
        assert!((info.ratio - (640.0 / 1920.0)).abs() < 1e-5);
        assert!((info.pad_x - 0.0).abs() < 1.0);
        assert!((info.pad_y - 140.0).abs() < 1.0);
    }

    #[test]
    fn tensor_shape_is_chw() {
        let img = RgbImage::from_pixel(INPUT_SIZE, INPUT_SIZE, Rgb([255, 128, 0]));
        let t = to_chw_tensor(&img);
        assert_eq!(t.shape(), &[1, 3, INPUT_SIZE as usize, INPUT_SIZE as usize]);
        assert!((t[[0, 0, 0, 0]] - 1.0).abs() < 1e-6);
        assert!((t[[0, 1, 0, 0]] - 128.0 / 255.0).abs() < 1e-6);
        assert!((t[[0, 2, 0, 0]] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn nms_dedup_overlapping_same_class() {
        let cands = vec![
            Candidate {
                bbox: [0.0, 0.0, 100.0, 100.0],
                score: 0.9,
                class_id: 2,
            },
            Candidate {
                bbox: [5.0, 5.0, 95.0, 95.0],
                score: 0.8,
                class_id: 2,
            },
            // 不同类别即使大量重叠也应保留
            Candidate {
                bbox: [10.0, 10.0, 90.0, 90.0],
                score: 0.7,
                class_id: 7,
            },
        ];
        let kept = nms(cands, 0.45);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].class_id, 2);
        assert_eq!(kept[1].class_id, 7);
    }
}
