// 证据视频剪辑 + 时间戳水印 (P3)
//
// 通过 ffmpeg CLI 子进程实现, 因为:
//   - ffmpeg-next 的 mux/encoder API 写一个一键剪辑 + drawtext 滤镜要 200 行;
//     CLI 一行 + libavfilter drawtext 完整可用
//   - 用户机器装的同一份 ffmpeg, 不引入额外依赖
//   - P3 阶段优先实现"能跑", P7 性能优化时再考虑改回 ffmpeg-next
//
// drawtext 字体:
//   macOS: /System/Library/Fonts/Supplemental/Arial.ttf (数字+横线+冒号够用, 不用中文)
//   字体不存在时降级到无水印 (仅警告)

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

const DEFAULT_FONT_MACOS: &str = "/System/Library/Fonts/Supplemental/Arial.ttf";

/// 剪辑选项
pub struct ClipOptions {
    /// 起始秒数 (距视频开头)
    pub start_secs: f64,
    /// 持续秒数 (典型 6.0 = 前 3 + 后 3)
    pub duration_secs: f64,
    /// 烧录的时间戳文本 (例 "2026-05-02 14:23:05"), None 表示不加水印
    pub timestamp_text: Option<String>,
}

/// 把 [start, start+duration] 段剪出来, 重编码 H.264 + 可选烧录时间戳水印
pub fn clip_with_watermark(input: &Path, output: &Path, opts: &ClipOptions) -> Result<()> {
    if !input.exists() {
        anyhow::bail!("输入视频不存在: {}", input.display());
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).context("创建输出目录失败")?;
    }

    let mut args: Vec<String> = vec![
        // -ss 放在 -i 前是 fast seek (快, 但精度差关键帧); 放在 -i 后是 slow seek (准)
        // 我们要的是精确切, 用 slow seek
        "-y".to_string(),
        "-i".to_string(),
        input.to_string_lossy().to_string(),
        "-ss".to_string(),
        format!("{:.3}", opts.start_secs.max(0.0)),
        "-t".to_string(),
        format!("{:.3}", opts.duration_secs.max(0.1)),
    ];

    // drawtext filter
    if let Some(ts) = &opts.timestamp_text {
        if std::path::Path::new(DEFAULT_FONT_MACOS).exists() {
            let escaped = escape_drawtext(ts);
            // 右下角 + 半透明黑底
            let filter = format!(
                "drawtext=text='{escaped}':fontfile={font}:x=w-tw-20:y=h-th-20:fontsize=42:fontcolor=white:box=1:boxcolor=black@0.5:boxborderw=10",
                font = DEFAULT_FONT_MACOS
            );
            args.push("-vf".to_string());
            args.push(filter);
        } else {
            tracing::warn!(font = DEFAULT_FONT_MACOS, "字体文件不存在, 视频不加水印");
        }
    }

    args.push("-c:v".to_string());
    args.push("libx264".to_string());
    args.push("-preset".to_string());
    args.push("ultrafast".to_string());
    args.push("-crf".to_string());
    args.push("23".to_string());
    args.push("-pix_fmt".to_string());
    args.push("yuv420p".to_string());
    // 音频流原样复制 (举报视频不一定要音频, 但保留无害)
    args.push("-c:a".to_string());
    args.push("copy".to_string());
    args.push(output.to_string_lossy().to_string());

    let output_res = Command::new("ffmpeg")
        .args(&args)
        .output()
        .context("启动 ffmpeg 失败 (确认 ffmpeg 在 PATH 内)")?;

    if !output_res.status.success() {
        let stderr = String::from_utf8_lossy(&output_res.stderr);
        // 音频复制失败时降级: 重试不加 -c:a copy
        if stderr.contains("Could not find tag") || stderr.contains("does not contain any stream") {
            return clip_without_audio(input, output, opts);
        }
        anyhow::bail!(
            "ffmpeg 退出码 {:?}, stderr 末尾:\n{}",
            output_res.status.code(),
            tail(&stderr, 1500)
        );
    }
    Ok(())
}

/// 高分辨率截图 + 烧录时间戳 (用于证据包的截图.jpg)
pub fn snapshot_with_watermark(
    input: &Path,
    output: &Path,
    snapshot_secs: f64,
    timestamp_text: Option<&str>,
) -> Result<()> {
    if !input.exists() {
        anyhow::bail!("输入视频不存在: {}", input.display());
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).context("创建输出目录失败")?;
    }
    let mut args: Vec<String> = vec![
        "-y".to_string(),
        "-i".to_string(),
        input.to_string_lossy().to_string(),
        "-ss".to_string(),
        format!("{:.3}", snapshot_secs.max(0.0)),
        "-frames:v".to_string(),
        "1".to_string(),
    ];
    if let Some(ts) = timestamp_text {
        if std::path::Path::new(DEFAULT_FONT_MACOS).exists() {
            let escaped = escape_drawtext(ts);
            let filter = format!(
                "drawtext=text='{escaped}':fontfile={font}:x=w-tw-20:y=h-th-20:fontsize=42:fontcolor=white:box=1:boxcolor=black@0.5:boxborderw=10",
                font = DEFAULT_FONT_MACOS
            );
            args.push("-vf".to_string());
            args.push(filter);
        }
    }
    args.push("-q:v".to_string());
    args.push("2".to_string()); // 高质量 jpg
    args.push(output.to_string_lossy().to_string());

    let output_res = Command::new("ffmpeg")
        .args(&args)
        .output()
        .context("启动 ffmpeg 失败")?;
    if !output_res.status.success() {
        let stderr = String::from_utf8_lossy(&output_res.stderr);
        anyhow::bail!(
            "截图 ffmpeg 退出码 {:?}, stderr:\n{}",
            output_res.status.code(),
            tail(&stderr, 1500)
        );
    }
    Ok(())
}

fn clip_without_audio(input: &Path, output: &Path, opts: &ClipOptions) -> Result<()> {
    let mut args: Vec<String> = vec![
        "-y".to_string(),
        "-i".to_string(),
        input.to_string_lossy().to_string(),
        "-ss".to_string(),
        format!("{:.3}", opts.start_secs.max(0.0)),
        "-t".to_string(),
        format!("{:.3}", opts.duration_secs.max(0.1)),
        "-an".to_string(),
    ];
    if let Some(ts) = &opts.timestamp_text {
        if std::path::Path::new(DEFAULT_FONT_MACOS).exists() {
            let escaped = escape_drawtext(ts);
            let filter = format!(
                "drawtext=text='{escaped}':fontfile={font}:x=w-tw-20:y=h-th-20:fontsize=42:fontcolor=white:box=1:boxcolor=black@0.5:boxborderw=10",
                font = DEFAULT_FONT_MACOS
            );
            args.push("-vf".to_string());
            args.push(filter);
        }
    }
    args.push("-c:v".to_string());
    args.push("libx264".to_string());
    args.push("-preset".to_string());
    args.push("ultrafast".to_string());
    args.push("-crf".to_string());
    args.push("23".to_string());
    args.push("-pix_fmt".to_string());
    args.push("yuv420p".to_string());
    args.push(output.to_string_lossy().to_string());

    let r = Command::new("ffmpeg").args(&args).output()?;
    if !r.status.success() {
        anyhow::bail!(
            "clip_without_audio 失败:\n{}",
            tail(&String::from_utf8_lossy(&r.stderr), 1500)
        );
    }
    Ok(())
}

/// drawtext 中需要转义的字符 (单引号包裹模式下: 反斜杠和单引号)
fn escape_drawtext(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str(r"\\"),
            '\'' => out.push_str(r"\'"),
            ':' => out.push_str(r"\:"),
            _ => out.push(ch),
        }
    }
    out
}

fn tail(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let skip = s.chars().count() - max_chars;
        s.chars().skip(skip).collect()
    }
}

/// 默认输出文件名 ({stem}_clip.mp4)
pub fn default_clip_path(orig: &Path) -> PathBuf {
    let stem = orig
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("clip");
    PathBuf::from(format!("{stem}_clip.mp4"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawtext_escapes_special() {
        assert_eq!(escape_drawtext("2026-05-02 14:23:05"), r"2026-05-02 14\:23\:05");
        assert_eq!(escape_drawtext("a'b"), r"a\'b");
        assert_eq!(escape_drawtext(r"a\b"), r"a\\b");
    }
}
