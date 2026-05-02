// SegFormer-B0 人行道语义分割 (P3)
//
// 模型: nvidia/segformer-b0-finetuned-ade-512-512
//   - input "pixel_values": [1, 3, H, W] (float32, ImageNet 归一化)
//     标准 H=W=512, 不保持宽高比 resize
//   - output "logits": [1, 150, H/4, W/4]
//
// ADE20K 类别 11 = sidewalk (验证: config.json id2label["11"] == "sidewalk")
//
// pipeline 输出: 与原图同尺寸的二值 mask (0 / 255)

use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use image::{imageops::FilterType, GrayImage, RgbImage};
use ndarray::{Array, Array4};
use once_cell::sync::OnceCell;
use ort::execution_providers::CoreMLExecutionProvider;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::TensorRef;

use crate::ai::model_path::{models_dir, SEGFORMER_FILENAME};

const INPUT_SIZE: u32 = 512;
const SIDEWALK_CLASS_ID: usize = 11;
const NUM_CLASSES: usize = 150;

// ImageNet 归一化
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];

pub struct SidewalkSegmenter {
    session: Session,
    input_name: String,
}

static SEGMENTER: OnceCell<Mutex<SidewalkSegmenter>> = OnceCell::new();

impl SidewalkSegmenter {
    pub fn load(path: &Path) -> Result<Self> {
        tracing::info!(?path, "加载 SegFormer-B0 模型");
        let mut builder = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?;
        #[cfg(target_os = "macos")]
        {
            builder = builder.with_execution_providers([CoreMLExecutionProvider::default().build()])?;
        }
        let session = builder
            .commit_from_file(path)
            .with_context(|| format!("加载 SegFormer 模型失败: {}", path.display()))?;
        let input_name = session
            .inputs
            .first()
            .map(|i| i.name.clone())
            .unwrap_or_else(|| "pixel_values".to_string());
        Ok(Self { session, input_name })
    }

    /// 输出与原图同尺寸的 binary mask, 255 = sidewalk
    pub fn segment_sidewalk(&mut self, img: &RgbImage) -> Result<GrayImage> {
        let (orig_w, orig_h) = img.dimensions();

        // 1. resize 到 512x512 (不保持宽高比, SegFormer 训练时也是这么做的)
        let resized = image::imageops::resize(img, INPUT_SIZE, INPUT_SIZE, FilterType::Triangle);

        // 2. ImageNet 归一化 + CHW
        let mut tensor: Array4<f32> = Array::zeros((1, 3, INPUT_SIZE as usize, INPUT_SIZE as usize));
        for y in 0..INPUT_SIZE {
            for x in 0..INPUT_SIZE {
                let p = resized.get_pixel(x, y);
                for c in 0..3 {
                    let v = p[c] as f32 / 255.0;
                    tensor[[0, c, y as usize, x as usize]] = (v - MEAN[c]) / STD[c];
                }
            }
        }

        let input_value = TensorRef::from_array_view(tensor.view())?;
        // SegFormer ONNX 的输入名是 "pixel_values", ort::inputs! 默认按位置传递,
        // 默认就 OK; 这里保留 input_name 方便日后多输入模型
        let _ = &self.input_name;
        let outputs = self
            .session
            .run(ort::inputs![input_value])
            .context("SegFormer 推理失败")?;

        // 3. 找 logits 输出
        let first_pair = outputs
            .iter()
            .next()
            .context("SegFormer 无输出")?;
        let logits = first_pair.1.try_extract_array::<f32>()?;
        let shape = logits.shape().to_vec();
        anyhow::ensure!(
            shape.len() == 4 && shape[0] == 1 && shape[1] == NUM_CLASSES,
            "SegFormer logits 形状异常: {:?}",
            shape
        );
        let out_h = shape[2];
        let out_w = shape[3];
        let view = logits.view();

        // 4. argmax over class -> binary mask (sidewalk only) at out_h x out_w
        let mut low = GrayImage::new(out_w as u32, out_h as u32);
        for y in 0..out_h {
            for x in 0..out_w {
                let mut best_c = 0usize;
                let mut best_v = f32::NEG_INFINITY;
                for c in 0..NUM_CLASSES {
                    let v = view[[0, c, y, x]];
                    if v > best_v {
                        best_v = v;
                        best_c = c;
                    }
                }
                if best_c == SIDEWALK_CLASS_ID {
                    low.put_pixel(x as u32, y as u32, image::Luma([255]));
                }
            }
        }

        // 5. resize 到原图尺寸 (Nearest 保持二值)
        let full = image::imageops::resize(&low, orig_w, orig_h, FilterType::Nearest);
        Ok(full)
    }
}

pub fn segmenter() -> Result<&'static Mutex<SidewalkSegmenter>> {
    if let Some(s) = SEGMENTER.get() {
        return Ok(s);
    }
    let path = models_dir().join(SEGFORMER_FILENAME);
    let s = SidewalkSegmenter::load(&path)?;
    let _ = SEGMENTER.set(Mutex::new(s));
    Ok(SEGMENTER.get().expect("SEGMENTER 已初始化"))
}
