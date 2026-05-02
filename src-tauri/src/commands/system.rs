use serde::Serialize;

use crate::ai::model_path::{ModelStatus, YOLOV8_FILENAME};

#[derive(Debug, Serialize)]
pub struct SystemStatus {
    /// 全部模型检查结果
    pub models: Vec<ModelStatus>,
    /// onnxruntime 动态库路径 (env ORT_DYLIB_PATH)
    pub ort_dylib_path: Option<String>,
    /// 软件版本
    pub app_version: &'static str,
}

/// 检查所有模型与运行时是否就绪
///
/// P0 阶段只检查 yolov8n.onnx,
/// P1 起会追加 hyperlpr3, P3 起追加 segformer
#[tauri::command]
pub fn check_system_status() -> SystemStatus {
    let models = vec![ModelStatus::check("YOLOv8", YOLOV8_FILENAME)];
    let ort_dylib_path = std::env::var("ORT_DYLIB_PATH").ok();

    SystemStatus {
        models,
        ort_dylib_path,
        app_version: env!("CARGO_PKG_VERSION"),
    }
}
