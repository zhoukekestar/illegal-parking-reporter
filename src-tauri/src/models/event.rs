// 单个违停事件 (MVU 6 聚合输出, MVU 7 持久化, MVU 8 端到端返回)
//
// 完整字段对照 DEVELOPMENT_PLAN.md §五 P1 数据模型,
// P1 阶段 iou_score / snapshot_path / clip_path 都是 None, P3 才填

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewStatus {
    Pending,
    Accepted,
    Rejected,
    Deferred,
}

impl Default for ReviewStatus {
    fn default() -> Self {
        ReviewStatus::Pending
    }
}

impl ReviewStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ReviewStatus::Pending => "pending",
            ReviewStatus::Accepted => "accepted",
            ReviewStatus::Rejected => "rejected",
            ReviewStatus::Deferred => "deferred",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "accepted" => Some(Self::Accepted),
            "rejected" => Some(Self::Rejected),
            "deferred" => Some(Self::Deferred),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParkingEvent {
    /// UUID v4
    pub id: String,
    /// 源视频文件 (绝对路径)
    pub source_video: String,
    /// 代表帧在抽样序列中的索引 (置信度最高的那一帧)
    pub representative_frame_index: usize,
    /// 代表帧距视频起点的毫秒数
    pub timestamp_ms: i64,
    /// 整体拍摄时间 (creation_time + timestamp), MVU 6 计算, ISO 8601
    pub event_time: Option<String>,
    /// 车牌号 (优先取人工修正值, 没有则用 OCR 值)
    pub plate_number: String,
    /// 车牌识别置信度
    pub plate_confidence: f32,
    /// 人工修正过的车牌 (None 表示未修正)
    pub plate_manual_corrected: Option<String>,
    /// 车辆 YOLOv8 类别名 (中英对照, e.g. "car (汽车)")
    pub vehicle_class: String,
    /// 车辆框 [x1, y1, x2, y2]
    pub vehicle_bbox: [f32; 4],
    /// 同一事件首次/末次出现的毫秒时间 (聚合窗口内)
    pub first_seen_ms: i64,
    pub last_seen_ms: i64,
    /// 该事件聚合了多少帧观测 (P1 用作"稳定度"指标)
    pub frame_hits: u32,
    /// 审核状态
    pub review_status: ReviewStatus,
    /// 占位字段, P3 起填充
    pub iou_score: Option<f32>,
    pub snapshot_path: Option<String>,
    pub clip_path: Option<String>,
    /// P5: 导出到证据包的时间, ISO 8601
    pub exported_at: Option<String>,
    /// P5: 导出后该事件子文件夹在目标目录中的绝对路径
    pub export_path: Option<String>,
}

impl ParkingEvent {
    pub fn new_id() -> String {
        Uuid::new_v4().to_string()
    }
}
