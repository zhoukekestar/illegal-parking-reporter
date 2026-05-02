// 路况记录助手 - Rust 后端入口
//
// 当前阶段: P7 (打磨 / 性能 / 诊断)

pub mod ai;
pub mod auth;
pub mod commands;
pub mod db;
pub mod diagnostic;
pub mod evidence;
pub mod models;
pub mod pipeline;
pub mod video;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // P7: 日志同时写文件 (guard 必须保持存活直到进程退出)
    let _log_guard = match diagnostic::init_logging_with_file() {
        Ok(g) => Some(g),
        Err(e) => {
            eprintln!("初始化日志文件 sink 失败: {e:#}, 退化到 stderr");
            None
        }
    };
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "路况记录助手启动");

    if let Err(e) = video::init() {
        tracing::error!(error = %e, "ffmpeg 初始化失败");
    }

    // 加载/初始化 auth.json + 派生 SQLCipher key
    let cipher_key_hex = match auth::load_or_init() {
        Ok(a) => match auth::derive_sqlcipher_key(&a, None) {
            Ok(k) => Some(k),
            Err(e) => {
                tracing::error!(error = %e, "派生 SQLCipher key 失败");
                None
            }
        },
        Err(e) => {
            tracing::error!(error = %e, "加载 auth.json 失败");
            None
        }
    };

    // db init: 失败时检测是否为旧 plain DB, 自动备份重建
    if let Err(e) = db::init(cipher_key_hex.as_deref()) {
        let msg = format!("{e:#}");
        let looks_like_plain = msg.contains("not a database")
            || msg.contains("file is encrypted")
            || msg.contains("not encrypted");
        if looks_like_plain {
            if let Ok(p) = db::db_path() {
                let backup = p.with_extension("plain.bak");
                tracing::warn!(
                    path = ?p,
                    backup = ?backup,
                    "检测到旧 plain SQLite (P0-P5 遗留), 备份并重建为 SQLCipher 加密 DB"
                );
                let _ = std::fs::rename(&p, &backup);
                // 顺便清掉同目录可能残留的 -wal / -shm
                let _ = std::fs::remove_file(p.with_extension("sqlite-wal"));
                let _ = std::fs::remove_file(p.with_extension("sqlite-shm"));
                if let Err(e2) = db::init(cipher_key_hex.as_deref()) {
                    tracing::error!(error = %e2, "重建数据库失败");
                }
            }
        } else {
            tracing::error!(error = %e, "数据库初始化失败");
        }
    }
    if let Ok(lock) = db::conn() {
        if let Ok(conn) = lock.lock() {
            let _ = db::jobs::reset_running_to_pending(&conn);
        }
    }

    if std::env::var("ORT_DYLIB_PATH").is_err() {
        tracing::warn!(
            "ORT_DYLIB_PATH 未设置. \
             export ORT_DYLIB_PATH=/opt/homebrew/lib/libonnxruntime.dylib"
        );
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_clipboard_manager::init())
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
            commands::video::mark_event_uploaded,
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
            commands::diag::export_diagnostic,
        ])
        .setup(|app| {
            // P7: 后台预热 YOLOv8, 让首次推理更快
            // 必须放在 tokio runtime 内, Tauri 的 setup 已是 runtime 内
            let _app = app;
            diagnostic::spawn_warmup();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}
