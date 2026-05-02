// Pipeline 中间产物: 每个抽样帧的车辆 + 车牌观测
// 用于 MVU 4 (车辆) -> MVU 5 (车牌) -> MVU 6 (聚合) 的数据流

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameObservation {
    /// 抽样序列中的索引 (0 开始)
    pub frame_index: usize,
    /// 距视频起点的毫秒数
    pub timestamp_ms: i64,
    /// 帧的显示尺寸 (已应用 EXIF 旋转)
    pub width: u32,
    pub height: u32,
    /// 该帧检测到的所有车辆
    pub vehicles: Vec<VehicleObservation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VehicleObservation {
    /// COCO 类别 ID (2=car / 3=motorcycle / 5=bus / 7=truck)
    pub class_id: u32,
    pub class_name: String,
    /// YOLOv8 检测置信度 [0, 1]
    pub vehicle_score: f32,
    /// 车辆框 [x1, y1, x2, y2] 像素坐标
    pub bbox: [f32; 4],
    /// 车辆掩膜 ∩ 人行道掩膜 / 车辆掩膜面积; P3 起填充
    pub iou_score: Option<f32>,
    /// 在该车上识别到的车牌, P1.MVU 4 阶段全部为 None, MVU 5 填充
    pub plate: Option<PlateReading>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlateReading {
    /// 车牌文本, 例如 "浙A12345"
    pub text: String,
    /// 字符识别置信度 [0, 1] (CTC 路径概率, 非阈值)
    pub confidence: f32,
    /// 车牌在原帧中的框 [x1, y1, x2, y2]
    pub plate_bbox: [f32; 4],
}
