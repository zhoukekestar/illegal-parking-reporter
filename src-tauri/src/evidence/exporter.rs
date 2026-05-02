// 证据包批量导出 (P5)
//
// 输出顶层目录结构 (DEVELOPMENT_PLAN.md §三):
//   违停举报包_YYYY-MM-DD_HH-MM/
//     ├── 索引.csv               UTF-8 BOM
//     ├── 上传指引.html          单文件 HTML, base64 内嵌截图
//     ├── 浙A12345_{video}_HH-MM-SS/
//     │   ├── 截图.jpg
//     │   ├── 视频.mp4
//     │   └── 信息.txt
//     └── ...

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Local;
use serde::Serialize;

use crate::evidence::csv_index::write_index_csv;
use crate::evidence::html_guide::write_html_guide;
use crate::models::event::ParkingEvent;

#[derive(Debug, Clone, Serialize)]
pub struct ExportSummary {
    /// 顶层导出文件夹绝对路径
    pub bundle_path: String,
    /// 实际导出的事件数 (跳过缺失证据/已导出的)
    pub exported_count: usize,
    /// 跳过事件的 (id, 原因)
    pub skipped: Vec<SkipReason>,
    /// 索引 CSV 路径
    pub index_csv: String,
    /// 上传指引 HTML 路径
    pub guide_html: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkipReason {
    pub event_id: String,
    pub reason: String,
}

/// 导出选中的事件到目标目录
///
/// 调用方应已经过滤为 review_status='accepted' 的事件
/// 已导出的事件 (exported_at != null) 不会被重导, 而是跳过
pub fn export_events(events: &[ParkingEvent], target_dir: &Path) -> Result<ExportSummary> {
    if !target_dir.exists() {
        anyhow::bail!("目标目录不存在: {}", target_dir.display());
    }
    let bundle_dir_name = format!(
        "违停举报包_{}",
        Local::now().format("%Y-%m-%d_%H-%M")
    );
    let bundle = target_dir.join(&bundle_dir_name);
    std::fs::create_dir_all(&bundle).context("创建顶层证据包目录失败")?;

    let mut copied: Vec<ParkingEvent> = Vec::new();
    let mut skipped: Vec<SkipReason> = Vec::new();

    for evt in events {
        // 必须有截图 + 视频 才能导出
        let snap = match &evt.snapshot_path {
            Some(p) => PathBuf::from(p),
            None => {
                skipped.push(SkipReason {
                    event_id: evt.id.clone(),
                    reason: "缺少截图".to_string(),
                });
                continue;
            }
        };
        if !snap.exists() {
            skipped.push(SkipReason {
                event_id: evt.id.clone(),
                reason: format!("截图不存在: {}", snap.display()),
            });
            continue;
        }
        let src_folder = match snap.parent() {
            Some(p) => p.to_path_buf(),
            None => {
                skipped.push(SkipReason {
                    event_id: evt.id.clone(),
                    reason: "无法定位证据子目录".to_string(),
                });
                continue;
            }
        };

        let folder_name = src_folder
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&evt.id);
        let dst_folder = bundle.join(folder_name);

        if let Err(e) = copy_dir_recursive(&src_folder, &dst_folder) {
            skipped.push(SkipReason {
                event_id: evt.id.clone(),
                reason: format!("复制失败: {e:#}"),
            });
            continue;
        }

        let mut copied_evt = evt.clone();
        copied_evt.export_path = Some(dst_folder.to_string_lossy().to_string());
        copied.push(copied_evt);
    }

    // 索引 CSV
    let index_csv = bundle.join("索引.csv");
    write_index_csv(&copied, &index_csv).context("生成 索引.csv 失败")?;

    // 上传指引 HTML
    let guide_html = bundle.join("上传指引.html");
    write_html_guide(&copied, &guide_html).context("生成 上传指引.html 失败")?;

    Ok(ExportSummary {
        bundle_path: bundle.to_string_lossy().to_string(),
        exported_count: copied.len(),
        skipped,
        index_csv: index_csv.to_string_lossy().to_string(),
        guide_html: guide_html.to_string_lossy().to_string(),
    })
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst).with_context(|| format!("创建目标目录失败: {}", dst.display()))?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).with_context(|| {
                format!("复制 {} -> {}", from.display(), to.display())
            })?;
        }
    }
    Ok(())
}
