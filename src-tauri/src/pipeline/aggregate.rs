// 事件聚合 (P1 简化版)
//
// 规则 (DEVELOPMENT_PLAN.md §五 P1):
//   - 同视频内, 同车牌, 60 秒内的多个观测合并为一个事件
//   - 取车牌识别置信度最高的那一帧作为代表
//   - 记录 first_seen 到 last_seen 的时间窗
//   - 车牌识别失败的帧 (plate=None) 也保留为 "未知车牌" 事件, 用稳定的 bbox 中心做 key
//     (P4 审核 UI 会强制用户手动输入车牌才能采纳, 见 §五 P4)

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Duration, Utc};

use crate::models::event::{ParkingEvent, ReviewStatus};
use crate::models::observation::{FrameObservation, VehicleObservation};

const UNKNOWN_PLATE: &str = "<待确认>";

/// 聚合单个视频的所有帧观测, 输出 Vec<ParkingEvent>
///
/// `event_time_base` 是视频拍摄时间 (creation_time), 可选; None 时事件不带 event_time
pub fn aggregate_events(
    source_video: &Path,
    observations: &[FrameObservation],
    event_time_base: Option<DateTime<Utc>>,
    window_ms: i64,
) -> Vec<ParkingEvent> {
    let mut buckets: HashMap<String, Vec<FrameSample>> = HashMap::new();

    for frame in observations {
        for v in &frame.vehicles {
            let key = bucket_key(v);
            buckets.entry(key).or_default().push(FrameSample {
                frame_index: frame.frame_index,
                timestamp_ms: frame.timestamp_ms,
                vehicle: v.clone(),
            });
        }
    }

    let source_str = source_video.to_string_lossy().to_string();
    let mut events = Vec::new();

    for (_, mut samples) in buckets {
        samples.sort_by_key(|s| s.timestamp_ms);
        // 按 60s 窗口切分: 第一个样本起 60s 内为同一组, 超出则开新组
        let mut window_start = samples.first().map(|s| s.timestamp_ms).unwrap_or(0);
        let mut current: Vec<FrameSample> = Vec::new();

        let flush = |group: &mut Vec<FrameSample>, events: &mut Vec<ParkingEvent>| {
            if group.is_empty() {
                return;
            }
            let evt = build_event(&source_str, group, event_time_base);
            events.push(evt);
            group.clear();
        };

        for s in samples.into_iter() {
            if s.timestamp_ms - window_start > window_ms {
                flush(&mut current, &mut events);
                window_start = s.timestamp_ms;
            }
            current.push(s);
        }
        flush(&mut current, &mut events);
    }

    // 按时间排序, UI 展示更直观
    events.sort_by_key(|e| (e.timestamp_ms, e.plate_number.clone()));
    events
}

#[derive(Debug, Clone)]
struct FrameSample {
    frame_index: usize,
    timestamp_ms: i64,
    vehicle: VehicleObservation,
}

/// 同车辆判定 key:
///   - 有车牌: 用 plate.text
///   - 无车牌: 用 bbox 中心粗粒度量化, 静止违停车辆每帧 bbox 接近, 量化后会落入同一桶
fn bucket_key(v: &VehicleObservation) -> String {
    if let Some(p) = &v.plate {
        if !p.text.is_empty() {
            return format!("plate::{}", p.text);
        }
    }
    // 无车牌走 bbox 中心 50px 量化 (针对静止车辆有效, 移动车辆 P2 提前判定模块解决)
    let cx = (v.bbox[0] + v.bbox[2]) / 2.0;
    let cy = (v.bbox[1] + v.bbox[3]) / 2.0;
    format!("nocls::{}::{}::{}",
        v.class_id,
        (cx / 50.0).round() as i32,
        (cy / 50.0).round() as i32
    )
}

/// 把同一 bucket 的样本组装成单个 ParkingEvent
fn build_event(
    source_video: &str,
    samples: &[FrameSample],
    event_time_base: Option<DateTime<Utc>>,
) -> ParkingEvent {
    // 代表帧: 优先 plate.confidence 最高, 没车牌则 vehicle_score 最高
    let representative = samples
        .iter()
        .max_by(|a, b| {
            let sa = a.vehicle.plate.as_ref().map(|p| p.confidence)
                .unwrap_or(a.vehicle.vehicle_score);
            let sb = b.vehicle.plate.as_ref().map(|p| p.confidence)
                .unwrap_or(b.vehicle.vehicle_score);
            sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("samples 非空");

    let first_seen_ms = samples.first().map(|s| s.timestamp_ms).unwrap_or(0);
    let last_seen_ms = samples.last().map(|s| s.timestamp_ms).unwrap_or(0);

    let (plate_number, plate_confidence) = match &representative.vehicle.plate {
        Some(p) => (p.text.clone(), p.confidence),
        None => (UNKNOWN_PLATE.to_string(), 0.0),
    };

    let event_time = event_time_base.map(|base| {
        (base + Duration::milliseconds(representative.timestamp_ms))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    });

    ParkingEvent {
        id: ParkingEvent::new_id(),
        source_video: source_video.to_string(),
        representative_frame_index: representative.frame_index,
        timestamp_ms: representative.timestamp_ms,
        event_time,
        plate_number,
        plate_confidence,
        plate_manual_corrected: None,
        vehicle_class: representative.vehicle.class_name.clone(),
        vehicle_bbox: representative.vehicle.bbox,
        first_seen_ms,
        last_seen_ms,
        frame_hits: samples.len() as u32,
        review_status: ReviewStatus::default(),
        iou_score: representative.vehicle.iou_score,
        snapshot_path: None,
        clip_path: None,
        exported_at: None,
        export_path: None,
        uploaded_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::observation::{PlateReading, VehicleObservation};
    use std::path::PathBuf;

    fn make_obs(frame_index: usize, t_ms: i64, plate: Option<&str>, conf: f32) -> FrameObservation {
        FrameObservation {
            frame_index,
            timestamp_ms: t_ms,
            width: 1920,
            height: 1080,
            vehicles: vec![VehicleObservation {
                class_id: 2,
                class_name: "car".to_string(),
                vehicle_score: 0.9,
                bbox: [10.0, 20.0, 110.0, 120.0],
                iou_score: Some(0.5),
                plate: plate.map(|t| PlateReading {
                    text: t.to_string(),
                    confidence: conf,
                    plate_bbox: [50.0, 70.0, 100.0, 90.0],
                }),
            }],
        }
    }

    #[test]
    fn merge_same_plate_within_window() {
        let obs = vec![
            make_obs(0, 0, Some("浙A12345"), 0.7),
            make_obs(1, 1000, Some("浙A12345"), 0.95),
            make_obs(2, 2000, Some("浙A12345"), 0.6),
        ];
        let evs = aggregate_events(&PathBuf::from("v.mp4"), &obs, None, 60_000);
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].plate_number, "浙A12345");
        // representative 是 0.95 那帧
        assert_eq!(evs[0].representative_frame_index, 1);
        assert_eq!(evs[0].first_seen_ms, 0);
        assert_eq!(evs[0].last_seen_ms, 2000);
        assert_eq!(evs[0].frame_hits, 3);
    }

    #[test]
    fn split_when_outside_window() {
        let obs = vec![
            make_obs(0, 0, Some("浙A12345"), 0.9),
            make_obs(1, 70_000, Some("浙A12345"), 0.9), // 70s > 60s
        ];
        let evs = aggregate_events(&PathBuf::from("v.mp4"), &obs, None, 60_000);
        assert_eq!(evs.len(), 2);
    }

    #[test]
    fn different_plates_make_different_events() {
        let obs = vec![
            make_obs(0, 0, Some("浙A12345"), 0.9),
            make_obs(1, 1000, Some("浙B88888"), 0.9),
        ];
        let evs = aggregate_events(&PathBuf::from("v.mp4"), &obs, None, 60_000);
        assert_eq!(evs.len(), 2);
    }

    #[test]
    fn no_plate_buckets_by_bbox_center() {
        // 同一辆静止车多帧 bbox 几乎一致 -> 应聚合为一个事件
        let obs = vec![
            make_obs(0, 0, None, 0.0),
            make_obs(1, 1000, None, 0.0),
        ];
        let evs = aggregate_events(&PathBuf::from("v.mp4"), &obs, None, 60_000);
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].plate_number, "<待确认>");
    }
}
