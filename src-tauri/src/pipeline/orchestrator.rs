// 编排器: 把抽帧 / 车辆检测 / 车牌识别 / 聚合串起来
//
// MVU 4 阶段: extract -> vehicle_detect 完成
// MVU 5 阶段: 在 detect_vehicles_in_frames 内追加 plate.recognize
// MVU 6 阶段: aggregate_events 把 FrameObservation -> Vec<ParkingEvent>

use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use image::RgbImage;

use crate::ai::vehicle::{detector, RELEVANT_CLASSES};
use crate::models::event::ParkingEvent;
use crate::models::observation::{FrameObservation, VehicleObservation};
use crate::pipeline::aggregate;
use crate::video::extract::{extract_frames, ExtractOptions};
use crate::video::metadata::{read_metadata, VideoMetadata};

/// `process_video` 端到端结果, 同时返回中间观测 (调试 / UI 展示)
pub struct ProcessOutcome {
    pub metadata: VideoMetadata,
    pub observations: Vec<FrameObservation>,
    pub events: Vec<ParkingEvent>,
}

/// 单视频处理入口 (P1 端到端)
///
/// 当前默认参数:
///   - target_fps: 1
///   - aggregation_window_secs: 60
///
/// 关于 plate_recognize:
///   true 时调用 HyperLPR3 (MVU 5+); false 时跳过, 用于 MVU 4 单独验证
pub fn process_video(path: &Path, plate_recognize: bool) -> Result<ProcessOutcome> {
    let metadata = read_metadata(path).context("读取视频元数据失败")?;

    let frames = extract_frames(
        path,
        &ExtractOptions {
            target_fps: 1.0,
            max_frames: None,
        },
    )
    .context("抽帧失败")?;

    tracing::info!(extracted_frames = frames.len(), "抽帧完成, 开始车辆检测");

    let observations = detect_vehicles_in_frames(frames, plate_recognize)
        .context("车辆/车牌检测失败")?;

    let event_time_base = parse_creation_time(metadata.creation_time.as_deref());
    let events = aggregate::aggregate_events(
        path,
        &observations,
        event_time_base,
        60_000, /* 60s window */
    );

    Ok(ProcessOutcome {
        metadata,
        observations,
        events,
    })
}

/// 给定抽样帧序列, 调用 YOLOv8 (MVU 4) + 可选 HyperLPR3 (MVU 5)
///
/// 输出每帧的观测 (含车辆 bbox + 可选 plate)
pub fn detect_vehicles_in_frames(
    frames: Vec<crate::video::extract::ExtractedFrame>,
    plate_recognize: bool,
) -> Result<Vec<FrameObservation>> {
    let det_lock = detector().context("加载 YOLOv8 检测器失败")?;

    let mut out = Vec::with_capacity(frames.len());
    for f in frames {
        let det_result = {
            let mut det = det_lock
                .lock()
                .map_err(|e| anyhow::anyhow!("Detector mutex 中毒: {e}"))?;
            det.detect(&f.image)?
        };

        // 只保留我们关心的车辆类别 (filter+masks 同步)
        let det_result_clone_masks = det_result.masks.clone();
        let zipped: Vec<(crate::models::detection::Detection, Option<image::GrayImage>)> =
            det_result
                .detections
                .into_iter()
                .zip(det_result_clone_masks.into_iter())
                .filter(|(d, _)| RELEVANT_CLASSES.contains(&d.class_id))
                .collect();

        // 计算 sidewalk mask + IoU (P3)
        let sidewalk = match crate::ai::sidewalk::segmenter() {
            Ok(seg) => match seg.lock() {
                Ok(mut s) => match s.segment_sidewalk(&f.image) {
                    Ok(m) => Some(m),
                    Err(e) => {
                        tracing::warn!(error = %e, "SegFormer 推理失败, 跳过本帧 IoU");
                        None
                    }
                },
                Err(e) => {
                    tracing::warn!(error = %e, "SegFormer mutex 中毒");
                    None
                }
            },
            Err(e) => {
                tracing::warn!(error = %e, "SegFormer 未加载, 跳过 IoU");
                None
            }
        };

        let mut vehicles: Vec<VehicleObservation> = zipped
            .into_iter()
            .map(|(d, mask)| {
                let iou_score = match (&mask, &sidewalk) {
                    (Some(vm), Some(sm)) => {
                        crate::ai::judge::intersection_over_vehicle(vm, sm).ok()
                    }
                    _ => None,
                };
                VehicleObservation {
                    class_id: d.class_id,
                    class_name: d.class_name,
                    vehicle_score: d.score,
                    bbox: d.bbox,
                    iou_score,
                    plate: None,
                }
            })
            .collect();

        if plate_recognize && !vehicles.is_empty() {
            run_plate_recognition(&f.image, &mut vehicles);
        }

        out.push(FrameObservation {
            frame_index: f.frame_index,
            timestamp_ms: f.timestamp_ms,
            width: f.image.width(),
            height: f.image.height(),
            vehicles,
        });
    }
    Ok(out)
}

/// MVU 5 车牌识别钩子 — 当前为占位实现, 真实逻辑在 ai::plate
fn run_plate_recognition(frame: &RgbImage, vehicles: &mut [VehicleObservation]) {
    // 实际 plate 推理委托到 ai::plate, 失败仅记录, 不阻塞 pipeline
    if let Err(e) = crate::ai::plate::recognize_into(frame, vehicles) {
        tracing::warn!(error = %e, "车牌识别整体失败, 跳过本帧");
    }
}

/// 解析 ISO8601 / RFC3339 创建时间, 失败返回 None
fn parse_creation_time(s: Option<&str>) -> Option<DateTime<Utc>> {
    let s = s?;
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|| {
            // ffmpeg 常见格式: "2026-05-02T11:24:33.000000Z"
            DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.fZ")
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        })
}
