use serde::{Deserialize, Serialize};

/// YOLOv8 单个检测框 (P0 demo 输出格式)
///
/// 坐标系为原始图像像素 (已经过 letterbox 反变换)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detection {
    /// COCO 类别 ID (0-79), 例如 car=2 / truck=7
    pub class_id: u32,
    /// 类别中文名 (P0 只翻译我们关心的几类, 其他用英文原名)
    pub class_name: String,
    /// 置信度 [0, 1]
    pub score: f32,
    /// 边界框 [x1, y1, x2, y2], 原始图像像素坐标
    pub bbox: [f32; 4],
}

/// 单次检测调用的整体结果
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DetectionResult {
    /// 推理总耗时 (毫秒, 不含图片解码)
    pub inference_ms: u64,
    /// 原始图片宽度
    pub image_width: u32,
    /// 原始图片高度
    pub image_height: u32,
    /// 所有保留的检测框 (已 NMS, 按置信度降序)
    pub detections: Vec<Detection>,
    /// 与 detections 一一对应的二值掩膜 (P3 起 yolov8n-seg 输出),
    /// 像素值: 0 (背景) / 255 (车辆); 与原图同尺寸
    /// 不序列化到前端 (体积大且前端用不上)
    #[serde(skip)]
    pub masks: Vec<Option<image::GrayImage>>,
}
