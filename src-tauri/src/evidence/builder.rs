// 单事件证据文件夹构建

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use crate::evidence::evidence_root;
use crate::models::event::ParkingEvent;
use crate::video::clip::{clip_with_watermark, snapshot_with_watermark, ClipOptions};

/// 单个事件的证据包构建结果
#[derive(Debug, Clone)]
pub struct EvidencePaths {
    pub folder: PathBuf,
    pub snapshot: PathBuf,
    pub clip: PathBuf,
    pub info: PathBuf,
}

/// 给定一个 ParkingEvent, 在 evidence_root 下生成完整证据文件夹
///
/// 同名文件夹存在时直接覆盖 (用于重处理)
pub fn build_for_event(event: &ParkingEvent, source_video: &Path) -> Result<EvidencePaths> {
    let folder = event_folder_path(event)?;
    tracing::info!(
        event_id = %event.id,
        plate = %event.plate_number,
        timestamp_ms = event.timestamp_ms,
        source = %source_video.display(),
        folder = %folder.display(),
        "evidence build_for_event 开始"
    );
    if !source_video.exists() {
        anyhow::bail!("源视频不存在: {}", source_video.display());
    }
    std::fs::create_dir_all(&folder).context("创建事件证据目录失败")?;

    let snapshot = folder.join("截图.jpg");
    let clip = folder.join("视频.mp4");
    let info = folder.join("信息.txt");

    // 时间戳文本 (秒级)
    let timestamp_text = render_timestamp(event);

    // 1. 截图: 取代表帧时刻
    let snapshot_secs = (event.timestamp_ms as f64) / 1000.0;
    snapshot_with_watermark(
        source_video,
        &snapshot,
        snapshot_secs,
        Some(&timestamp_text),
    )
    .context("生成截图失败")?;

    // 2. 视频片段: 代表帧 ± 3 秒, 共 6 秒
    let start = (snapshot_secs - 3.0).max(0.0);
    let duration = 6.0;
    clip_with_watermark(
        source_video,
        &clip,
        &ClipOptions {
            start_secs: start,
            duration_secs: duration,
            timestamp_text: Some(timestamp_text.clone()),
        },
    )
    .context("生成视频片段失败")?;

    // 3. 信息.txt
    let info_content = render_info_txt(event, &timestamp_text);
    std::fs::write(&info, info_content.as_bytes()).context("写入信息.txt 失败")?;

    tracing::info!(
        event_id = %event.id,
        snapshot = %snapshot.display(),
        clip = %clip.display(),
        "evidence build_for_event 完成"
    );
    Ok(EvidencePaths {
        folder,
        snapshot,
        clip,
        info,
    })
}

/// 事件文件夹路径: {root}/{车牌}_{源视频名}_{HHMMSS}/
fn event_folder_path(event: &ParkingEvent) -> Result<PathBuf> {
    let root = evidence_root()?;
    let plate = sanitize_filename(&event.plate_number);
    let video_stem = Path::new(&event.source_video)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("video");
    let video_stem = sanitize_filename(video_stem);
    let hms = format_hms(event.timestamp_ms);
    Ok(root.join(format!("{plate}_{video_stem}_{hms}")))
}

/// HH-MM-SS 格式 (来自 timestamp_ms 距视频起点的偏移)
fn format_hms(ms: i64) -> String {
    let total = ms.max(0) / 1000;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    format!("{:02}-{:02}-{:02}", h, m, s)
}

/// 文件系统不允许的字符替换为 _
fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

fn render_timestamp(event: &ParkingEvent) -> String {
    if let Some(et) = &event.event_time {
        if let Ok(dt) = DateTime::parse_from_rfc3339(et) {
            return dt
                .with_timezone(&Utc)
                .format("%Y-%m-%d %H:%M:%S UTC")
                .to_string();
        }
        return et.clone();
    }
    // 没有 event_time 时, 用 hms 偏移
    format!("视频内偏移 {}", format_hms(event.timestamp_ms))
}

fn render_info_txt(event: &ParkingEvent, timestamp_text: &str) -> String {
    let mut out = String::new();
    out.push_str("路况记录助手 - 违停证据 信息卡\n");
    out.push_str("==========================================\n");
    out.push_str(&format!("车牌: {}\n", event.plate_number));
    if let Some(corr) = &event.plate_manual_corrected {
        out.push_str(&format!("人工修正车牌: {}\n", corr));
    }
    out.push_str(&format!(
        "车牌识别置信度: {:.1}%\n",
        event.plate_confidence * 100.0
    ));
    out.push_str(&format!("车型: {}\n", event.vehicle_class));
    out.push_str(&format!("拍摄时间: {}\n", timestamp_text));
    out.push_str(&format!(
        "视频内出现窗口: {:.1}s - {:.1}s (聚合 {} 帧)\n",
        event.first_seen_ms as f64 / 1000.0,
        event.last_seen_ms as f64 / 1000.0,
        event.frame_hits
    ));
    out.push_str(&format!(
        "占用人行道率 (intersection / vehicle): {}\n",
        event
            .iou_score
            .map(|s| format!("{:.1}%", s * 100.0))
            .unwrap_or_else(|| "—".to_string())
    ));
    out.push_str(&format!("源视频: {}\n", event.source_video));
    out.push_str(&format!("事件 ID: {}\n", event.id));
    out.push_str("\n");
    out.push_str("--------------------------------------------\n");
    out.push_str("使用建议:\n");
    out.push_str("1. 把本目录(截图.jpg + 视频.mp4)上传到「警察叔叔」App / 支付宝城市服务 / 交通拍客\n");
    out.push_str("2. 举报时间 ≤ 拍摄后 72 小时\n");
    out.push_str("3. 软件仅辅助识别, 用户对最终提交内容负责\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hms_format() {
        assert_eq!(format_hms(0), "00-00-00");
        assert_eq!(format_hms(123_456), "00-02-03");
        assert_eq!(format_hms(3_600_000), "01-00-00");
        assert_eq!(format_hms(3_661_500), "01-01-01");
    }

    #[test]
    fn sanitize_strips_fs_specials() {
        assert_eq!(sanitize_filename("浙A12345"), "浙A12345");
        assert_eq!(sanitize_filename("浙A:12*345"), "浙A_12_345");
        assert_eq!(sanitize_filename("<待确认>"), "_待确认_");
    }
}
