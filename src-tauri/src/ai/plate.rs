// HyperLPR3 车牌识别 (Rust port)
//
// 两个 ONNX 模型:
//   - y5fu_320x_sim.onnx: YOLOv5-based 检测器, 输入 [1,3,320,320], 输出 [1,6300,15]
//     输出列含义: 0-3=xywh, 4=obj, 5-12=四角点(P1 暂不用), 13-14=单/双行类别
//   - rpv3_mdict_160_r3.onnx: CRNN+CTC 识别器, 输入 [1,3,48,160], 输出 [1,20,78]
//
// pre/post 处理对照 site-packages/hyperlpr3 Python 源码 (Apache-2.0)

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

use crate::ai::model_path::models_dir;
use crate::models::observation::{PlateReading, VehicleObservation};

// ========== 字符表 ==========

/// HyperLPR3 字符表, 78 项 (index 0 = CTC blank)
/// 来源: site-packages/hyperlpr3/common/tokenize.py
/// Python 原表 77 项, 但模型输出 78 类; 末尾补一个占位项防越界
const TOKENS: &[&str] = &[
    // 0: blank
    "blank",
    // 1: '
    "'",
    // 2..=11: 数字
    "0", "1", "2", "3", "4", "5", "6", "7", "8", "9",
    // 12..=36: 字母 (跳过 I)
    "A", "B", "C", "D", "E", "F", "G", "H", "J", "K", "L", "M", "N", "O", "P", "Q",
    "R", "S", "T", "U", "V", "W", "X", "Y", "Z",
    // 37..=76: 中文省份与特殊
    "云", "京", "冀", "吉", "学", "宁", "川", "挂", "新", "晋", "桂", "民", "沪", "津",
    "浙", "渝", "港", "湘", "琼", "甘", "皖", "粤", "航", "苏", "蒙", "藏", "警", "豫",
    "贵", "赣", "辽", "鄂", "闽", "陕", "青", "鲁", "黑", "领", "使", "澳",
    // 77: 占位 (模型输出 78 类, 但字典只有 77 项, 兜底防越界)
    "?",
];
const NUM_CLASSES: usize = 78;

// ========== 检测器 ==========

const DET_INPUT_SIZE: u32 = 320;
const DET_CONF_THRESHOLD: f32 = 0.25;
const DET_NMS_IOU: f32 = 0.5;

#[derive(Debug, Clone)]
pub struct PlateBox {
    /// [x1, y1, x2, y2] 原图像素坐标
    pub bbox: [f32; 4],
    /// 检测置信度 = obj × max(cls)
    pub score: f32,
}

#[derive(Debug, Clone, Copy)]
struct LetterboxInfo {
    ratio: f32,
    pad_x: f32,
    pad_y: f32,
}

fn letterbox_for_detect(img: &RgbImage) -> (RgbImage, LetterboxInfo) {
    let (w, h) = img.dimensions();
    let target = DET_INPUT_SIZE as f32;
    let ratio = (target / w as f32).min(target / h as f32);
    let new_w = (w as f32 * ratio).round() as u32;
    let new_h = (h as f32 * ratio).round() as u32;
    let resized = image::imageops::resize(img, new_w, new_h, FilterType::Triangle);
    let pad_x = (DET_INPUT_SIZE - new_w) / 2;
    let pad_y = (DET_INPUT_SIZE - new_h) / 2;
    // HyperLPR3 用 (0, 0, 0) padding (黑边), 与 YOLOv8 的 (114, 114, 114) 不同
    let mut canvas = RgbImage::from_pixel(DET_INPUT_SIZE, DET_INPUT_SIZE, image::Rgb([0, 0, 0]));
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

fn detect_to_chw(img: &RgbImage) -> Array4<f32> {
    let (w, h) = img.dimensions();
    let mut t = Array::zeros((1, 3, h as usize, w as usize));
    for y in 0..h {
        for x in 0..w {
            let p = img.get_pixel(x, y);
            t[[0, 0, y as usize, x as usize]] = p[0] as f32 / 255.0;
            t[[0, 1, y as usize, x as usize]] = p[1] as f32 / 255.0;
            t[[0, 2, y as usize, x as usize]] = p[2] as f32 / 255.0;
        }
    }
    t
}

fn unletterbox(xyxy: [f32; 4], info: LetterboxInfo, img_w: u32, img_h: u32) -> [f32; 4] {
    let inv = 1.0 / info.ratio;
    let x1 = ((xyxy[0] - info.pad_x) * inv).clamp(0.0, img_w as f32);
    let y1 = ((xyxy[1] - info.pad_y) * inv).clamp(0.0, img_h as f32);
    let x2 = ((xyxy[2] - info.pad_x) * inv).clamp(0.0, img_w as f32);
    let y2 = ((xyxy[3] - info.pad_y) * inv).clamp(0.0, img_h as f32);
    [x1, y1, x2, y2]
}

fn iou(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let inter_x1 = a[0].max(b[0]);
    let inter_y1 = a[1].max(b[1]);
    let inter_x2 = a[2].min(b[2]);
    let inter_y2 = a[3].min(b[3]);
    let iw = (inter_x2 - inter_x1).max(0.0);
    let ih = (inter_y2 - inter_y1).max(0.0);
    let inter = iw * ih;
    let area_a = (a[2] - a[0]).max(0.0) * (a[3] - a[1]).max(0.0);
    let area_b = (b[2] - b[0]).max(0.0) * (b[3] - b[1]).max(0.0);
    let union = area_a + area_b - inter;
    if union <= 0.0 { 0.0 } else { inter / union }
}

fn nms_class_agnostic(mut boxes: Vec<PlateBox>, iou_thr: f32) -> Vec<PlateBox> {
    boxes.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let mut keep: Vec<PlateBox> = Vec::new();
    'outer: for c in boxes.into_iter() {
        for k in keep.iter() {
            if iou(&k.bbox, &c.bbox) > iou_thr {
                continue 'outer;
            }
        }
        keep.push(c);
    }
    keep
}

pub struct PlateDetector {
    session: Session,
}

impl PlateDetector {
    fn load(path: &Path) -> Result<Self> {
        tracing::info!(?path, "加载 HyperLPR3 车牌检测器");
        let mut builder = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?;
        #[cfg(target_os = "macos")]
        {
            builder = builder.with_execution_providers([CoreMLExecutionProvider::default().build()])?;
        }
        let session = builder
            .commit_from_file(path)
            .with_context(|| format!("加载车牌检测模型失败: {}", path.display()))?;
        Ok(Self { session })
    }

    fn detect(&mut self, img: &RgbImage) -> Result<Vec<PlateBox>> {
        let (img_w, img_h) = img.dimensions();
        let (lb, info) = letterbox_for_detect(img);
        let input = detect_to_chw(&lb);
        let input_value = TensorRef::from_array_view(input.view())?;

        let outputs = self.session.run(ort::inputs![input_value])?;
        let first = outputs.iter().next().context("车牌检测器无输出")?;
        let view = first.1.try_extract_array::<f32>()?;
        let shape = view.shape().to_vec();
        anyhow::ensure!(
            shape.len() == 3 && shape[0] == 1 && shape[2] == 15,
            "车牌检测输出形状异常: {:?}",
            shape
        );
        let num_anchors = shape[1];
        let v = view.view();

        let mut cands: Vec<PlateBox> = Vec::with_capacity(32);
        for a in 0..num_anchors {
            let obj = v[[0, a, 4]];
            if obj < DET_CONF_THRESHOLD {
                continue;
            }
            // class_scores 13/14 (单/双行) 各乘以 obj
            let cls0 = v[[0, a, 13]] * obj;
            let cls1 = v[[0, a, 14]] * obj;
            let max_cls = cls0.max(cls1);
            let cx = v[[0, a, 0]];
            let cy = v[[0, a, 1]];
            let w = v[[0, a, 2]];
            let h = v[[0, a, 3]];
            let xyxy = [
                cx - w / 2.0,
                cy - h / 2.0,
                cx + w / 2.0,
                cy + h / 2.0,
            ];
            let bbox = unletterbox(xyxy, info, img_w, img_h);
            cands.push(PlateBox { bbox, score: max_cls });
        }
        Ok(nms_class_agnostic(cands, DET_NMS_IOU))
    }
}

// ========== 识别器 ==========

const REC_INPUT_H: u32 = 48;
const REC_INPUT_W: u32 = 160;

pub struct PlateRecognizer {
    session: Session,
}

impl PlateRecognizer {
    fn load(path: &Path) -> Result<Self> {
        tracing::info!(?path, "加载 HyperLPR3 车牌识别器");
        let mut builder = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?;
        #[cfg(target_os = "macos")]
        {
            builder = builder.with_execution_providers([CoreMLExecutionProvider::default().build()])?;
        }
        let session = builder
            .commit_from_file(path)
            .with_context(|| format!("加载车牌识别模型失败: {}", path.display()))?;
        Ok(Self { session })
    }

    /// 识别单张已裁剪的车牌图, 返回 (text, confidence)
    fn recognize(&mut self, plate_img: &RgbImage) -> Result<(String, f32)> {
        let (orig_w, orig_h) = plate_img.dimensions();
        // HyperLPR3 encode_images: 保持高=48, 宽按比例 cap 在 [48, 160]
        let target_h = REC_INPUT_H;
        let target_w = REC_INPUT_W;
        let ratio = orig_w as f32 / orig_h as f32;
        let resized_w_f = (target_h as f32 * ratio).ceil();
        let resized_w = (resized_w_f.round() as u32).clamp(48, target_w);

        let resized = image::imageops::resize(plate_img, resized_w, target_h, FilterType::Triangle);

        // CHW + normalize 到 [-1,1] (-127.5)/127.5; 右侧补零至 160
        let mut t: Array4<f32> = Array::zeros((1, 3, target_h as usize, target_w as usize));
        for y in 0..target_h {
            for x in 0..resized_w {
                let p = resized.get_pixel(x, y);
                t[[0, 0, y as usize, x as usize]] = (p[0] as f32 - 127.5) / 127.5;
                t[[0, 1, y as usize, x as usize]] = (p[1] as f32 - 127.5) / 127.5;
                t[[0, 2, y as usize, x as usize]] = (p[2] as f32 - 127.5) / 127.5;
            }
        }

        let input_value = TensorRef::from_array_view(t.view())?;
        let outputs = self.session.run(ort::inputs![input_value])?;
        let first = outputs.iter().next().context("车牌识别器无输出")?;
        let view = first.1.try_extract_array::<f32>()?;
        let shape = view.shape().to_vec();
        anyhow::ensure!(
            shape.len() == 3 && shape[0] == 1 && shape[2] == NUM_CLASSES,
            "车牌识别输出形状异常: {:?}, 期待 [1, T, {NUM_CLASSES}]",
            shape
        );
        let t_steps = shape[1];
        let v = view.view();

        // 每个时间步取 argmax
        let mut indices: Vec<usize> = Vec::with_capacity(t_steps);
        let mut max_probs: Vec<f32> = Vec::with_capacity(t_steps);
        for ti in 0..t_steps {
            let mut max_idx = 0usize;
            let mut max_val = f32::NEG_INFINITY;
            for c in 0..NUM_CLASSES {
                let val = v[[0, ti, c]];
                if val > max_val {
                    max_val = val;
                    max_idx = c;
                }
            }
            indices.push(max_idx);
            max_probs.push(max_val);
        }

        // CTC greedy: 折叠连续重复 + 移除 blank(0)
        let mut chars: Vec<&str> = Vec::new();
        let mut confs: Vec<f32> = Vec::new();
        let mut prev: Option<usize> = None;
        for (i, &idx) in indices.iter().enumerate() {
            if Some(idx) == prev {
                prev = Some(idx);
                continue;
            }
            prev = Some(idx);
            if idx == 0 {
                continue;
            }
            if idx >= TOKENS.len() {
                continue;
            }
            chars.push(TOKENS[idx]);
            confs.push(max_probs[i]);
        }
        let text: String = chars.join("");
        let confidence = if confs.is_empty() {
            0.0
        } else {
            confs.iter().sum::<f32>() / confs.len() as f32
        };
        Ok((text, confidence))
    }
}

// ========== 全局实例 ==========

struct PlatePipeline {
    detector: PlateDetector,
    recognizer: PlateRecognizer,
}

static PIPELINE: OnceCell<Mutex<PlatePipeline>> = OnceCell::new();

pub const DETECTOR_FILENAME: &str = "hyperlpr3/y5fu_320x_sim.onnx";
pub const RECOGNIZER_FILENAME: &str = "hyperlpr3/rpv3_mdict_160_r3.onnx";

fn pipeline() -> Result<&'static Mutex<PlatePipeline>> {
    if let Some(p) = PIPELINE.get() {
        return Ok(p);
    }
    let det_path = models_dir().join(DETECTOR_FILENAME);
    let rec_path = models_dir().join(RECOGNIZER_FILENAME);
    let detector = PlateDetector::load(&det_path)?;
    let recognizer = PlateRecognizer::load(&rec_path)?;
    let _ = PIPELINE.set(Mutex::new(PlatePipeline { detector, recognizer }));
    Ok(PIPELINE.get().expect("PIPELINE 已初始化"))
}

// ========== 中国车牌格式校验 ==========

/// 中国大陆车牌首位合法汉字 (省/直辖市/自治区简称, 31 个)
const VALID_PROVINCES: &[char] = &[
    '京', '津', '沪', '渝', // 直辖市
    '冀', '晋', '辽', '吉', '黑', '苏', '浙', '皖', '闽', '赣', '鲁',
    '豫', '鄂', '湘', '粤', '桂', '琼', '川', '贵', '云', '陕', '甘', '青',
    '蒙', '宁', '新', '藏', // 5 自治区
];

/// 末位特殊字符 (警车 / 教练车 / 港澳进入内地 / 大使馆领馆)
const VALID_SUFFIXES: &[char] = &['警', '学', '港', '澳', '领', '使'];

/// 校验车牌是否符合中国大陆格式
///
/// 规范 (DEVELOPMENT_PLAN.md §三):
///   - 长度 7 或 8
///   - 首位: 省份汉字 (31 个之一)
///   - 第二位: A-Z 字母 (排除 I/O, 中国车牌不用)
///   - 后续位: 字母数字 (同样排除 I/O), 末位可选 警/学/港/澳/领/使
///
/// 不严格区分蓝牌/绿牌/警车细节, 只做结构校验. 用于过滤 OCR 乱码识别.
pub fn is_valid_chinese_plate(s: &str) -> bool {
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    if n < 7 || n > 8 {
        return false;
    }
    // 首位省份
    if !VALID_PROVINCES.contains(&chars[0]) {
        return false;
    }
    // 第二位 A-Z (不含 I/O)
    let c1 = chars[1];
    if !(c1.is_ascii_uppercase() && c1 != 'I' && c1 != 'O') {
        return false;
    }
    // 末位单独看: 如果是特殊后缀, body 长度 -1
    let last = *chars.last().unwrap();
    let body_end = if VALID_SUFFIXES.contains(&last) { n - 1 } else { n };
    if body_end < 6 {
        // 至少 4 位车牌主体 + 第二位字母 + 首位省份 = 6 位 (7 位车牌, 末位特殊后缀的不算)
        return false;
    }
    // body 中间位 (第 2-?): 字母数字, 不含 I/O
    for i in 2..body_end {
        let c = chars[i];
        let ok = (c.is_ascii_uppercase() || c.is_ascii_digit()) && c != 'I' && c != 'O';
        if !ok {
            return false;
        }
    }
    true
}

// ========== 公共 API ==========

/// 单图检测 + 识别 (供 demo 命令使用)
pub fn detect_and_recognize(img: &RgbImage) -> Result<Vec<PlateReading>> {
    let pl = pipeline()?;
    let mut p = pl.lock().map_err(|e| anyhow::anyhow!("Plate mutex 中毒: {e}"))?;
    let t0 = Instant::now();
    let plate_boxes = p.detector.detect(img)?;
    tracing::debug!(plate_count = plate_boxes.len(), ms = t0.elapsed().as_millis() as u64, "车牌检测完成");

    let mut results = Vec::with_capacity(plate_boxes.len());
    for pb in plate_boxes {
        let crop = crop_with_padding(img, &pb.bbox, 0.05);
        if crop.width() < 8 || crop.height() < 8 {
            continue;
        }
        match p.recognizer.recognize(&crop) {
            Ok((text, confidence)) if !text.is_empty() => {
                results.push(PlateReading {
                    text,
                    confidence,
                    plate_bbox: pb.bbox,
                });
            }
            Ok(_) => {} // 空文本丢弃
            Err(e) => tracing::warn!(error = %e, "单张车牌识别失败, 跳过"),
        }
    }
    Ok(results)
}

/// 把帧上识别到的车牌, 按"被哪个车辆框包住"的关系, 写到对应 vehicle.plate
///
/// 匹配规则: 车牌中心点落在车辆 bbox 内, 选 vehicle_score 最高的那辆;
/// 找不到则忽略该车牌 (P1 简化, 不创建"无主"车牌事件)
pub fn recognize_into(img: &RgbImage, vehicles: &mut [VehicleObservation]) -> Result<()> {
    if vehicles.is_empty() {
        return Ok(());
    }
    let plates = detect_and_recognize(img)?;
    for plate in plates {
        let cx = (plate.plate_bbox[0] + plate.plate_bbox[2]) / 2.0;
        let cy = (plate.plate_bbox[1] + plate.plate_bbox[3]) / 2.0;

        let mut best_idx: Option<usize> = None;
        let mut best_score: f32 = -1.0;
        for (i, v) in vehicles.iter().enumerate() {
            if cx >= v.bbox[0] && cx <= v.bbox[2] && cy >= v.bbox[1] && cy <= v.bbox[3] {
                if v.vehicle_score > best_score {
                    best_score = v.vehicle_score;
                    best_idx = Some(i);
                }
            }
        }
        if let Some(i) = best_idx {
            // 多车牌匹配同一辆车时, 保留置信度更高的
            let keep = match &vehicles[i].plate {
                Some(existing) => plate.confidence > existing.confidence,
                None => true,
            };
            if keep {
                vehicles[i].plate = Some(plate);
            }
        }
    }
    Ok(())
}

/// 按 bbox 加边距裁剪原帧
fn crop_with_padding(img: &RgbImage, bbox: &[f32; 4], pad_ratio: f32) -> RgbImage {
    let (w, h) = img.dimensions();
    let bw = (bbox[2] - bbox[0]).max(1.0);
    let bh = (bbox[3] - bbox[1]).max(1.0);
    let pad_x = bw * pad_ratio;
    let pad_y = bh * pad_ratio;
    let x1 = ((bbox[0] - pad_x).max(0.0)) as u32;
    let y1 = ((bbox[1] - pad_y).max(0.0)) as u32;
    let x2 = ((bbox[2] + pad_x).min(w as f32 - 1.0)) as u32;
    let y2 = ((bbox[3] + pad_y).min(h as f32 - 1.0)) as u32;
    let cw = x2.saturating_sub(x1).max(1);
    let ch = y2.saturating_sub(y1).max(1);
    image::imageops::crop_imm(img, x1, y1, cw, ch).to_image()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_table_size() {
        assert_eq!(TOKENS.len(), NUM_CLASSES);
        assert_eq!(TOKENS[0], "blank");
        assert_eq!(TOKENS[1], "'");
        // 数字
        assert_eq!(TOKENS[2], "0");
        assert_eq!(TOKENS[11], "9");
        // 字母 (跳过 I)
        assert_eq!(TOKENS[12], "A");
        assert_eq!(TOKENS[19], "H");
        assert_eq!(TOKENS[20], "J");
        // 浙
        assert!(TOKENS.iter().any(|&t| t == "浙"));
    }

    #[test]
    fn valid_chinese_plate_accepts_common_forms() {
        // 普通蓝牌 7 位
        assert!(is_valid_chinese_plate("浙A12345"));
        assert!(is_valid_chinese_plate("京A88888"));
        assert!(is_valid_chinese_plate("粤B7H3K9"));
        // 新能源 8 位
        assert!(is_valid_chinese_plate("浙AD12345"));
        assert!(is_valid_chinese_plate("沪BD00001"));
        // 警车 7 位 (末位 警)
        assert!(is_valid_chinese_plate("浙A1234警"));
        // 教练 7 位
        assert!(is_valid_chinese_plate("京A1234学"));
        // 港澳粤 Z
        assert!(is_valid_chinese_plate("粤Z1234港"));
        assert!(is_valid_chinese_plate("粤Z1234澳"));
    }

    #[test]
    fn valid_chinese_plate_rejects_garbage() {
        // <待确认> 占位符
        assert!(!is_valid_chinese_plate("<待确认>"));
        // 空 / 太短
        assert!(!is_valid_chinese_plate(""));
        assert!(!is_valid_chinese_plate("浙A"));
        assert!(!is_valid_chinese_plate("浙A123"));
        // 太长 (> 8)
        assert!(!is_valid_chinese_plate("浙A1234567"));
        // 首位不是省份
        assert!(!is_valid_chinese_plate("AAA1234"));
        assert!(!is_valid_chinese_plate("挂A12345"));
        assert!(!is_valid_chinese_plate("学A12345"));
        // 第二位不是字母
        assert!(!is_valid_chinese_plate("浙112345"));
        // 第二位 I/O
        assert!(!is_valid_chinese_plate("浙I12345"));
        assert!(!is_valid_chinese_plate("浙O12345"));
        // 含小写字母
        assert!(!is_valid_chinese_plate("浙a12345"));
        // 含特殊符号
        assert!(!is_valid_chinese_plate("浙A1-345"));
        assert!(!is_valid_chinese_plate("浙A1.345"));
        // 中间位含 I/O
        assert!(!is_valid_chinese_plate("浙A1I345"));
        assert!(!is_valid_chinese_plate("浙AO2345"));
    }

    #[test]
    fn ctc_decode_collapse_and_blank() {
        // 模拟 CTC 输出: blank, 浙, 浙, blank, A, A, blank, 1, 2
        // 期望文本 "浙A12"
        let zhe_idx = TOKENS.iter().position(|&t| t == "浙").unwrap();
        let a_idx = TOKENS.iter().position(|&t| t == "A").unwrap();
        let one_idx = TOKENS.iter().position(|&t| t == "1").unwrap();
        let two_idx = TOKENS.iter().position(|&t| t == "2").unwrap();

        let indices = vec![0, zhe_idx, zhe_idx, 0, a_idx, a_idx, 0, one_idx, two_idx];
        let probs: Vec<f32> = vec![0.5, 0.9, 0.85, 0.5, 0.95, 0.9, 0.5, 0.92, 0.88];
        let mut chars: Vec<&str> = Vec::new();
        let mut confs: Vec<f32> = Vec::new();
        let mut prev: Option<usize> = None;
        for (i, &idx) in indices.iter().enumerate() {
            if Some(idx) == prev {
                prev = Some(idx);
                continue;
            }
            prev = Some(idx);
            if idx == 0 || idx >= TOKENS.len() {
                continue;
            }
            chars.push(TOKENS[idx]);
            confs.push(probs[i]);
        }
        assert_eq!(chars.join(""), "浙A12");
    }
}
