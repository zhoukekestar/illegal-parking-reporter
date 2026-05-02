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
