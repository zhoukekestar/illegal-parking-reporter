// P2 并发流水线 (tokio + mpsc + Semaphore)
//
// 架构:
//   单视频内部三阶段串行流水: extract -> infer -> aggregate
//   多视频之间: 各 stage 通过全局 Semaphore 限并发
//     - Stage 1 (抽帧 CPU): 4 并发
//     - Stage 2 (AI 推理): 1 并发 (YOLOv8/HyperLPR3 都是 Mutex<Session>, 多并发也得排队, P2 简单起见限 1)
//     - Stage 3 (聚合 + DB 写入): 4 并发
//
// Tauri Event:
//   - "pipeline:event" 是统一信道, payload 是 PipelineEvent 枚举
//   - 前端节流后渲染进度

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, Semaphore};
use uuid::Uuid;

use crate::ai::vehicle::RELEVANT_CLASSES;
use crate::models::job::{JobStatus, VideoJob};
use crate::models::observation::{FrameObservation, VehicleObservation};
use crate::pipeline::aggregate::aggregate_events;
use crate::video::extract::{extract_frames_with_callback, ExtractOptions, ExtractedFrame};
use crate::video::metadata::read_metadata;

pub const EVENT_NAME: &str = "pipeline:event";

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Extract,
    Infer,
    Aggregate,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PipelineEvent {
    BatchStarted {
        batch_id: String,
        total: usize,
    },
    JobStarted {
        batch_id: String,
        job_id: String,
        video: String,
    },
    JobProgress {
        batch_id: String,
        job_id: String,
        stage: Stage,
        processed: u32,
        total: u32,
    },
    JobSucceeded {
        batch_id: String,
        job_id: String,
        events_count: u32,
        duration_ms: u64,
    },
    JobFailed {
        batch_id: String,
        job_id: String,
        error: String,
    },
    BatchFinished {
        batch_id: String,
        success_count: u32,
        fail_count: u32,
        duration_ms: u64,
    },
}

fn emit(app: &AppHandle, payload: PipelineEvent) {
    if let Err(e) = app.emit(EVENT_NAME, payload) {
        tracing::warn!(error = %e, "emit pipeline event 失败");
    }
}

// ========== 并发配额 ==========

#[derive(Clone)]
struct Stages {
    s1: Arc<Semaphore>,
    s2: Arc<Semaphore>,
    s3: Arc<Semaphore>,
}

fn default_stages() -> Stages {
    Stages {
        s1: Arc::new(Semaphore::new(4)),
        s2: Arc::new(Semaphore::new(1)),
        s3: Arc::new(Semaphore::new(4)),
    }
}

// ========== 公共入口 ==========

pub struct StartBatchOutcome {
    pub batch_id: String,
    pub job_count: usize,
}

/// 启动一个批处理: 立即返回 batch_id, 后续通过 Tauri Event 推送进度
///
/// 已经处理过的视频 (status=success) 会被跳过 (断点续传)
pub async fn start_batch(app: AppHandle, paths: Vec<PathBuf>) -> Result<StartBatchOutcome> {
    let batch_id = Uuid::new_v4().to_string();

    // 1. 入库: 为每个视频创建 job, 跳过已成功的
    let jobs = create_jobs(&batch_id, &paths)?;
    let total_jobs = jobs.len();

    // 2. 通知 BatchStarted
    emit(
        &app,
        PipelineEvent::BatchStarted {
            batch_id: batch_id.clone(),
            total: total_jobs,
        },
    );

    if jobs.is_empty() {
        emit(
            &app,
            PipelineEvent::BatchFinished {
                batch_id: batch_id.clone(),
                success_count: 0,
                fail_count: 0,
                duration_ms: 0,
            },
        );
        return Ok(StartBatchOutcome { batch_id, job_count: 0 });
    }

    // 3. 派发: 每个 job 一个 task, 共享 Semaphore
    spawn_batch_tasks(app, batch_id.clone(), jobs);

    Ok(StartBatchOutcome {
        batch_id,
        job_count: total_jobs,
    })
}

/// 续跑数据库里所有 status='pending' 的任务 (启动后调用)
pub async fn resume_pending(app: AppHandle) -> Result<StartBatchOutcome> {
    let pending: Vec<VideoJob> = {
        let lock = crate::db::conn()?;
        let conn = lock.lock().map_err(|e| anyhow::anyhow!("DB mutex 中毒: {e}"))?;
        crate::db::jobs::list_pending(&conn)?
    };

    if pending.is_empty() {
        return Ok(StartBatchOutcome {
            batch_id: String::new(),
            job_count: 0,
        });
    }

    let resume_batch_id = format!("resume-{}", Uuid::new_v4());
    emit(
        &app,
        PipelineEvent::BatchStarted {
            batch_id: resume_batch_id.clone(),
            total: pending.len(),
        },
    );

    spawn_batch_tasks(app, resume_batch_id.clone(), pending.clone());

    Ok(StartBatchOutcome {
        batch_id: resume_batch_id,
        job_count: pending.len(),
    })
}

// ========== 内部 ==========

/// 入库 jobs, 跳过已 success 的视频 (断点续传)
fn create_jobs(batch_id: &str, paths: &[PathBuf]) -> Result<Vec<VideoJob>> {
    let lock = crate::db::conn()?;
    let mut conn = lock.lock().map_err(|e| anyhow::anyhow!("DB mutex 中毒: {e}"))?;
    let tx = conn.transaction()?;

    let mut jobs = Vec::new();
    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    for path in paths {
        let video_str = path.to_string_lossy().to_string();
        if let Some(prev) = crate::db::jobs::find_latest_by_video(&tx, &video_str)? {
            if prev.status == JobStatus::Success {
                tracing::info!(video = %video_str, "已成功处理过, 跳过");
                continue;
            }
        }
        let job = VideoJob {
            id: Uuid::new_v4().to_string(),
            batch_id: batch_id.to_string(),
            source_video: video_str,
            status: JobStatus::Pending,
            processed_frames: 0,
            estimated_frames: 0,
            last_error: None,
            created_at: now.clone(),
            finished_at: None,
            events_count: 0,
        };
        crate::db::jobs::insert_job(&tx, &job)?;
        jobs.push(job);
    }
    tx.commit()?;
    Ok(jobs)
}

fn spawn_batch_tasks(app: AppHandle, batch_id: String, jobs: Vec<VideoJob>) {
    let stages = default_stages();
    let total = jobs.len() as u32;
    let started = Instant::now();
    let success_counter = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let fail_counter = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let remaining = Arc::new(std::sync::atomic::AtomicU32::new(total));

    for job in jobs {
        let app = app.clone();
        let stages = stages.clone();
        let batch_id = batch_id.clone();
        let succ = success_counter.clone();
        let fail = fail_counter.clone();
        let remaining = remaining.clone();
        tokio::spawn(async move {
            let res = process_one_job(app.clone(), batch_id.clone(), job.clone(), stages).await;
            match res {
                Ok(events_count) => {
                    succ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let _ = mark_job_success(&job.id, events_count);
                    emit(
                        &app,
                        PipelineEvent::JobSucceeded {
                            batch_id: batch_id.clone(),
                            job_id: job.id.clone(),
                            events_count,
                            duration_ms: 0, // 单 job 时间在 process_one_job 内已计, 此处简化为 0
                        },
                    );
                }
                Err(e) => {
                    fail.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let msg = format!("{e:#}");
                    let _ = mark_job_failed(&job.id, &msg);
                    emit(
                        &app,
                        PipelineEvent::JobFailed {
                            batch_id: batch_id.clone(),
                            job_id: job.id.clone(),
                            error: msg,
                        },
                    );
                }
            }
            // 最后一个 job 完成时, 派发 BatchFinished
            let left = remaining.fetch_sub(1, std::sync::atomic::Ordering::Relaxed) - 1;
            if left == 0 {
                emit(
                    &app,
                    PipelineEvent::BatchFinished {
                        batch_id,
                        success_count: succ.load(std::sync::atomic::Ordering::Relaxed),
                        fail_count: fail.load(std::sync::atomic::Ordering::Relaxed),
                        duration_ms: started.elapsed().as_millis() as u64,
                    },
                );
            }
        });
    }
}

/// 单个视频的三阶段流水: extract -> infer -> aggregate
async fn process_one_job(
    app: AppHandle,
    batch_id: String,
    job: VideoJob,
    stages: Stages,
) -> Result<u32> {
    let path: PathBuf = PathBuf::from(&job.source_video);
    let job_id = job.id.clone();

    // 状态置 Running
    set_job_running(&job_id)?;
    emit(
        &app,
        PipelineEvent::JobStarted {
            batch_id: batch_id.clone(),
            job_id: job_id.clone(),
            video: job.source_video.clone(),
        },
    );

    // 读元数据 (估算总帧数)
    let metadata = {
        let p = path.clone();
        tokio::task::spawn_blocking(move || read_metadata(&p))
            .await
            .map_err(|e| anyhow::anyhow!("读 metadata 任务 panic: {e}"))?
            .context("读元数据失败")?
    };
    let est_frames = (metadata.duration_seconds.max(0.0)).ceil().max(1.0) as u32;
    set_job_estimated(&job_id, est_frames)?;
    let event_time_base = metadata
        .creation_time
        .as_deref()
        .and_then(|s| {
            DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
                .or_else(|| {
                    DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.fZ")
                        .ok()
                        .map(|dt| dt.with_timezone(&Utc))
                })
        });

    // mpsc: stage1 -> stage2 -> stage3
    let (frame_tx, frame_rx) = mpsc::channel::<ExtractedFrame>(4);
    let (obs_tx, obs_rx) = mpsc::channel::<FrameObservation>(8);

    // Stage 1
    let s1 = stages.s1.clone();
    let app_s1 = app.clone();
    let batch_s1 = batch_id.clone();
    let job_s1 = job_id.clone();
    let path_s1 = path.clone();
    let stage1 = tokio::spawn(async move {
        let _permit = s1
            .acquire_owned()
            .await
            .map_err(|_| anyhow::anyhow!("stage1 semaphore closed"))?;
        let frame_tx = frame_tx;
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut last_emit = Instant::now();
            extract_frames_with_callback(
                &path_s1,
                &ExtractOptions::default(),
                |frame| {
                    let idx = frame.frame_index as u32 + 1;
                    // 节流: 至少 100ms emit 一次
                    if last_emit.elapsed().as_millis() >= 100 || idx == est_frames {
                        emit(
                            &app_s1,
                            PipelineEvent::JobProgress {
                                batch_id: batch_s1.clone(),
                                job_id: job_s1.clone(),
                                stage: Stage::Extract,
                                processed: idx,
                                total: est_frames,
                            },
                        );
                        last_emit = Instant::now();
                    }
                    frame_tx
                        .blocking_send(frame)
                        .map_err(|_| anyhow::anyhow!("stage1->2 channel 已关闭"))
                },
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("stage1 blocking task panic: {e}"))??;
        Ok::<(), anyhow::Error>(())
    });

    // Stage 2
    let s2 = stages.s2.clone();
    let app_s2 = app.clone();
    let batch_s2 = batch_id.clone();
    let job_s2 = job_id.clone();
    let stage2 = tokio::spawn(async move {
        let mut frame_rx = frame_rx;
        let obs_tx = obs_tx;
        let mut count: u32 = 0;
        let mut last_emit = Instant::now();
        while let Some(frame) = frame_rx.recv().await {
            let _permit = s2
                .acquire()
                .await
                .map_err(|_| anyhow::anyhow!("stage2 semaphore closed"))?;
            let obs = tokio::task::spawn_blocking(move || infer_one_frame(frame))
                .await
                .map_err(|e| anyhow::anyhow!("stage2 blocking task panic: {e}"))??;
            count += 1;
            if last_emit.elapsed().as_millis() >= 100 || count == est_frames {
                emit(
                    &app_s2,
                    PipelineEvent::JobProgress {
                        batch_id: batch_s2.clone(),
                        job_id: job_s2.clone(),
                        stage: Stage::Infer,
                        processed: count,
                        total: est_frames,
                    },
                );
                set_job_progress(&job_s2, count)?;
                last_emit = Instant::now();
            }
            if obs_tx.send(obs).await.is_err() {
                break;
            }
        }
        Ok::<u32, anyhow::Error>(count)
    });

    // Stage 3
    let s3 = stages.s3.clone();
    let app_s3 = app.clone();
    let batch_s3 = batch_id.clone();
    let job_s3 = job_id.clone();
    let path_s3 = path.clone();
    let stage3 = tokio::spawn(async move {
        let _permit = s3
            .acquire_owned()
            .await
            .map_err(|_| anyhow::anyhow!("stage3 semaphore closed"))?;
        let mut obs_rx = obs_rx;
        let mut all: Vec<FrameObservation> = Vec::new();
        while let Some(obs) = obs_rx.recv().await {
            all.push(obs);
        }
        let total_frames = all.len() as u32;
        emit(
            &app_s3,
            PipelineEvent::JobProgress {
                batch_id: batch_s3.clone(),
                job_id: job_s3.clone(),
                stage: Stage::Aggregate,
                processed: 0,
                total: total_frames,
            },
        );
        let events = aggregate_events(&path_s3, &all, event_time_base, 60_000);
        let events_count = events.len() as u32;

        // DB 写入
        if !events.is_empty() {
            let events_clone = events.clone();
            tokio::task::spawn_blocking(move || -> Result<()> {
                let lock = crate::db::conn()?;
                let mut conn = lock.lock().map_err(|e| anyhow::anyhow!("DB mutex 中毒: {e}"))?;
                crate::db::events::save_events(&mut conn, &events_clone)?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("stage3 DB 任务 panic: {e}"))??;
        }

        emit(
            &app_s3,
            PipelineEvent::JobProgress {
                batch_id: batch_s3.clone(),
                job_id: job_s3.clone(),
                stage: Stage::Aggregate,
                processed: total_frames,
                total: total_frames,
            },
        );

        Ok::<u32, anyhow::Error>(events_count)
    });

    // 等三个阶段全部结束
    stage1
        .await
        .map_err(|e| anyhow::anyhow!("stage1 join 失败: {e}"))??;
    let _processed = stage2
        .await
        .map_err(|e| anyhow::anyhow!("stage2 join 失败: {e}"))??;
    let events_count = stage3
        .await
        .map_err(|e| anyhow::anyhow!("stage3 join 失败: {e}"))??;

    Ok(events_count)
}

/// 单帧推理: YOLOv8 + HyperLPR3 (与 orchestrator::detect_vehicles_in_frames 内部一致)
fn infer_one_frame(extracted: ExtractedFrame) -> Result<FrameObservation> {
    let det_lock = crate::ai::vehicle::detector()?;
    let det_result = {
        let mut det = det_lock
            .lock()
            .map_err(|e| anyhow::anyhow!("Vehicle detector mutex 中毒: {e}"))?;
        det.detect(&extracted.image)?
    };

    let mut vehicles: Vec<VehicleObservation> = det_result
        .detections
        .into_iter()
        .filter(|d| RELEVANT_CLASSES.contains(&d.class_id))
        .map(|d| VehicleObservation {
            class_id: d.class_id,
            class_name: d.class_name,
            vehicle_score: d.score,
            bbox: d.bbox,
            plate: None,
        })
        .collect();

    if !vehicles.is_empty() {
        if let Err(e) = crate::ai::plate::recognize_into(&extracted.image, &mut vehicles) {
            tracing::warn!(error = %e, frame = extracted.frame_index, "车牌识别失败, 跳过");
        }
    }

    Ok(FrameObservation {
        frame_index: extracted.frame_index,
        timestamp_ms: extracted.timestamp_ms,
        width: extracted.image.width(),
        height: extracted.image.height(),
        vehicles,
    })
}

// ========== DB 状态封装 (内部用) ==========

fn set_job_running(job_id: &str) -> Result<()> {
    let lock = crate::db::conn()?;
    let conn = lock.lock().map_err(|e| anyhow::anyhow!("DB mutex 中毒: {e}"))?;
    crate::db::jobs::update_status(&conn, job_id, JobStatus::Running, None, false)
}

fn set_job_progress(job_id: &str, processed: u32) -> Result<()> {
    let lock = crate::db::conn()?;
    let conn = lock.lock().map_err(|e| anyhow::anyhow!("DB mutex 中毒: {e}"))?;
    crate::db::jobs::update_progress(&conn, job_id, processed)
}

fn set_job_estimated(job_id: &str, est: u32) -> Result<()> {
    let lock = crate::db::conn()?;
    let conn = lock.lock().map_err(|e| anyhow::anyhow!("DB mutex 中毒: {e}"))?;
    conn.execute(
        "UPDATE video_jobs SET estimated_frames = ?1 WHERE id = ?2",
        rusqlite::params![est as i64, job_id],
    )?;
    Ok(())
}

fn mark_job_success(job_id: &str, events_count: u32) -> Result<()> {
    let lock = crate::db::conn()?;
    let conn = lock.lock().map_err(|e| anyhow::anyhow!("DB mutex 中毒: {e}"))?;
    crate::db::jobs::finalize_success(&conn, job_id, events_count)
}

fn mark_job_failed(job_id: &str, err: &str) -> Result<()> {
    let lock = crate::db::conn()?;
    let conn = lock.lock().map_err(|e| anyhow::anyhow!("DB mutex 中毒: {e}"))?;
    crate::db::jobs::update_status(&conn, job_id, JobStatus::Failed, Some(err), true)
}

// 让 chrono::Duration 在某些子模块需要时可用 (orchestrator 依赖, 此处不必)
#[allow(dead_code)]
fn _retain_chrono_dep() -> ChronoDuration {
    ChronoDuration::seconds(1)
}

#[allow(dead_code)]
fn _retain_path_use(p: &Path) -> &Path {
    p
}
