import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** 单视频任务, 与 Rust VideoJob 对应 */
export interface VideoJob {
  id: string;
  batch_id: string;
  source_video: string;
  status: "pending" | "running" | "success" | "failed";
  processed_frames: number;
  estimated_frames: number;
  last_error: string | null;
  created_at: string;
  finished_at: string | null;
  events_count: number;
}

export interface StartBatchOutcome {
  batch_id: string;
  job_count: number;
}

export type Stage = "extract" | "infer" | "aggregate";

/** Pipeline Tauri Event payload (tag-based discriminated union) */
export type PipelineEvent =
  | { type: "batch_started"; batch_id: string; total: number }
  | { type: "job_started"; batch_id: string; job_id: string; video: string }
  | {
      type: "job_progress";
      batch_id: string;
      job_id: string;
      stage: Stage;
      processed: number;
      total: number;
    }
  | {
      type: "job_succeeded";
      batch_id: string;
      job_id: string;
      events_count: number;
      duration_ms: number;
    }
  | { type: "job_failed"; batch_id: string; job_id: string; error: string }
  | {
      type: "batch_finished";
      batch_id: string;
      success_count: number;
      fail_count: number;
      duration_ms: number;
    };

const EVENT_NAME = "pipeline:event";

export async function listenPipeline(
  cb: (e: PipelineEvent) => void
): Promise<UnlistenFn> {
  return listen<PipelineEvent>(EVENT_NAME, (e) => cb(e.payload));
}

export async function startBatchPipeline(paths: string[]): Promise<StartBatchOutcome> {
  return invoke<StartBatchOutcome>("start_batch_pipeline", { paths });
}

export async function resumePendingJobs(): Promise<StartBatchOutcome> {
  return invoke<StartBatchOutcome>("resume_pending_jobs");
}

export async function listJobs(): Promise<VideoJob[]> {
  return invoke<VideoJob[]>("list_jobs");
}

export async function listPendingJobs(): Promise<VideoJob[]> {
  return invoke<VideoJob[]>("list_pending_jobs");
}
