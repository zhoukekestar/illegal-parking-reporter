// 证据导出命令 (P5)

use std::path::PathBuf;

use crate::evidence::exporter::{export_events as inner_export, ExportSummary};

#[tauri::command]
pub async fn export_accepted_events(
    event_ids: Vec<String>,
    target_dir: String,
) -> Result<ExportSummary, String> {
    tokio::task::spawn_blocking(move || -> anyhow::Result<ExportSummary> {
        // 拉取所有事件, 过滤选中的
        let events = {
            let lock = crate::db::conn()?;
            let conn = lock.lock().map_err(|e| anyhow::anyhow!("DB mutex 中毒: {e}"))?;
            crate::db::events::list_all(&conn)?
        };
        let id_set: std::collections::HashSet<String> = event_ids.into_iter().collect();
        let selected: Vec<crate::models::event::ParkingEvent> = events
            .into_iter()
            .filter(|e| id_set.contains(&e.id))
            .collect();
        if selected.is_empty() {
            anyhow::bail!("没有选中事件");
        }

        let target = PathBuf::from(&target_dir);
        let summary = inner_export(&selected, &target)?;

        // 标记 DB
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let lock = crate::db::conn()?;
        let conn = lock.lock().map_err(|e| anyhow::anyhow!("DB mutex 中毒: {e}"))?;
        // 收集成功导出的 id
        let ids: Vec<String> = selected
            .iter()
            .filter(|e| {
                !summary
                    .skipped
                    .iter()
                    .any(|s| s.event_id == e.id)
            })
            .map(|e| e.id.clone())
            .collect();
        crate::db::events::mark_exported(&conn, &ids, &summary.bundle_path, &now)?;

        Ok(summary)
    })
    .await
    .map_err(|e| format!("blocking task panic: {e}"))?
    .map_err(|e| format!("导出失败: {e:#}"))
}
