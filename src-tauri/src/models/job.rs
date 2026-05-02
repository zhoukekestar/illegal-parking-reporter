// 视频处理任务 (P2)
//
// 一个 batch 包含多个 video_jobs, 每个 job 跟踪单视频在 pipeline 中的状态。
// kill -9 重启后, 处于 Pending/Running 的 job 会被恢复入口检测到 (并重置为 Pending)

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    /// 已入库, 等待执行
    Pending,
    /// 正在执行 (任意 stage)
    Running,
    /// 全部 stage 成功完成
    Success,
    /// 失败 (错误信息存 last_error)
    Failed,
}

impl JobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            JobStatus::Pending => "pending",
            JobStatus::Running => "running",
            JobStatus::Success => "success",
            JobStatus::Failed => "failed",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "running" => Some(Self::Running),
            "success" => Some(Self::Success),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoJob {
    pub id: String,
    pub batch_id: String,
    /// 源视频绝对路径
    pub source_video: String,
    pub status: JobStatus,
    /// 已处理帧数 (Stage 1+2 累计, Stage 3 = 完成)
    pub processed_frames: u32,
    /// 估算总帧数 (基于 duration × target_fps)
    pub estimated_frames: u32,
    /// 最近一次错误 (Failed 时填)
    pub last_error: Option<String>,
    /// 入队时间, ISO 8601
    pub created_at: String,
    /// 完成时间 (Success/Failed)
    pub finished_at: Option<String>,
    /// 该 job 产出的事件数 (Success 时填)
    pub events_count: u32,
}
