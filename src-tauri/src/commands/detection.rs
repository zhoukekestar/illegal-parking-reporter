use std::path::PathBuf;

use crate::ai::vehicle::detector;
use crate::models::detection::DetectionResult;

/// P0 demo 命令: 给定本地图片路径, 跑 YOLOv8 推理返回检测结果
///
/// 前端先用 plugin-dialog 让用户选 JPG, 拿到路径后调用本命令
#[tauri::command]
pub async fn detect_demo(image_path: String) -> Result<DetectionResult, String> {
    let path = PathBuf::from(&image_path);
    if !path.exists() {
        return Err(format!("图片不存在: {}", path.display()));
    }

    // 推理是 CPU/GPU 密集任务, 移到 blocking 池避免阻塞 Tauri 异步运行时
    tokio::task::spawn_blocking(move || -> anyhow::Result<DetectionResult> {
        tracing::info!(?path, "开始解码图片");
        let img = image::open(&path)?.to_rgb8();
        tracing::info!(w = img.width(), h = img.height(), "图片解码完成, 进入推理");

        let det_lock = detector()?;
        let mut det = det_lock.lock().map_err(|e| anyhow::anyhow!("Detector mutex 中毒: {e}"))?;
        let result = det.detect(&img)?;
        tracing::info!(
            count = result.detections.len(),
            ms = result.inference_ms,
            "推理完成"
        );
        Ok(result)
    })
    .await
    .map_err(|e| format!("blocking 任务 panic: {e}"))?
    .map_err(|e| format!("检测失败: {e:#}"))
}
