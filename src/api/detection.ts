import { invoke } from "@tauri-apps/api/core";

/** YOLOv8 单个检测框, 与 Rust Detection 对应 */
export interface Detection {
  class_id: number;
  class_name: string;
  score: number;
  /** [x1, y1, x2, y2] 原始图片像素坐标 */
  bbox: [number, number, number, number];
}

/** 单次检测调用返回值, 与 Rust DetectionResult 对应 */
export interface DetectionResult {
  inference_ms: number;
  image_width: number;
  image_height: number;
  detections: Detection[];
}

/** 调用后端 detect_demo, 给定图片绝对路径返回检测结果 */
export async function detectDemo(imagePath: string): Promise<DetectionResult> {
  return invoke<DetectionResult>("detect_demo", { imagePath });
}
