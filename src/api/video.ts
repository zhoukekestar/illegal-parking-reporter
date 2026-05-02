import { invoke } from "@tauri-apps/api/core";

/** 与 Rust VideoMetadata 对应 */
export interface VideoMetadata {
  creation_time: string | null;
  duration_seconds: number;
  frame_rate: number;
  rotation_degrees: number;
  width: number;
  height: number;
  display_width: number;
  display_height: number;
  codec_name: string;
  file_size_bytes: number;
}

/** 帧观测中的车牌 */
export interface PlateReading {
  text: string;
  confidence: number;
  plate_bbox: [number, number, number, number];
}

/** 帧观测中的车辆 */
export interface VehicleObservation {
  class_id: number;
  class_name: string;
  vehicle_score: number;
  bbox: [number, number, number, number];
  plate: PlateReading | null;
}

/** 抽样帧观测 (intermediate) */
export interface FrameObservation {
  frame_index: number;
  timestamp_ms: number;
  width: number;
  height: number;
  vehicles: VehicleObservation[];
}

export async function markEventUploaded(eventId: string): Promise<void> {
  await invoke("mark_event_uploaded", { eventId });
}

export interface CleanupSummary {
  deleted_count: number;
  deleted_evidence_dirs: number;
}

export async function cleanupInvalidEvents(): Promise<CleanupSummary> {
  return invoke<CleanupSummary>("cleanup_invalid_events");
}

/** 单个违停事件 (聚合后) */
export interface ParkingEvent {
  id: string;
  source_video: string;
  representative_frame_index: number;
  timestamp_ms: number;
  event_time: string | null;
  plate_number: string;
  plate_confidence: number;
  plate_manual_corrected: string | null;
  vehicle_class: string;
  vehicle_bbox: [number, number, number, number];
  first_seen_ms: number;
  last_seen_ms: number;
  frame_hits: number;
  review_status: "pending" | "accepted" | "rejected" | "deferred";
  iou_score: number | null;
  snapshot_path: string | null;
  clip_path: string | null;
  exported_at: string | null;
  export_path: string | null;
  uploaded_at: string | null;
}

export interface ProcessOutcome {
  metadata: VideoMetadata;
  observations: FrameObservation[];
  events: ParkingEvent[];
}

export async function readVideoMetadata(path: string): Promise<VideoMetadata> {
  return invoke<VideoMetadata>("read_video_metadata", { path });
}

export async function processVideo(path: string): Promise<ProcessOutcome> {
  return invoke<ProcessOutcome>("process_video", { path });
}

export async function listEvents(): Promise<ParkingEvent[]> {
  return invoke<ParkingEvent[]>("list_events");
}

export async function detectPlateDemo(imagePath: string): Promise<PlateReading[]> {
  return invoke<PlateReading[]>("detect_plate_demo", { imagePath });
}

export async function updateEventStatus(
  eventId: string,
  status: ParkingEvent["review_status"]
): Promise<void> {
  await invoke("update_event_status", { eventId, status });
}

export async function updateEventPlate(
  eventId: string,
  corrected: string | null
): Promise<void> {
  await invoke("update_event_plate", { eventId, corrected });
}

export interface SkipReason {
  event_id: string;
  reason: string;
}

export interface ExportSummary {
  bundle_path: string;
  exported_count: number;
  skipped: SkipReason[];
  index_csv: string;
  guide_html: string;
}

export async function exportAcceptedEvents(
  eventIds: string[],
  targetDir: string
): Promise<ExportSummary> {
  return invoke<ExportSummary>("export_accepted_events", { eventIds, targetDir });
}
