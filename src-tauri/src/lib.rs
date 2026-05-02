// 路况记录助手 - Rust 后端入口
//
// 当前阶段: P2 (批量并发流水线)
// 已挂载模块:
//   - ai::vehicle / ai::plate / ai::model_path
//   - video::metadata / video::extract
//   - pipeline::orchestrator (P1 单视频) / pipeline::parallel (P2 批处理)
//   - db (events + video_jobs)
//   - commands::{system, detection, video, pipeline}

pub mod ai;
pub mod commands;
pub mod db;
pub mod evidence;
pub mod models;
pub mod pipeline;
pub mod video;

use tracing_subscriber::EnvFilter;

fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("info,illegal_parking_reporter_lib=debug,ort=info")
    });
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .try_init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "路况记录助手启动");

    if let Err(e) = video::init() {
        tracing::error!(error = %e, "ffmpeg 初始化失败, 视频功能将不可用");
    }
    if let Err(e) = db::init() {
        tracing::error!(error = %e, "数据库初始化失败, 持久化功能将不可用");
    } else {
        // 启动恢复: 把残留的 running 任务重置为 pending, 等待用户在 UI 点续跑
        if let Ok(lock) = db::conn() {
            if let Ok(conn) = lock.lock() {
                let _ = db::jobs::reset_running_to_pending(&conn);
            }
        }
    }

    if std::env::var("ORT_DYLIB_PATH").is_err() {
        tracing::warn!(
            "ORT_DYLIB_PATH 未设置, ort 加载会失败. \
             请在 ~/.zshrc 加: \
             export ORT_DYLIB_PATH=/opt/homebrew/lib/libonnxruntime.dylib"
        );
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            commands::system::check_system_status,
            commands::system::open_in_file_manager,
            commands::detection::detect_demo,
            commands::video::read_video_metadata,
            commands::video::process_video,
            commands::video::list_events,
            commands::video::detect_plate_demo,
            commands::pipeline::start_batch_pipeline,
            commands::pipeline::resume_pending_jobs,
            commands::pipeline::list_jobs,
            commands::pipeline::list_pending_jobs,
        ])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}
