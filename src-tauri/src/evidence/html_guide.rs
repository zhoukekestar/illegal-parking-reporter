// 上传指引 HTML 生成 (P5)
//
// 决策: 用 HTML 替代 PDF, 原因:
//   - Rust 端纯 PDF 库 (printpdf 等) 中文需要嵌入 ttf, 系统字体多为 ttc 不直接支持
//   - 单文件 HTML 自包含: base64 内嵌截图, 可双击直接看, 浏览器 ⌘P 可保存为 PDF
//   - 验收标准 "PDF 在 Preview 正常显示" 用户可一键打印为 PDF 满足
//
// 每个事件一页 (CSS 分页), 含: 截图 + 车牌(大字号) + 时间 + 违法类型 + 填表对照说明

use std::fs::File;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use base64::Engine;

use crate::models::event::ParkingEvent;

pub fn write_html_guide(events: &[ParkingEvent], out: &Path) -> Result<()> {
    let mut f = File::create(out).with_context(|| format!("创建 HTML 失败: {}", out.display()))?;

    f.write_all(HEADER.as_bytes())?;
    f.write_all(STYLE.as_bytes())?;

    write!(f, "<title>路况记录助手 - 上传指引</title></head><body>")?;

    write!(
        f,
        r#"<header class="cover">
            <h1>路况记录助手</h1>
            <h2>违停举报上传指引</h2>
            <p class="subtitle">共 {} 个事件 / 生成时间 {}</p>
            <div class="callout">
                <strong>使用说明</strong>
                <ol>
                    <li>把每个事件子文件夹的 <code>截图.jpg</code> + <code>视频.mp4</code> 上传到「警察叔叔」App / 支付宝城市服务 / 交通拍客</li>
                    <li>填表时对照本指引下方每个事件页的「填表对照」</li>
                    <li>举报时间 ≤ 拍摄后 72 小时</li>
                    <li>软件仅辅助识别, 用户对最终提交内容负责</li>
                    <li>本 HTML 可在浏览器中按 ⌘P (macOS) / Ctrl+P (Win) 打印为 PDF 存档</li>
                </ol>
            </div>
        </header>"#,
        events.len(),
        chrono::Local::now().format("%Y-%m-%d %H:%M")
    )?;

    for (i, e) in events.iter().enumerate() {
        let plate = e.plate_manual_corrected.clone().unwrap_or_else(|| e.plate_number.clone());
        let plate_html = html_escape(&plate);
        let time_str = e.event_time.clone().unwrap_or_else(|| {
            format!("视频偏移 {:.1}s", e.timestamp_ms as f64 / 1000.0)
        });
        let time_html = html_escape(&time_str);
        let source_html = html_escape(&e.source_video);
        let vehicle_class_html = html_escape(&e.vehicle_class);
        let conf = (e.plate_confidence * 100.0).round() as i32;
        let iou = e
            .iou_score
            .map(|s| format!("{:.0}%", s * 100.0))
            .unwrap_or_else(|| "—".to_string());
        let folder_html = html_escape(e.export_path.as_deref().unwrap_or("(未导出)"));

        // 嵌入截图 (优先 export_path 内的截图, 否则 snapshot_path)
        let snapshot_b64 = embed_snapshot(e);

        writeln!(
            f,
            r#"<section class="event-page">
                <header>
                    <span class="badge">事件 {}</span>
                    <span class="plate">{}</span>
                </header>
                <div class="grid">
                    <div class="img-wrap">
                        {}
                    </div>
                    <table class="info">
                        <tr><th>车牌号</th><td>{}</td></tr>
                        <tr><th>识别置信度</th><td>{}%</td></tr>
                        <tr><th>车型</th><td>{}</td></tr>
                        <tr><th>拍摄时间</th><td>{}</td></tr>
                        <tr><th>占用人行道率</th><td>{}</td></tr>
                        <tr><th>来源视频</th><td><code>{}</code></td></tr>
                        <tr><th>证据子目录</th><td><code>{}</code></td></tr>
                    </table>
                </div>
                <h3>填表对照 (警察叔叔 / 支付宝城市服务)</h3>
                <ol class="howto">
                    <li>「违法行为」选择: <strong>占用人行道</strong></li>
                    <li>「车牌号」填: <strong>{}</strong></li>
                    <li>「时间」: {}</li>
                    <li>「视频/图片」: 上传该事件子目录的 <code>视频.mp4</code> + <code>截图.jpg</code></li>
                    <li>「拍摄地址」: 用户根据现场记忆填写</li>
                </ol>
            </section>"#,
            i + 1,
            plate_html,
            snapshot_b64,
            plate_html,
            conf,
            vehicle_class_html,
            time_html,
            iou,
            source_html,
            folder_html,
            plate_html,
            time_html,
        )?;
    }

    f.write_all(b"</body></html>")?;
    f.flush()?;
    Ok(())
}

fn embed_snapshot(e: &ParkingEvent) -> String {
    let path_in_export = e.export_path.as_ref().map(|p| Path::new(p).join("截图.jpg"));
    let path_orig = e.snapshot_path.as_ref().map(|p| Path::new(p).to_path_buf());
    let snapshot = path_in_export
        .as_ref()
        .filter(|p| p.exists())
        .or(path_orig.as_ref())
        .map(|p| p.as_path());
    let p = match snapshot {
        Some(p) => p,
        None => return r#"<div class="img-empty">无截图</div>"#.to_string(),
    };
    match std::fs::read(p) {
        Ok(bytes) => {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            format!(r#"<img src="data:image/jpeg;base64,{b64}" alt="截图" />"#)
        }
        Err(_) => r#"<div class="img-empty">截图读取失败</div>"#.to_string(),
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

const HEADER: &str = r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
"#;

const STYLE: &str = r#"<style>
*, *::before, *::after { box-sizing: border-box; }
body {
  font-family: -apple-system, "PingFang SC", "Microsoft YaHei", "Segoe UI", sans-serif;
  margin: 0;
  background: #f5f5f7;
  color: #222;
}
header.cover, section.event-page {
  background: #fff;
  margin: 24px auto;
  max-width: 800px;
  padding: 32px 40px;
  border-radius: 8px;
  box-shadow: 0 2px 12px rgba(0,0,0,0.06);
}
header.cover h1 { margin: 0; font-size: 32px; color: #2080ff; }
header.cover h2 { margin: 8px 0 4px; font-size: 22px; }
.subtitle { color: #888; font-size: 14px; }
.callout {
  background: #fff8e1;
  border-left: 4px solid #ffb300;
  padding: 16px 20px;
  margin-top: 20px;
  border-radius: 4px;
}
.callout ol { padding-left: 20px; margin: 6px 0 0; }
.callout li { margin: 4px 0; }
.callout code { background: #fff3cd; padding: 1px 6px; border-radius: 3px; }

section.event-page {
  page-break-before: always;
}
section.event-page > header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  border-bottom: 2px solid #2080ff;
  padding-bottom: 12px;
}
.badge {
  background: #2080ff;
  color: #fff;
  padding: 6px 12px;
  border-radius: 4px;
  font-size: 14px;
}
.plate {
  font-size: 36px;
  font-weight: 700;
  letter-spacing: 4px;
  color: #2080ff;
}
.grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 24px;
  margin: 24px 0;
}
.img-wrap img {
  width: 100%;
  border-radius: 6px;
  border: 1px solid #ddd;
}
.img-empty {
  height: 200px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #f0f0f0;
  color: #888;
  border-radius: 6px;
}
.info {
  width: 100%;
  border-collapse: collapse;
  font-size: 14px;
}
.info th {
  text-align: left;
  width: 110px;
  background: #f9fafb;
  padding: 10px 12px;
  border-bottom: 1px solid #e5e7eb;
  color: #555;
  font-weight: 500;
}
.info td {
  padding: 10px 12px;
  border-bottom: 1px solid #e5e7eb;
}
.info code {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
  word-break: break-all;
}
h3 {
  margin: 24px 0 8px;
  color: #444;
  font-size: 16px;
}
ol.howto { padding-left: 20px; }
ol.howto li { margin: 6px 0; line-height: 1.7; }
ol.howto code { background: #f0f0f0; padding: 1px 6px; border-radius: 3px; font-size: 12px; }
@media print {
  body { background: #fff; }
  header.cover, section.event-page {
    box-shadow: none;
    margin: 0;
    border-radius: 0;
    page-break-inside: avoid;
  }
}
</style>
"#;
