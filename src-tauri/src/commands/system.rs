use serde::Serialize;

use crate::ai::model_path::{ModelStatus, SEGFORMER_FILENAME, YOLOV8_FILENAME};
use crate::ai::plate::{DETECTOR_FILENAME as PLATE_DET_FILE, RECOGNIZER_FILENAME as PLATE_REC_FILE};

#[derive(Debug, Serialize)]
pub struct SystemStatus {
    pub models: Vec<ModelStatus>,
    pub ort_dylib_path: Option<String>,
    pub app_version: &'static str,
}

#[tauri::command]
pub fn check_system_status() -> SystemStatus {
    let models = vec![
        ModelStatus::check("YOLOv8-seg (车辆+掩膜)", YOLOV8_FILENAME),
        ModelStatus::check("HyperLPR3 检测器", PLATE_DET_FILE),
        ModelStatus::check("HyperLPR3 识别器", PLATE_REC_FILE),
        ModelStatus::check("SegFormer-B0 (人行道)", SEGFORMER_FILENAME),
    ];
    let ort_dylib_path = std::env::var("ORT_DYLIB_PATH").ok();

    SystemStatus {
        models,
        ort_dylib_path,
        app_version: env!("CARGO_PKG_VERSION"),
    }
}

/// 在 Finder/资源管理器 中打开指定路径 (P3: 证据文件夹)
#[tauri::command]
pub fn open_in_file_manager(path: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(format!("路径不存在: {path}"));
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .status()
            .map_err(|e| format!("打开 Finder 失败: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .status()
            .map_err(|e| format!("xdg-open 失败: {e}"))?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&path)
            .status()
            .map_err(|e| format!("explorer 失败: {e}"))?;
    }
    Ok(())
}
