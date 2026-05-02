// P2 批处理流水线命令

use std::path::PathBuf;

use serde::Serialize;
use tauri::AppHandle;

use crate::models::job::VideoJob;
use crate::pipeline::parallel;

#[derive(Debug, Serialize)]
pub struct StartBatchDto {
    pub batch_id: String,
    pub job_count: usize,
}

#[tauri::command]
pub async fn start_batch_pipeline(
    paths: Vec<String>,
    app: AppHandle,
) -> Result<StartBatchDto, String> {
    let path_bufs: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    parallel::start_batch(app, path_bufs)
        .await
        .map(|o| StartBatchDto {
            batch_id: o.batch_id,
            job_count: o.job_count,
        })
        .map_err(|e| format!("启动批处理失败: {e:#}"))
}

#[tauri::command]
pub async fn resume_pending_jobs(app: AppHandle) -> Result<StartBatchDto, String> {
    parallel::resume_pending(app)
        .await
        .map(|o| StartBatchDto {
            batch_id: o.batch_id,
            job_count: o.job_count,
        })
        .map_err(|e| format!("续跑失败: {e:#}"))
}

#[tauri::command]
pub async fn list_jobs() -> Result<Vec<VideoJob>, String> {
    tokio::task::spawn_blocking(|| -> anyhow::Result<Vec<VideoJob>> {
        let lock = crate::db::conn()?;
        let conn = lock.lock().map_err(|e| anyhow::anyhow!("DB mutex 中毒: {e}"))?;
        crate::db::jobs::list_all(&conn)
    })
    .await
    .map_err(|e| format!("查询任务 panic: {e}"))?
    .map_err(|e| format!("查询任务失败: {e:#}"))
}

#[tauri::command]
pub async fn list_pending_jobs() -> Result<Vec<VideoJob>, String> {
    tokio::task::spawn_blocking(|| -> anyhow::Result<Vec<VideoJob>> {
        let lock = crate::db::conn()?;
        let conn = lock.lock().map_err(|e| anyhow::anyhow!("DB mutex 中毒: {e}"))?;
        crate::db::jobs::list_pending(&conn)
    })
    .await
    .map_err(|e| format!("查询任务 panic: {e}"))?
    .map_err(|e| format!("查询任务失败: {e:#}"))
}
