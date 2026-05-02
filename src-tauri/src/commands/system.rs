use serde::Serialize;

use crate::ai::model_path::{ModelStatus, YOLOV8_FILENAME};
use crate::ai::plate::{DETECTOR_FILENAME as PLATE_DET_FILE, RECOGNIZER_FILENAME as PLATE_REC_FILE};

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
#[tauri::command]
pub fn check_system_status() -> SystemStatus {
    let models = vec![
        ModelStatus::check("YOLOv8 (车辆)", YOLOV8_FILENAME),
        ModelStatus::check("HyperLPR3 检测器", PLATE_DET_FILE),
        ModelStatus::check("HyperLPR3 识别器", PLATE_REC_FILE),
    ];
    let ort_dylib_path = std::env::var("ORT_DYLIB_PATH").ok();

    SystemStatus {
        models,
        ort_dylib_path,
        app_version: env!("CARGO_PKG_VERSION"),
    }
}
