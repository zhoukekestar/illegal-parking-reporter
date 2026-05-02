// 路况记录助手 - Rust 后端入口
//
// 当前阶段: P6 (本地登录 + 设置 + SQLCipher 加密)

pub mod ai;
pub mod auth;
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

    // 初始化 ffmpeg
    if let Err(e) = video::init() {
        tracing::error!(error = %e, "ffmpeg 初始化失败, 视频功能将不可用");
    }

    // 加载/初始化 auth.json + 派生 SQLCipher key (无密码时用 secret 直接做 key)
    let cipher_key_hex = match auth::load_or_init() {
        Ok(a) => match auth::derive_sqlcipher_key(&a, None) {
            Ok(k) => Some(k),
            Err(e) => {
                tracing::error!(error = %e, "派生 SQLCipher key 失败, 退化为 plain DB");
                None
            }
        },
        Err(e) => {
            tracing::error!(error = %e, "加载 auth.json 失败");
            None
        }
    };

    if let Err(e) = db::init(cipher_key_hex.as_deref()) {
        tracing::error!(error = %e, "数据库初始化失败");
    } else {
        if let Ok(lock) = db::conn() {
            if let Ok(conn) = lock.lock() {
                let _ = db::jobs::reset_running_to_pending(&conn);
            }
        }
    }

    if std::env::var("ORT_DYLIB_PATH").is_err() {
        tracing::warn!(
            "ORT_DYLIB_PATH 未设置, ort 加载会失败. \
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
            commands::video::update_event_status,
            commands::video::update_event_plate,
            commands::pipeline::start_batch_pipeline,
            commands::pipeline::resume_pending_jobs,
            commands::pipeline::list_jobs,
            commands::pipeline::list_pending_jobs,
            commands::export::export_accepted_events,
            commands::auth::auth_state,
            commands::auth::set_password,
            commands::auth::unlock,
            commands::auth::lock,
            commands::auth::get_settings,
            commands::auth::save_settings,
            commands::auth::purge_data,
        ])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}
