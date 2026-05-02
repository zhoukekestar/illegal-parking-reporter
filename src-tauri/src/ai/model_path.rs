use std::path::PathBuf;

/// 解析 ONNX 模型目录
///
/// 优先级:
/// 1. 环境变量 `IPR_MODELS_DIR` (开发期手动指定)
/// 2. dev 模式: `<crate_dir>/models/`
/// 3. release 模式: `<exe_dir>/../Resources/models/` (macOS bundle)
///    或 `<exe_dir>/models/` (其他平台)
pub fn models_dir() -> PathBuf {
    if let Ok(p) = std::env::var("IPR_MODELS_DIR") {
        return PathBuf::from(p);
    }

    // dev 模式: CARGO_MANIFEST_DIR 在编译时注入, 指向 src-tauri/
    #[cfg(debug_assertions)]
    {
        return PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models");
    }

    // release 模式: 推断可执行文件相对路径
    #[cfg(not(debug_assertions))]
    {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                #[cfg(target_os = "macos")]
                {
                    // macOS .app bundle: Contents/MacOS/<exe> → Contents/Resources/models
                    let candidate = dir.join("../Resources/models");
                    if candidate.exists() {
                        return candidate;
                    }
                }
                return dir.join("models");
            }
        }
        PathBuf::from("models")
    }
}

/// 单个模型的状态报告
#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelStatus {
    pub name: String,
    pub ready: bool,
    pub path: String,
    pub size_bytes: Option<u64>,
    pub error: Option<String>,
}

impl ModelStatus {
    pub fn check(name: &str, file: &str) -> Self {
        let path = models_dir().join(file);
        let path_str = path.display().to_string();
        match std::fs::metadata(&path) {
            Ok(m) if m.is_file() && m.len() > 0 => Self {
                name: name.to_string(),
                ready: true,
                path: path_str,
                size_bytes: Some(m.len()),
                error: None,
            },
            Ok(_) => Self {
                name: name.to_string(),
                ready: false,
                path: path_str,
                size_bytes: None,
                error: Some("文件存在但不是有效模型文件".to_string()),
            },
            Err(e) => Self {
                name: name.to_string(),
                ready: false,
                path: path_str,
                size_bytes: None,
                error: Some(format!("找不到模型文件: {e}")),
            },
        }
    }
}

/// 路径常量, 集中维护
///
/// P3 起 YOLOv8 升级到 -seg 版本, 同时输出车辆掩膜
pub const YOLOV8_FILENAME: &str = "yolov8n-seg.onnx";

/// SegFormer-B0 (P3): ADE20K 语义分割, 用于人行道掩膜
pub const SEGFORMER_FILENAME: &str = "segformer/model.onnx";
