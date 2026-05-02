// P7 诊断命令

use std::path::PathBuf;

use crate::diagnostic::{export_diagnostic_bundle, DiagnosticReport};

#[tauri::command]
pub async fn export_diagnostic(target_dir: String) -> Result<DiagnosticReport, String> {
    tokio::task::spawn_blocking(move || -> anyhow::Result<DiagnosticReport> {
        export_diagnostic_bundle(&PathBuf::from(&target_dir))
    })
    .await
    .map_err(|e| format!("blocking task panic: {e}"))?
    .map_err(|e| format!("导出诊断包失败: {e:#}"))
}
