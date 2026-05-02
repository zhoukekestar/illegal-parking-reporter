// YOLOv8-seg 车辆检测推理流水线 (P3 升级, 含掩膜)
//
// 模型输出 (yolov8n-seg.onnx):
//   - output0: [1, 116, 8400]
//       cols 0..4   = cx, cy, w, h (640 input space)
//       cols 4..84  = 80 类得分
//       cols 84..116 = 32 个 mask 系数
//   - output1: [1, 32, 160, 160] mask 原型
//
// 单个检测的二值 mask 重建:
//   logits[h, w] = sum_c coeffs[c] * proto[c, h, w]   for h,w in 160x160
//   mask[h, w]   = sigmoid(logits[h, w]) > 0.5
//   bbox crop (160-space)
//   resize 160 -> 640 (Nearest)
//   反 letterbox -> 原图尺寸

use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

use anyhow::{Context, Result};
use image::{imageops::FilterType, GrayImage, RgbImage};
use ndarray::{Array, Array4, ArrayView3};
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
/// 我们关心的 COCO 类别 ID (DEVELOPMENT_PLAN.md §五 P0)
pub const RELEVANT_CLASSES: [u32; 4] = [2, 3, 5, 7]; // car / motorcycle / bus / truck

const MASK_PROTO_SIZE: u32 = 160;
const MASK_PROTO_CHANNELS: usize = 32;

// ========== Letterbox ==========

#[derive(Debug, Clone, Copy)]
struct LetterboxInfo {
    ratio: f32,
    pad_x: f32,
    pad_y: f32,
}

fn letterbox(img: &RgbImage) -> (RgbImage, LetterboxInfo) {
    let (w, h) = img.dimensions();
    let target = INPUT_SIZE as f32;
    let ratio = (target / w as f32).min(target / h as f32);
    let new_w = (w as f32 * ratio).round() as u32;
    let new_h = (h as f32 * ratio).round() as u32;
    let resized = image::imageops::resize(img, new_w, new_h, FilterType::Triangle);
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

fn unletterbox_xyxy(
    xyxy: [f32; 4],
    info: &LetterboxInfo,
    img_w: u32,
    img_h: u32,
) -> [f32; 4] {
    let inv = 1.0 / info.ratio;
    let x1 = ((xyxy[0] - info.pad_x) * inv).clamp(0.0, img_w as f32);
    let y1 = ((xyxy[1] - info.pad_y) * inv).clamp(0.0, img_h as f32);
    let x2 = ((xyxy[2] - info.pad_x) * inv).clamp(0.0, img_w as f32);
    let y2 = ((xyxy[3] - info.pad_y) * inv).clamp(0.0, img_h as f32);
    [x1, y1, x2, y2]
}

// ========== NMS ==========

#[derive(Debug, Clone)]
struct Candidate {
    /// (cx, cy, w, h) in 640 input space (用于 mask crop)
    cxywh_input: [f32; 4],
    /// xyxy in 原图坐标 (用于输出)
    bbox_orig: [f32; 4],
    score: f32,
    class_id: u32,
    mask_coeffs: [f32; MASK_PROTO_CHANNELS],
}

fn box_iou_xyxy(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let ix1 = a[0].max(b[0]);
    let iy1 = a[1].max(b[1]);
    let ix2 = a[2].min(b[2]);
    let iy2 = a[3].min(b[3]);
    let iw = (ix2 - ix1).max(0.0);
    let ih = (iy2 - iy1).max(0.0);
    let inter = iw * ih;
    let area_a = (a[2] - a[0]).max(0.0) * (a[3] - a[1]).max(0.0);
    let area_b = (b[2] - b[0]).max(0.0) * (b[3] - b[1]).max(0.0);
    let union = area_a + area_b - inter;
    if union <= 0.0 { 0.0 } else { inter / union }
}

fn nms(mut cands: Vec<Candidate>, iou_thr: f32) -> Vec<Candidate> {
    cands.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut keep: Vec<Candidate> = Vec::new();
    'outer: for c in cands.into_iter() {
        for k in keep.iter() {
            if k.class_id == c.class_id && box_iou_xyxy(&k.bbox_orig, &c.bbox_orig) > iou_thr {
                continue 'outer;
            }
        }
        keep.push(c);
    }
    keep
}

// ========== Mask 重建 ==========

/// 给定一个检测的 mask 系数 + bbox, 输出原图尺寸的二值 mask (0/255)
fn build_mask_for_detection(
    coeffs: &[f32; MASK_PROTO_CHANNELS],
    prototypes: &ArrayView3<f32>, // [32, 160, 160]
    cxywh_input: &[f32; 4],       // 640 space
    info: &LetterboxInfo,
    img_w: u32,
    img_h: u32,
) -> GrayImage {
    let proto_h = MASK_PROTO_SIZE as usize;
    let proto_w = MASK_PROTO_SIZE as usize;
    let scale = INPUT_SIZE as f32 / MASK_PROTO_SIZE as f32; // 640/160 = 4

    // bbox in proto-space (160x160)
    let cx = cxywh_input[0] / scale;
    let cy = cxywh_input[1] / scale;
    let w = cxywh_input[2] / scale;
    let h = cxywh_input[3] / scale;
    let bx1 = (cx - w / 2.0).max(0.0);
    let by1 = (cy - h / 2.0).max(0.0);
    let bx2 = (cx + w / 2.0).min(proto_w as f32);
    let by2 = (cy + h / 2.0).min(proto_h as f32);

    let mut mask_proto = GrayImage::new(proto_w as u32, proto_h as u32);

    for h_i in 0..proto_h {
        let hf = h_i as f32;
        if hf < by1 - 0.5 || hf > by2 + 0.5 {
            continue;
        }
        for w_i in 0..proto_w {
            let wf = w_i as f32;
            if wf < bx1 - 0.5 || wf > bx2 + 0.5 {
                continue;
            }
            let mut sum: f32 = 0.0;
            for c in 0..MASK_PROTO_CHANNELS {
                sum += coeffs[c] * prototypes[[c, h_i, w_i]];
            }
            // sigmoid > 0.5  等价于  sum > 0
            if sum > 0.0 {
                mask_proto.put_pixel(w_i as u32, h_i as u32, image::Luma([255]));
            }
        }
    }

    // 160 -> 640
    let mask_input = image::imageops::resize(
        &mask_proto,
        INPUT_SIZE,
        INPUT_SIZE,
        FilterType::Nearest,
    );

    // 反 letterbox: 取出非 padding 区域, 再 resize 到原图
    let pad_x = info.pad_x as u32;
    let pad_y = info.pad_y as u32;
    let valid_w = INPUT_SIZE.saturating_sub(2 * pad_x).max(1);
    let valid_h = INPUT_SIZE.saturating_sub(2 * pad_y).max(1);

    let cropped = image::imageops::crop_imm(&mask_input, pad_x, pad_y, valid_w, valid_h).to_image();
    image::imageops::resize(&cropped, img_w, img_h, FilterType::Nearest)
}

// ========== Detector ==========

pub struct Detector {
    session: Session,
}

static DETECTOR: OnceCell<Mutex<Detector>> = OnceCell::new();

impl Detector {
    pub fn load(model_path: &Path) -> Result<Self> {
        tracing::info!(?model_path, "加载 YOLOv8-seg 模型");
        let mut builder = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?;
        #[cfg(target_os = "macos")]
        {
            builder = builder.with_execution_providers([CoreMLExecutionProvider::default().build()])?;
        }
        let session = builder
            .commit_from_file(model_path)
            .with_context(|| format!("加载模型失败: {}", model_path.display()))?;
        Ok(Self { session })
    }

    pub fn detect(&mut self, img: &RgbImage) -> Result<DetectionResult> {
        let (img_w, img_h) = img.dimensions();
        let (lb_img, lb_info) = letterbox(img);
        let input = to_chw_tensor(&lb_img);
        let input_value = TensorRef::from_array_view(input.view())?;

        let t0 = Instant::now();
        let outputs = self
            .session
            .run(ort::inputs![input_value])
            .context("ort 推理失败")?;

        // 收集两个输出 (按名字, 顺序不保证)
        let mut output0: Option<ndarray::ArrayD<f32>> = None;
        let mut output1: Option<ndarray::ArrayD<f32>> = None;
        for (name, val) in outputs.iter() {
            let arr = val.try_extract_array::<f32>()?;
            match name.as_ref() {
                "output0" => output0 = Some(arr.view().to_owned()),
                "output1" => output1 = Some(arr.view().to_owned()),
                _ => {}
            }
        }
        let output0 = output0.context("找不到 output0")?;
        let output1 = output1.context("找不到 output1")?;
        let inference_ms = t0.elapsed().as_millis() as u64;

        let s0 = output0.shape().to_vec();
        let s1 = output1.shape().to_vec();
        anyhow::ensure!(
            s0.len() == 3 && s0[0] == 1 && s0[1] == 116,
            "output0 形状异常: {:?}",
            s0
        );
        anyhow::ensure!(
            s1.len() == 4 && s1[0] == 1 && s1[1] == MASK_PROTO_CHANNELS,
            "output1 形状异常: {:?}",
            s1
        );
        let num_anchors = s0[2];
        let num_classes = 80usize;
        let v = output0.view();
        let prototypes_dyn = output1.view();
        let prototypes = prototypes_dyn
            .into_shape_with_order((MASK_PROTO_CHANNELS, MASK_PROTO_SIZE as usize, MASK_PROTO_SIZE as usize))
            .context("output1 reshape 失败")?
            .into_dimensionality::<ndarray::Ix3>()
            .context("output1 转 Ix3 失败")?;

        // 候选框
        let mut cands: Vec<Candidate> = Vec::new();
        for a in 0..num_anchors {
            // 类别 argmax
            let mut best_cls = 0usize;
            let mut best_score = 0.0f32;
            for c in 0..num_classes {
                let s = v[[0, 4 + c, a]];
                if s > best_score {
                    best_score = s;
                    best_cls = c;
                }
            }
            if best_score < CONF_THRESHOLD {
                continue;
            }
            let cx = v[[0, 0, a]];
            let cy = v[[0, 1, a]];
            let w = v[[0, 2, a]];
            let h = v[[0, 3, a]];
            let xyxy_input = [cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0];
            let bbox_orig = unletterbox_xyxy(xyxy_input, &lb_info, img_w, img_h);

            let mut coeffs = [0.0f32; MASK_PROTO_CHANNELS];
            for c in 0..MASK_PROTO_CHANNELS {
                coeffs[c] = v[[0, 4 + 80 + c, a]];
            }

            cands.push(Candidate {
                cxywh_input: [cx, cy, w, h],
                bbox_orig,
                score: best_score,
                class_id: best_cls as u32,
                mask_coeffs: coeffs,
            });
        }

        let kept = nms(cands, NMS_IOU_THRESHOLD);

        // 重建 mask + 输出 Detection
        let mut detections: Vec<Detection> = Vec::with_capacity(kept.len());
        let mut masks: Vec<Option<GrayImage>> = Vec::with_capacity(kept.len());

        for c in kept.into_iter() {
            // 只为我们关心的类别构造 mask (省时); 其他类别保留 bbox 但不算 mask
            let mask = if RELEVANT_CLASSES.contains(&c.class_id) {
                Some(build_mask_for_detection(
                    &c.mask_coeffs,
                    &prototypes,
                    &c.cxywh_input,
                    &lb_info,
                    img_w,
                    img_h,
                ))
            } else {
                None
            };
            detections.push(Detection {
                class_id: c.class_id,
                class_name: coco_class_name(c.class_id).to_string(),
                score: c.score,
                bbox: c.bbox_orig,
            });
            masks.push(mask);
        }

        Ok(DetectionResult {
            inference_ms,
            image_width: img_w,
            image_height: img_h,
            detections,
            masks,
        })
    }
}

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
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    #[test]
    fn letterbox_keeps_aspect_ratio() {
        let img = RgbImage::from_pixel(1920, 1080, Rgb([10, 20, 30]));
        let (lb, info) = letterbox(&img);
        assert_eq!(lb.dimensions(), (INPUT_SIZE, INPUT_SIZE));
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
        let mk = |bbox: [f32; 4], score: f32, cls: u32| Candidate {
            cxywh_input: [0.0; 4],
            bbox_orig: bbox,
            score,
            class_id: cls,
            mask_coeffs: [0.0; MASK_PROTO_CHANNELS],
        };
        let cands = vec![
            mk([0.0, 0.0, 100.0, 100.0], 0.9, 2),
            mk([5.0, 5.0, 95.0, 95.0], 0.8, 2),
            mk([10.0, 10.0, 90.0, 90.0], 0.7, 7),
        ];
        let kept = nms(cands, 0.45);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].class_id, 2);
        assert_eq!(kept[1].class_id, 7);
    }

    #[test]
    fn build_mask_uses_bbox_crop_and_sigmoid() {
        // 构造合成 prototypes: 全 1, coeffs[0]=1 其他 0 -> logits=1 > 0 -> mask 全 1 (within bbox)
        let proto: ndarray::Array3<f32> = ndarray::Array3::from_elem(
            (MASK_PROTO_CHANNELS, MASK_PROTO_SIZE as usize, MASK_PROTO_SIZE as usize),
            0.0,
        );
        let mut proto = proto;
        // 让 channel 0 全 1
        proto.slice_mut(ndarray::s![0, .., ..]).fill(1.0);
        let mut coeffs = [0.0f32; MASK_PROTO_CHANNELS];
        coeffs[0] = 2.0;

        // bbox 在 640 空间是 [0, 0, 320, 320] (= [0, 0, 80, 80] in 160 proto space)
        // 所以 mask 在原图左上 1/4 是车辆
        let info = LetterboxInfo {
            ratio: 1.0,
            pad_x: 0.0,
            pad_y: 0.0,
        };
        let cxywh = [160.0, 160.0, 320.0, 320.0];
        let mask = build_mask_for_detection(&coeffs, &proto.view(), &cxywh, &info, 640, 640);
        assert_eq!(mask.dimensions(), (640, 640));
        // 中心点 (160, 160) 应在 mask 内
        assert!(mask.get_pixel(160, 160)[0] > 127);
        // 远处 (500, 500) 应在 bbox 外
        assert_eq!(mask.get_pixel(500, 500)[0], 0);
    }
}
