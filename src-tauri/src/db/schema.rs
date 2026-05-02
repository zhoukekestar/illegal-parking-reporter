// 表结构 (递增式迁移)
//
// V1 (P1): events 表
// V2 (P2): video_jobs 表 (批处理状态跟踪 + 断点续传)

use anyhow::Result;
use rusqlite::Connection;

const SCHEMA_VERSION: i32 = 4;

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
        tracing::info!("schema 升级到 v1");
    }
    if current < 2 {
        conn.execute_batch(SCHEMA_V2)?;
        tracing::info!("schema 升级到 v2 (video_jobs)");
    }
    if current < 3 {
        conn.execute_batch(SCHEMA_V3)?;
        tracing::info!("schema 升级到 v3 (events.exported_*)");
    }
    if current < 4 {
        crate::db::settings::ensure_schema(conn)?;
        tracing::info!("schema 升级到 v4 (settings)");
    }

    if current < SCHEMA_VERSION {
        conn.execute(
            "INSERT OR REPLACE INTO schema_meta (key, value) VALUES ('version', ?1)",
            rusqlite::params![SCHEMA_VERSION.to_string()],
        )?;
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
    event_time TEXT,
    plate_number TEXT NOT NULL,
    plate_confidence REAL NOT NULL,
    plate_manual_corrected TEXT,
    vehicle_class TEXT NOT NULL,
    vehicle_bbox_json TEXT NOT NULL,
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

const SCHEMA_V2: &str = r#"
CREATE TABLE IF NOT EXISTS video_jobs (
    id TEXT PRIMARY KEY,
    batch_id TEXT NOT NULL,
    source_video TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    processed_frames INTEGER NOT NULL DEFAULT 0,
    estimated_frames INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    finished_at TEXT,
    events_count INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_jobs_batch ON video_jobs(batch_id);
CREATE INDEX IF NOT EXISTS idx_jobs_status ON video_jobs(status);
CREATE INDEX IF NOT EXISTS idx_jobs_video ON video_jobs(source_video);
"#;

const SCHEMA_V3: &str = r#"
ALTER TABLE events ADD COLUMN exported_at TEXT;
ALTER TABLE events ADD COLUMN export_path TEXT;
CREATE INDEX IF NOT EXISTS idx_events_exported_at ON events(exported_at);
"#;
