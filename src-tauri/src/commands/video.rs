// 视频相关 Tauri 命令
// MVU 2: read_video_metadata
// MVU 5: detect_plate_demo (单图调试用)
// MVU 8: process_video / list_events

use std::path::PathBuf;

use serde::Serialize;

use crate::models::event::ParkingEvent;
use crate::models::observation::FrameObservation;
use crate::pipeline::orchestrator;
use crate::video::metadata::{read_metadata, VideoMetadata};

#[tauri::command]
pub async fn read_video_metadata(path: String) -> Result<VideoMetadata, String> {
    tokio::task::spawn_blocking(move || -> anyhow::Result<VideoMetadata> {
        let p = PathBuf::from(&path);
        if !p.exists() {
            anyhow::bail!("视频不存在: {}", p.display());
        }
        read_metadata(&p)
    })
    .await
    .map_err(|e| format!("blocking 任务 panic: {e}"))?
    .map_err(|e| format!("读取元数据失败: {e:#}"))
}

#[derive(Debug, Serialize)]
pub struct ProcessOutcomeDto {
    pub metadata: VideoMetadata,
    pub observations: Vec<FrameObservation>,
    pub events: Vec<ParkingEvent>,
}

/// 端到端处理一个视频, 把事件写入 DB, 返回完整结果
#[tauri::command]
pub async fn process_video(path: String) -> Result<ProcessOutcomeDto, String> {
    tokio::task::spawn_blocking(move || -> anyhow::Result<ProcessOutcomeDto> {
        let p = PathBuf::from(&path);
        if !p.exists() {
            anyhow::bail!("视频不存在: {}", p.display());
        }
        let outcome = orchestrator::process_video(&p, true)?;

        // 持久化
        if !outcome.events.is_empty() {
            let lock = crate::db::conn()?;
            let mut conn = lock.lock().map_err(|e| anyhow::anyhow!("DB mutex 中毒: {e}"))?;
            crate::db::events::save_events(&mut conn, &outcome.events)?;
        }
        Ok(ProcessOutcomeDto {
            metadata: outcome.metadata,
            observations: outcome.observations,
            events: outcome.events,
        })
    })
    .await
    .map_err(|e| format!("blocking 任务 panic: {e}"))?
    .map_err(|e| format!("处理视频失败: {e:#}"))
}

#[tauri::command]
pub async fn list_events() -> Result<Vec<ParkingEvent>, String> {
    tokio::task::spawn_blocking(|| -> anyhow::Result<Vec<ParkingEvent>> {
        let lock = crate::db::conn()?;
        let conn = lock.lock().map_err(|e| anyhow::anyhow!("DB mutex 中毒: {e}"))?;
        crate::db::events::list_all(&conn)
    })
    .await
    .map_err(|e| format!("blocking 任务 panic: {e}"))?
    .map_err(|e| format!("查询事件失败: {e:#}"))
}

/// 修改事件审核状态 (P4)
#[tauri::command]
pub async fn update_event_status(
    event_id: String,
    status: String,
) -> Result<(), String> {
    let parsed = crate::models::event::ReviewStatus::parse(&status)
        .ok_or_else(|| format!("非法 status: {status}"))?;
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let lock = crate::db::conn()?;
        let conn = lock.lock().map_err(|e| anyhow::anyhow!("DB mutex 中毒: {e}"))?;
        crate::db::events::update_review_status(&conn, &event_id, parsed)
    })
    .await
    .map_err(|e| format!("blocking task panic: {e}"))?
    .map_err(|e| format!("更新状态失败: {e:#}"))
}

/// 修改事件人工车牌 (P4)
#[tauri::command]
pub async fn update_event_plate(
    event_id: String,
    corrected: Option<String>,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let lock = crate::db::conn()?;
        let conn = lock.lock().map_err(|e| anyhow::anyhow!("DB mutex 中毒: {e}"))?;
        crate::db::events::update_plate_correction(&conn, &event_id, corrected.as_deref())
    })
    .await
    .map_err(|e| format!("blocking task panic: {e}"))?
    .map_err(|e| format!("更新车牌失败: {e:#}"))
}

#[derive(Debug, serde::Serialize)]
pub struct CleanupSummary {
    pub deleted_count: usize,
    pub deleted_evidence_dirs: usize,
}

/// 删除不符合中国车牌格式的历史事件 (用户反馈: <待确认> 与 OCR 乱码事件淹没列表)
///
/// 同时尝试删除每个事件对应的 evidence 子目录 (失败仅记录日志, 不影响 DB 删除)
#[tauri::command]
pub async fn cleanup_invalid_events() -> Result<CleanupSummary, String> {
    tokio::task::spawn_blocking(|| -> anyhow::Result<CleanupSummary> {
        let lock = crate::db::conn()?;
        let conn = lock.lock().map_err(|e| anyhow::anyhow!("DB mutex 中毒: {e}"))?;
        let (ids, dirs) = crate::db::events::delete_events_with_invalid_plates(&conn)?;
        let deleted_count = ids.len();
        let mut dir_removed = 0usize;
        for d in &dirs {
            match std::fs::remove_dir_all(d) {
                Ok(_) => dir_removed += 1,
                Err(e) => tracing::warn!(error = %e, path = %d, "删除 evidence 子目录失败"),
            }
        }
        tracing::info!(
            deleted_count,
            evidence_dirs_removed = dir_removed,
            "清理无效事件完成"
        );
        Ok(CleanupSummary {
            deleted_count,
            deleted_evidence_dirs: dir_removed,
        })
    })
    .await
    .map_err(|e| format!("blocking task panic: {e}"))?
    .map_err(|e| format!("清理失败: {e:#}"))
}

/// P8.1: 标记事件为"已上传"
#[tauri::command]
pub async fn mark_event_uploaded(event_id: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let lock = crate::db::conn()?;
        let conn = lock.lock().map_err(|e| anyhow::anyhow!("DB mutex 中毒: {e}"))?;
        crate::db::events::mark_uploaded(&conn, &event_id)
    })
    .await
    .map_err(|e| format!("blocking task panic: {e}"))?
    .map_err(|e| format!("标记上传失败: {e:#}"))
}

/// 单图车牌检测 + 识别 (MVU 5 demo)
#[tauri::command]
pub async fn detect_plate_demo(image_path: String) -> Result<Vec<crate::models::observation::PlateReading>, String> {
    tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let p = PathBuf::from(&image_path);
        if !p.exists() {
            anyhow::bail!("图片不存在: {}", p.display());
        }
        let img = image::open(&p)?.to_rgb8();
        crate::ai::plate::detect_and_recognize(&img)
    })
    .await
    .map_err(|e| format!("blocking 任务 panic: {e}"))?
    .map_err(|e| format!("车牌识别失败: {e:#}"))
}
