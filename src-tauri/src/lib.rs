// 路况记录助手 - Rust 后端入口
//
// 当前阶段: P0 (工程脚手架)
// 已挂载模块:
//   - ai::model_path  模型路径解析
//   - ai::vehicle     YOLOv8 推理
//   - commands::system    系统/模型状态检查
//   - commands::detection P0 demo 推理命令

pub mod ai;
pub mod commands;
pub mod models;

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

    // 注意: ort 用 load-dynamic 模式时会读 ORT_DYLIB_PATH
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
        ])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}
