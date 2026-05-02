// 表结构 (P1 简化版)
//
// 设计原则:
//   - bbox / event_time 用 TEXT 存 JSON 字符串, 简化 schema 演进
//   - plate_number 允许 NULL (车牌识别失败 = "<待确认>" 也按 TEXT 存)
//   - review_status 用 TEXT 枚举: pending/accepted/rejected/deferred
//   - 预留 user_id 列(P6 多用户准备); P1 全部填 "default"

use anyhow::Result;
use rusqlite::Connection;

const SCHEMA_VERSION: i32 = 1;

pub fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
        [],
    )?;

    let current: i32 = conn
        .query_row(
            "SELECT value FROM schema_meta WHERE key = 'version'",
            [],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    if current < 1 {
        conn.execute_batch(SCHEMA_V1)?;
        conn.execute(
            "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('version', ?1)",
            rusqlite::params![SCHEMA_VERSION.to_string()],
        )?;
        tracing::info!(version = SCHEMA_VERSION, "schema 迁移完成");
    }

    Ok(())
}

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS events (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL DEFAULT 'default',
    source_video TEXT NOT NULL,
    representative_frame_index INTEGER NOT NULL,
    timestamp_ms INTEGER NOT NULL,
    event_time TEXT,                          -- ISO 8601, 可空
    plate_number TEXT NOT NULL,
    plate_confidence REAL NOT NULL,
    plate_manual_corrected TEXT,
    vehicle_class TEXT NOT NULL,
    vehicle_bbox_json TEXT NOT NULL,          -- "[x1,y1,x2,y2]"
    first_seen_ms INTEGER NOT NULL,
    last_seen_ms INTEGER NOT NULL,
    frame_hits INTEGER NOT NULL,
    review_status TEXT NOT NULL DEFAULT 'pending',
    iou_score REAL,
    snapshot_path TEXT,
    clip_path TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE INDEX IF NOT EXISTS idx_events_source_video ON events(source_video);
CREATE INDEX IF NOT EXISTS idx_events_review_status ON events(review_status);
CREATE INDEX IF NOT EXISTS idx_events_plate ON events(plate_number);
"#;
