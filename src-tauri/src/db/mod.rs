// 本地数据库 (P1 plain SQLite -> P6 SQLCipher 加密)

pub mod events;
pub mod jobs;
pub mod schema;
pub mod settings;

use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{Context, Result};
use once_cell::sync::OnceCell;
use rusqlite::Connection;

static DB: OnceCell<Mutex<Connection>> = OnceCell::new();

/// 数据库文件路径 (公开给 auth 模块)
pub fn db_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("IPR_DB_PATH") {
        return Ok(PathBuf::from(p));
    }
    #[cfg(debug_assertions)]
    {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push(".local");
        std::fs::create_dir_all(&p).context("创建 .local 目录失败")?;
        p.push("parking.sqlite");
        return Ok(p);
    }
    #[cfg(not(debug_assertions))]
    {
        let home = std::env::var("HOME").context("HOME 环境变量未设置")?;
        let mut p = PathBuf::from(home);
        #[cfg(target_os = "macos")]
        {
            p.push("Library");
            p.push("Application Support");
            p.push("路况记录助手");
        }
        #[cfg(not(target_os = "macos"))]
        {
            p.push(".local");
            p.push("share");
            p.push("illegal-parking-reporter");
        }
        std::fs::create_dir_all(&p).context("创建用户数据目录失败")?;
        p.push("parking.sqlite");
        Ok(p)
    }
}

/// 初始化数据库
///
/// `cipher_key_hex`: SQLCipher 用的十六进制密钥 (64 char = 32 字节). None 表示不加密 (仅供测试)
pub fn init(cipher_key_hex: Option<&str>) -> Result<()> {
    if DB.get().is_some() {
        return Ok(());
    }
    let path = db_path()?;
    tracing::info!(?path, encrypted = cipher_key_hex.is_some(), "打开 SQLite 数据库");
    let conn = Connection::open(&path).with_context(|| format!("打开 SQLite 失败: {}", path.display()))?;

    if let Some(key_hex) = cipher_key_hex {
        // SQLCipher: PRAGMA key 必须在第一次访问 db 之前执行
        // 用 x'...' 形式直接传 raw key (避免 PBKDF2 二次派生)
        conn.execute_batch(&format!("PRAGMA key = \"x'{key_hex}'\";"))
            .context("PRAGMA key 设置失败 (检查 SQLCipher 是否启用)")?;
    }

    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    schema::run_migrations(&conn).context("运行 schema 迁移失败")?;
    let _ = DB.set(Mutex::new(conn));
    Ok(())
}

pub fn conn() -> Result<&'static Mutex<Connection>> {
    DB.get().context("数据库未初始化, 请先调用 db::init()")
}

/// 清空所有业务数据 (P6: 清空数据按钮)
pub fn purge_all() -> Result<()> {
    let lock = conn()?;
    let conn = lock.lock().map_err(|e| anyhow::anyhow!("DB mutex 中毒: {e}"))?;
    conn.execute_batch(
        r#"DELETE FROM events;
           DELETE FROM video_jobs;
           DELETE FROM settings;"#,
    )?;
    Ok(())
}
