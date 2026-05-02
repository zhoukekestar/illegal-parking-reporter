// video_jobs 表 CRUD (P2)

use anyhow::{Context, Result};
use rusqlite::{params, Connection, Row};

use crate::models::job::{JobStatus, VideoJob};

pub fn insert_job(conn: &Connection, job: &VideoJob) -> Result<()> {
    conn.execute(
        r#"INSERT INTO video_jobs (
            id, batch_id, source_video, status, processed_frames, estimated_frames,
            last_error, created_at, finished_at, events_count
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"#,
        params![
            job.id,
            job.batch_id,
            job.source_video,
            job.status.as_str(),
            job.processed_frames as i64,
            job.estimated_frames as i64,
            job.last_error,
            job.created_at,
            job.finished_at,
            job.events_count as i64,
        ],
    )?;
    Ok(())
}

pub fn update_status(
    conn: &Connection,
    job_id: &str,
    status: JobStatus,
    last_error: Option<&str>,
    finished: bool,
) -> Result<()> {
    let finished_at = if finished {
        Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
    } else {
        None
    };
    conn.execute(
        r#"UPDATE video_jobs
              SET status = ?1,
                  last_error = COALESCE(?2, last_error),
                  finished_at = COALESCE(?3, finished_at)
            WHERE id = ?4"#,
        params![status.as_str(), last_error, finished_at, job_id],
    )?;
    Ok(())
}

pub fn update_progress(
    conn: &Connection,
    job_id: &str,
    processed_frames: u32,
) -> Result<()> {
    conn.execute(
        "UPDATE video_jobs SET processed_frames = ?1 WHERE id = ?2",
        params![processed_frames as i64, job_id],
    )?;
    Ok(())
}

pub fn finalize_success(
    conn: &Connection,
    job_id: &str,
    events_count: u32,
) -> Result<()> {
    let finished_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    conn.execute(
        r#"UPDATE video_jobs
              SET status = 'success',
                  events_count = ?1,
                  finished_at = ?2,
                  last_error = NULL
            WHERE id = ?3"#,
        params![events_count as i64, finished_at, job_id],
    )?;
    Ok(())
}

/// 启动时把残留的 Running 重置为 Pending (kill 进程后这些 job 实际未完成)
pub fn reset_running_to_pending(conn: &Connection) -> Result<usize> {
    let n = conn.execute(
        "UPDATE video_jobs SET status = 'pending', last_error = '上次会话被终止' WHERE status = 'running'",
        [],
    )?;
    if n > 0 {
        tracing::warn!(count = n, "把残留的 running 任务重置为 pending");
    }
    Ok(n)
}

pub fn list_all(conn: &Connection) -> Result<Vec<VideoJob>> {
    let mut stmt = conn.prepare(
        r#"SELECT id, batch_id, source_video, status, processed_frames, estimated_frames,
                  last_error, created_at, finished_at, events_count
           FROM video_jobs
           ORDER BY created_at DESC"#,
    )?;
    let rows = stmt.query_map([], row_to_job)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn list_pending(conn: &Connection) -> Result<Vec<VideoJob>> {
    let mut stmt = conn.prepare(
        r#"SELECT id, batch_id, source_video, status, processed_frames, estimated_frames,
                  last_error, created_at, finished_at, events_count
           FROM video_jobs
           WHERE status = 'pending'
           ORDER BY created_at"#,
    )?;
    let rows = stmt.query_map([], row_to_job)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// 查找指定视频的最近一次任务 (避免重复入队同一视频)
pub fn find_latest_by_video(conn: &Connection, source_video: &str) -> Result<Option<VideoJob>> {
    let mut stmt = conn.prepare(
        r#"SELECT id, batch_id, source_video, status, processed_frames, estimated_frames,
                  last_error, created_at, finished_at, events_count
           FROM video_jobs
           WHERE source_video = ?1
           ORDER BY created_at DESC
           LIMIT 1"#,
    )?;
    let mut rows = stmt.query(params![source_video])?;
    if let Some(r) = rows.next()? {
        Ok(Some(row_to_job(r)?))
    } else {
        Ok(None)
    }
}

fn row_to_job(r: &Row<'_>) -> rusqlite::Result<VideoJob> {
    let status_str: String = r.get(3)?;
    let status = JobStatus::parse(&status_str).unwrap_or(JobStatus::Pending);
    Ok(VideoJob {
        id: r.get(0)?,
        batch_id: r.get(1)?,
        source_video: r.get(2)?,
        status,
        processed_frames: r.get::<_, i64>(4)? as u32,
        estimated_frames: r.get::<_, i64>(5)? as u32,
        last_error: r.get(6)?,
        created_at: r.get(7)?,
        finished_at: r.get(8)?,
        events_count: r.get::<_, i64>(9)? as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::job::JobStatus;

    fn sample(id: &str, batch: &str, video: &str) -> VideoJob {
        VideoJob {
            id: id.to_string(),
            batch_id: batch.to_string(),
            source_video: video.to_string(),
            status: JobStatus::Pending,
            processed_frames: 0,
            estimated_frames: 30,
            last_error: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            finished_at: None,
            events_count: 0,
        }
    }

    #[test]
    fn job_lifecycle() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::schema::run_migrations(&conn).unwrap();

        let j = sample("j-1", "b-1", "/tmp/v.mp4");
        insert_job(&conn, &j).unwrap();

        let all = list_all(&conn).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].status, JobStatus::Pending);

        update_status(&conn, "j-1", JobStatus::Running, None, false).unwrap();
        update_progress(&conn, "j-1", 15).unwrap();
        finalize_success(&conn, "j-1", 3).unwrap();

        let all = list_all(&conn).unwrap();
        assert_eq!(all[0].status, JobStatus::Success);
        assert_eq!(all[0].processed_frames, 15);
        assert_eq!(all[0].events_count, 3);
        assert!(all[0].finished_at.is_some());
    }

    #[test]
    fn reset_running_marks_pending() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::schema::run_migrations(&conn).unwrap();

        let mut j = sample("j-1", "b-1", "/tmp/v.mp4");
        j.status = JobStatus::Running;
        // 直接 INSERT 时 status 会按字段; 但 sample 默认 Pending, 我们手动改:
        insert_job(&conn, &j).ok();
        update_status(&conn, "j-1", JobStatus::Running, None, false).unwrap();

        let n = reset_running_to_pending(&conn).unwrap();
        assert_eq!(n, 1);
        let all = list_all(&conn).unwrap();
        assert_eq!(all[0].status, JobStatus::Pending);
        assert!(all[0].last_error.is_some());
    }

    #[test]
    fn find_latest_by_video_returns_newest() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::schema::run_migrations(&conn).unwrap();

        let mut j1 = sample("j-1", "b-1", "/tmp/v.mp4");
        j1.created_at = "2026-05-02T10:00:00Z".to_string();
        let mut j2 = sample("j-2", "b-2", "/tmp/v.mp4");
        j2.created_at = "2026-05-02T11:00:00Z".to_string();
        insert_job(&conn, &j1).unwrap();
        insert_job(&conn, &j2).unwrap();

        let found = find_latest_by_video(&conn, "/tmp/v.mp4").unwrap();
        assert_eq!(found.unwrap().id, "j-2");
    }
}

// Helpers used by orchestrator
pub fn _unused_compile_check(_: &Connection) -> Result<()> {
    Context::context(Ok::<_, anyhow::Error>(()), "_unused").map_err(Into::into)
}
