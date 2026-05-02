// settings 表: kv 形式存用户设置 (P6)
//
// 默认值参见 DEVELOPMENT_PLAN.md §6
//   - iou_threshold: 0.3
//   - clip_pre_secs / clip_post_secs: 3 / 3 (合 6s)
//   - sample_fps: 1.0
//   - plate_conf_threshold: 0.6
//   - aggregate_window_secs: 60
//   - first_run_done: false (首启引导用)

use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub iou_threshold: f32,
    pub clip_pre_secs: f32,
    pub clip_post_secs: f32,
    pub sample_fps: f32,
    pub plate_conf_threshold: f32,
    pub aggregate_window_secs: u32,
    pub first_run_done: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            iou_threshold: 0.3,
            clip_pre_secs: 3.0,
            clip_post_secs: 3.0,
            sample_fps: 1.0,
            plate_conf_threshold: 0.6,
            aggregate_window_secs: 60,
            first_run_done: false,
        }
    }
}

pub fn ensure_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )"#,
    )?;
    Ok(())
}

pub fn load(conn: &Connection) -> Result<AppSettings> {
    let mut s = AppSettings::default();
    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (k, v) = row?;
        match k.as_str() {
            "iou_threshold" => s.iou_threshold = v.parse().unwrap_or(s.iou_threshold),
            "clip_pre_secs" => s.clip_pre_secs = v.parse().unwrap_or(s.clip_pre_secs),
            "clip_post_secs" => s.clip_post_secs = v.parse().unwrap_or(s.clip_post_secs),
            "sample_fps" => s.sample_fps = v.parse().unwrap_or(s.sample_fps),
            "plate_conf_threshold" => {
                s.plate_conf_threshold = v.parse().unwrap_or(s.plate_conf_threshold)
            }
            "aggregate_window_secs" => {
                s.aggregate_window_secs = v.parse().unwrap_or(s.aggregate_window_secs)
            }
            "first_run_done" => s.first_run_done = v == "true",
            _ => {}
        }
    }
    Ok(s)
}

pub fn save(conn: &Connection, s: &AppSettings) -> Result<()> {
    let pairs: [(&str, String); 7] = [
        ("iou_threshold", s.iou_threshold.to_string()),
        ("clip_pre_secs", s.clip_pre_secs.to_string()),
        ("clip_post_secs", s.clip_post_secs.to_string()),
        ("sample_fps", s.sample_fps.to_string()),
        ("plate_conf_threshold", s.plate_conf_threshold.to_string()),
        ("aggregate_window_secs", s.aggregate_window_secs.to_string()),
        ("first_run_done", s.first_run_done.to_string()),
    ];
    for (k, v) in pairs.iter() {
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![k, v],
        )?;
    }
    Ok(())
}
