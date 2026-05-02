// 事件聚合 (P1 简化版, 用户反馈后强化过滤)
//
// 规则:
//   - 必须有 plate (HyperLPR3 识别成功)
//   - plate.text 必须符合中国车牌格式 (is_valid_chinese_plate)
//   - plate.confidence >= min_plate_confidence (默认 0.6, settings 可调)
//   - 同视频内, 同车牌, window_ms 内多帧合并为一个事件
//   - 取车牌识别置信度最高的那一帧作为代表
//
// 不合法的 plate (空/<待确认>/格式乱码) 直接丢弃, 不再生成 <待确认> 占位事件
// (用户反馈: 太多无效事件淹没列表)

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Duration, Utc};

use crate::models::event::{ParkingEvent, ReviewStatus};
use crate::models::observation::{FrameObservation, VehicleObservation};

/// 聚合单个视频的所有帧观测, 输出 Vec<ParkingEvent>
///
/// `event_time_base` 是视频拍摄时间 (creation_time), 可选; None 时事件不带 event_time
/// `min_plate_confidence`: 低于此值的车牌识别丢弃
pub fn aggregate_events(
    source_video: &Path,
    observations: &[FrameObservation],
    event_time_base: Option<DateTime<Utc>>,
    window_ms: i64,
    min_plate_confidence: f32,
) -> Vec<ParkingEvent> {
    let mut buckets: HashMap<String, Vec<FrameSample>> = HashMap::new();
    let mut total_skipped_no_plate = 0usize;
    let mut total_skipped_invalid = 0usize;
    let mut total_skipped_low_conf = 0usize;
    let mut total_kept = 0usize;

    for frame in observations {
        for v in &frame.vehicles {
            let plate = match &v.plate {
                Some(p) => p,
                None => {
                    total_skipped_no_plate += 1;
                    continue;
                }
            };
            if !crate::ai::plate::is_valid_chinese_plate(&plate.text) {
                total_skipped_invalid += 1;
                continue;
            }
            if plate.confidence < min_plate_confidence {
                total_skipped_low_conf += 1;
                continue;
            }
            total_kept += 1;
            let key = format!("plate::{}", plate.text);
            buckets.entry(key).or_default().push(FrameSample {
                frame_index: frame.frame_index,
                timestamp_ms: frame.timestamp_ms,
                vehicle: v.clone(),
            });
        }
    }
    tracing::info!(
        kept = total_kept,
        skipped_no_plate = total_skipped_no_plate,
        skipped_invalid = total_skipped_invalid,
        skipped_low_conf = total_skipped_low_conf,
        min_plate_confidence,
        "聚合阶段过滤统计"
    );

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

// (旧 bucket_key 已并入 aggregate_events 过滤逻辑, 不再使用)

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

    // 此时一定有 plate (aggregate_events 已过滤), 否则不会进 bucket
    let plate = representative
        .vehicle
        .plate
        .as_ref()
        .expect("representative 必有合法 plate");
    let (plate_number, plate_confidence) = (plate.text.clone(), plate.confidence);

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

    const MIN_CONF: f32 = 0.6;

    #[test]
    fn merge_same_plate_within_window() {
        let obs = vec![
            make_obs(0, 0, Some("浙A12345"), 0.7),
            make_obs(1, 1000, Some("浙A12345"), 0.95),
            make_obs(2, 2000, Some("浙A12345"), 0.6),
        ];
        let evs = aggregate_events(&PathBuf::from("v.mp4"), &obs, None, 60_000, MIN_CONF);
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].plate_number, "浙A12345");
        assert_eq!(evs[0].representative_frame_index, 1);
        assert_eq!(evs[0].first_seen_ms, 0);
        assert_eq!(evs[0].last_seen_ms, 2000);
        assert_eq!(evs[0].frame_hits, 3);
    }

    #[test]
    fn split_when_outside_window() {
        let obs = vec![
            make_obs(0, 0, Some("浙A12345"), 0.9),
            make_obs(1, 70_000, Some("浙A12345"), 0.9),
        ];
        let evs = aggregate_events(&PathBuf::from("v.mp4"), &obs, None, 60_000, MIN_CONF);
        assert_eq!(evs.len(), 2);
    }

    #[test]
    fn different_plates_make_different_events() {
        let obs = vec![
            make_obs(0, 0, Some("浙A12345"), 0.9),
            make_obs(1, 1000, Some("浙B88888"), 0.9),
        ];
        let evs = aggregate_events(&PathBuf::from("v.mp4"), &obs, None, 60_000, MIN_CONF);
        assert_eq!(evs.len(), 2);
    }

    #[test]
    fn no_plate_observations_produce_no_events() {
        // P1 强化后: 无 plate 直接丢弃, 不再生成 <待确认>
        let obs = vec![
            make_obs(0, 0, None, 0.0),
            make_obs(1, 1000, None, 0.0),
        ];
        let evs = aggregate_events(&PathBuf::from("v.mp4"), &obs, None, 60_000, MIN_CONF);
        assert!(evs.is_empty());
    }

    #[test]
    fn invalid_plate_format_filtered_out() {
        // OCR 乱码 / 不符合中国格式 -> 丢弃
        let obs = vec![
            make_obs(0, 0, Some("ABC1234"), 0.95), // 首位不是省份
            make_obs(1, 1000, Some("浙IO234"), 0.95), // 含 I/O
            make_obs(2, 2000, Some("浙A12345"), 0.95), // 合法, 应保留
        ];
        let evs = aggregate_events(&PathBuf::from("v.mp4"), &obs, None, 60_000, MIN_CONF);
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].plate_number, "浙A12345");
    }

    #[test]
    fn low_confidence_plate_filtered_out() {
        let obs = vec![
            make_obs(0, 0, Some("浙A12345"), 0.4), // < 0.6 丢弃
            make_obs(1, 1000, Some("浙A12345"), 0.95), // 保留
        ];
        let evs = aggregate_events(&PathBuf::from("v.mp4"), &obs, None, 60_000, MIN_CONF);
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].frame_hits, 1);
    }
}
