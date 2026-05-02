// 本地数据库 (P1: 明文 SQLite, P6: SQLCipher 加密)
//
// 单进程访问, 用 Mutex<Connection> 包装即可 (rusqlite 单连接非 Sync)
// 高并发场景将来切到 r2d2/deadpool 连接池

pub mod events;
pub mod schema;

use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{Context, Result};
use once_cell::sync::OnceCell;
use rusqlite::Connection;

static DB: OnceCell<Mutex<Connection>> = OnceCell::new();

/// 数据库文件路径
///
/// 优先级:
///   1. env IPR_DB_PATH (开发期手动指定)
///   2. dev: <crate>/.local/parking.sqlite
///   3. release: $HOME/Library/Application Support/路况记录助手/parking.sqlite (macOS)
fn db_path() -> Result<PathBuf> {
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

/// 初始化数据库, 创建表 (幂等)
pub fn init() -> Result<()> {
    if DB.get().is_some() {
        return Ok(());
    }
    let path = db_path()?;
    tracing::info!(?path, "打开本地 SQLite 数据库");
    let conn = Connection::open(&path).with_context(|| format!("打开 SQLite 失败: {}", path.display()))?;

    // PRAGMA: 开发期友好的设置
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    schema::run_migrations(&conn).context("运行 schema 迁移失败")?;
    let _ = DB.set(Mutex::new(conn));
    Ok(())
}

/// 取全局连接 (其他模块通过此入口)
pub fn conn() -> Result<&'static Mutex<Connection>> {
    DB.get().context("数据库未初始化, 请先调用 db::init()")
}
