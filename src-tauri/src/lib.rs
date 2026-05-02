// 路况记录助手 - Rust 后端入口
//
// 当前阶段: P1 (单视频识别 pipeline)
// 已挂载模块:
//   - ai::vehicle / ai::plate / ai::model_path
//   - video::metadata / video::extract
//   - pipeline::orchestrator / pipeline::aggregate
//   - db (SQLite 持久化, P6 升级 SQLCipher)
//   - commands::{system, detection, video}

pub mod ai;
pub mod commands;
pub mod db;
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
            commands::detection::detect_demo,
            commands::video::read_video_metadata,
            commands::video::process_video,
            commands::video::list_events,
            commands::video::detect_plate_demo,
        ])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}
