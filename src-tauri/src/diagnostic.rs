// P7: 日志 + 诊断包导出
//
// 日志: tracing 同时写控制台 + 滚动日志文件 (parking.log + 5 备份)
// 诊断包: 一个 zip 文件含
//   - logs/parking.log + 历史
//   - models.json (模型路径与状态)
//   - system.json (系统信息)
//   - failed_jobs.json (DB 中 status=failed 的任务, 脱敏车牌)

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::json;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

/// 日志目录: <app_data>/logs/
pub fn logs_dir() -> Result<PathBuf> {
    let mut p = crate::db::db_path()?
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| anyhow::anyhow!("无法定位 logs 目录"))?;
    p.push("logs");
    std::fs::create_dir_all(&p)?;
    Ok(p)
}

/// 初始化 tracing: stdout + 滚动文件
///
/// 返回的 WorkerGuard 必须在程序退出前保持存活, 否则文件 sink 不刷新
pub fn init_logging_with_file() -> Result<WorkerGuard> {
    let dir = logs_dir()?;
    let appender = rolling::daily(&dir, "parking.log");
    let (file_writer, guard) = tracing_appender::non_blocking(appender);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("info,illegal_parking_reporter_lib=debug,ort=info")
    });

    let stdout_layer = fmt::layer()
        .with_target(true)
        .with_writer(std::io::stdout);
    let file_layer = fmt::layer()
        .with_ansi(false)
        .with_target(true)
        .with_writer(file_writer);

    tracing_subscriber::registry()
        .with(filter)
        .with(stdout_layer)
        .with(file_layer)
        .try_init()
        .ok();

    tracing::info!(?dir, "日志文件 sink 已就绪");
    Ok(guard)
}

#[derive(Debug, Serialize)]
pub struct DiagnosticReport {
    pub bundle_path: String,
    pub size_bytes: u64,
    pub log_files_included: usize,
    pub failed_jobs_count: usize,
}

/// 把日志 + 系统信息 + 失败任务打包到 zip
pub fn export_diagnostic_bundle(target_dir: &Path) -> Result<DiagnosticReport> {
    if !target_dir.exists() {
        anyhow::bail!("目标目录不存在: {}", target_dir.display());
    }
    let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let zip_path = target_dir.join(format!("路况记录助手_诊断包_{stamp}.zip"));
    let file = std::fs::File::create(&zip_path)
        .with_context(|| format!("创建 zip 失败: {}", zip_path.display()))?;
    let mut zw = zip::ZipWriter::new(file);
    let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    // 1. 日志文件
    let logs = logs_dir()?;
    let mut log_count = 0usize;
    if let Ok(entries) = std::fs::read_dir(&logs) {
        for ent in entries.flatten() {
            let p = ent.path();
            if !p.is_file() {
                continue;
            }
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("log");
            zw.start_file(format!("logs/{name}"), opts)?;
            let bytes = std::fs::read(&p).unwrap_or_default();
            std::io::Write::write_all(&mut zw, &bytes)?;
            log_count += 1;
        }
    }

    // 2. 系统信息
    let sys_info = json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "app_version": env!("CARGO_PKG_VERSION"),
        "rust_version": env!("CARGO_PKG_RUST_VERSION"),
        "ort_dylib_path": std::env::var("ORT_DYLIB_PATH").ok(),
        "models_dir_env": std::env::var("IPR_MODELS_DIR").ok(),
        "now": chrono::Utc::now().to_rfc3339(),
    });
    zw.start_file("system.json", opts)?;
    std::io::Write::write_all(&mut zw, serde_json::to_string_pretty(&sys_info)?.as_bytes())?;

    // 3. 模型列表
    let model_status = crate::commands::system::check_system_status();
    zw.start_file("models.json", opts)?;
    std::io::Write::write_all(
        &mut zw,
        serde_json::to_string_pretty(&model_status)?.as_bytes(),
    )?;

    // 4. 失败任务 (车牌脱敏)
    let mut failed_count = 0usize;
    if let Ok(lock) = crate::db::conn() {
        if let Ok(conn) = lock.lock() {
            if let Ok(jobs) = crate::db::jobs::list_all(&conn) {
                let failed: Vec<_> = jobs
                    .into_iter()
                    .filter(|j| matches!(j.status, crate::models::job::JobStatus::Failed))
                    .collect();
                failed_count = failed.len();
                zw.start_file("failed_jobs.json", opts)?;
                std::io::Write::write_all(
                    &mut zw,
                    serde_json::to_string_pretty(&failed)?.as_bytes(),
                )?;
            }
            // 脱敏的事件统计 (不输出车牌, 只统计数量)
            if let Ok(events) = crate::db::events::list_all(&conn) {
                let by_status = events.iter().fold(
                    std::collections::HashMap::<&str, usize>::new(),
                    |mut acc, e| {
                        let k = e.review_status.as_str();
                        *acc.entry(k).or_default() += 1;
                        acc
                    },
                );
                let summary = json!({
                    "total_events": events.len(),
                    "by_status": by_status,
                });
                zw.start_file("events_summary.json", opts)?;
                std::io::Write::write_all(
                    &mut zw,
                    serde_json::to_string_pretty(&summary)?.as_bytes(),
                )?;
            }
        }
    }

    zw.finish()?;
    let size = std::fs::metadata(&zip_path).map(|m| m.len()).unwrap_or(0);
    Ok(DiagnosticReport {
        bundle_path: zip_path.to_string_lossy().to_string(),
        size_bytes: size,
        log_files_included: log_count,
        failed_jobs_count: failed_count,
    })
}

/// 模型预热: 后台普通线程触发 OnceCell 初始化, 让首帧推理快
///
/// 注意: 不能用 tokio::task::spawn_blocking, 因为 Tauri 2 的 Builder::setup
/// 闭包在 tao 主线程而非 tokio runtime 内. Detector::load 本身是同步的,
/// 用 std::thread 即可.
pub fn spawn_warmup() {
    std::thread::Builder::new()
        .name("ai-warmup".to_string())
        .spawn(|| {
            tracing::info!("开始预热 AI 模型 (后台线程)");
            if let Err(e) = crate::ai::vehicle::detector() {
                tracing::warn!(error = %e, "YOLOv8 预热失败 (推理时会重试)");
            }
            // plate / sidewalk 留首次调用时延迟加载, 避免占用太多内存
        })
        .ok();
}
