// events 表的 CRUD

use anyhow::{Context, Result};
use rusqlite::{params, Connection, Row};

use crate::models::event::{ParkingEvent, ReviewStatus};

pub fn upsert_event(conn: &Connection, e: &ParkingEvent) -> Result<()> {
    let bbox_json = serde_json::to_string(&e.vehicle_bbox).context("序列化 bbox 失败")?;
    conn.execute(
        r#"INSERT INTO events (
            id, source_video, representative_frame_index, timestamp_ms, event_time,
            plate_number, plate_confidence, plate_manual_corrected, vehicle_class,
            vehicle_bbox_json, first_seen_ms, last_seen_ms, frame_hits, review_status,
            iou_score, snapshot_path, clip_path
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17
        )
        ON CONFLICT(id) DO UPDATE SET
            source_video = excluded.source_video,
            representative_frame_index = excluded.representative_frame_index,
            timestamp_ms = excluded.timestamp_ms,
            event_time = excluded.event_time,
            plate_number = excluded.plate_number,
            plate_confidence = excluded.plate_confidence,
            plate_manual_corrected = excluded.plate_manual_corrected,
            vehicle_class = excluded.vehicle_class,
            vehicle_bbox_json = excluded.vehicle_bbox_json,
            first_seen_ms = excluded.first_seen_ms,
            last_seen_ms = excluded.last_seen_ms,
            frame_hits = excluded.frame_hits,
            review_status = excluded.review_status,
            iou_score = excluded.iou_score,
            snapshot_path = excluded.snapshot_path,
            clip_path = excluded.clip_path"#,
        params![
            e.id,
            e.source_video,
            e.representative_frame_index as i64,
            e.timestamp_ms,
            e.event_time,
            e.plate_number,
            e.plate_confidence,
            e.plate_manual_corrected,
            e.vehicle_class,
            bbox_json,
            e.first_seen_ms,
            e.last_seen_ms,
            e.frame_hits as i64,
            e.review_status.as_str(),
            e.iou_score,
            e.snapshot_path,
            e.clip_path,
        ],
    )?;
    Ok(())
}

pub fn save_events(conn: &mut Connection, events: &[ParkingEvent]) -> Result<()> {
    let tx = conn.transaction()?;
    for e in events {
        upsert_event(&tx, e)?;
    }
    tx.commit()?;
    Ok(())
}

pub fn list_all(conn: &Connection) -> Result<Vec<ParkingEvent>> {
    let mut stmt = conn.prepare(
        r#"SELECT id, source_video, representative_frame_index, timestamp_ms, event_time,
                  plate_number, plate_confidence, plate_manual_corrected, vehicle_class,
                  vehicle_bbox_json, first_seen_ms, last_seen_ms, frame_hits, review_status,
                  iou_score, snapshot_path, clip_path
           FROM events
           ORDER BY timestamp_ms DESC, id"#,
    )?;
    let rows = stmt.query_map([], row_to_event)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn list_by_source_video(conn: &Connection, source_video: &str) -> Result<Vec<ParkingEvent>> {
    let mut stmt = conn.prepare(
        r#"SELECT id, source_video, representative_frame_index, timestamp_ms, event_time,
                  plate_number, plate_confidence, plate_manual_corrected, vehicle_class,
                  vehicle_bbox_json, first_seen_ms, last_seen_ms, frame_hits, review_status,
                  iou_score, snapshot_path, clip_path
           FROM events
           WHERE source_video = ?1
           ORDER BY timestamp_ms"#,
    )?;
    let rows = stmt.query_map(params![source_video], row_to_event)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

fn row_to_event(r: &Row<'_>) -> rusqlite::Result<ParkingEvent> {
    let bbox_json: String = r.get(9)?;
    let bbox: [f32; 4] = serde_json::from_str(&bbox_json)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(9, rusqlite::types::Type::Text, Box::new(e)))?;
    let status_str: String = r.get(13)?;
    let review_status = ReviewStatus::parse(&status_str).unwrap_or(ReviewStatus::Pending);

    Ok(ParkingEvent {
        id: r.get(0)?,
        source_video: r.get(1)?,
        representative_frame_index: r.get::<_, i64>(2)? as usize,
        timestamp_ms: r.get(3)?,
        event_time: r.get(4)?,
        plate_number: r.get(5)?,
        plate_confidence: r.get::<_, f64>(6)? as f32,
        plate_manual_corrected: r.get(7)?,
        vehicle_class: r.get(8)?,
        vehicle_bbox: bbox,
        first_seen_ms: r.get(10)?,
        last_seen_ms: r.get(11)?,
        frame_hits: r.get::<_, i64>(12)? as u32,
        review_status,
        iou_score: r.get::<_, Option<f64>>(14)?.map(|v| v as f32),
        snapshot_path: r.get(15)?,
        clip_path: r.get(16)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::event::ParkingEvent;

    fn sample_event(id: &str, plate: &str, ts: i64) -> ParkingEvent {
        ParkingEvent {
            id: id.to_string(),
            source_video: "/tmp/v.mp4".to_string(),
            representative_frame_index: 0,
            timestamp_ms: ts,
            event_time: None,
            plate_number: plate.to_string(),
            plate_confidence: 0.9,
            plate_manual_corrected: None,
            vehicle_class: "car".to_string(),
            vehicle_bbox: [10.0, 20.0, 110.0, 120.0],
            first_seen_ms: ts,
            last_seen_ms: ts + 1000,
            frame_hits: 2,
            review_status: ReviewStatus::Pending,
            iou_score: None,
            snapshot_path: None,
            clip_path: None,
        }
    }

    #[test]
    fn roundtrip_event() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::schema::run_migrations(&conn).unwrap();

        let e1 = sample_event("id-1", "浙A12345", 1000);
        let e2 = sample_event("id-2", "浙B88888", 2000);
        save_events(&mut conn, &[e1.clone(), e2.clone()]).unwrap();

        let all = list_all(&conn).unwrap();
        assert_eq!(all.len(), 2);
        // 倒序 (timestamp_ms DESC)
        assert_eq!(all[0].id, "id-2");
        assert_eq!(all[1].id, "id-1");
        assert_eq!(all[1].plate_number, "浙A12345");
        assert_eq!(all[1].vehicle_bbox, e1.vehicle_bbox);
    }

    #[test]
    fn upsert_overwrites() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::schema::run_migrations(&conn).unwrap();

        let mut e = sample_event("same-id", "x", 100);
        save_events(&mut conn, &[e.clone()]).unwrap();
        e.plate_number = "y".to_string();
        save_events(&mut conn, &[e.clone()]).unwrap();

        let all = list_all(&conn).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].plate_number, "y");
    }
}
