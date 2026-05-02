// 证据视频剪辑 + 时间戳水印 (P3)
//
// ffmpeg CLI 子进程实现.
//
// 时间戳水印:
//   - Homebrew ffmpeg 不带 libfreetype, drawtext 不可用
//   - 改用 Rust 端 ab_glyph 渲染 PNG + ffmpeg overlay filter (overlay 默认带)
//   - 见 video::watermark

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use crate::video::watermark::make_temp_timestamp_png;

/// 剪辑选项
pub struct ClipOptions {
    /// 起始秒数 (距视频开头)
    pub start_secs: f64,
    /// 持续秒数 (典型 6.0 = 前 3 + 后 3)
    pub duration_secs: f64,
    /// 烧录的时间戳文本 (例 "2026-05-02 14:23:05"), None 表示不加水印
    pub timestamp_text: Option<String>,
}

struct WatermarkAsset {
    png_path: PathBuf,
}
impl Drop for WatermarkAsset {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.png_path);
    }
}

fn maybe_make_watermark(text: &Option<String>) -> Option<WatermarkAsset> {
    let t = text.as_ref()?;
    match make_temp_timestamp_png(t) {
        Ok(p) => Some(WatermarkAsset { png_path: p }),
        Err(e) => {
            tracing::warn!(error = %e, "渲染时间戳水印 PNG 失败, 视频/截图将不带水印");
            None
        }
    }
}

/// 把 [start, start+duration] 段剪出来, 重编码 H.264 + 可选烧录时间戳水印
pub fn clip_with_watermark(input: &Path, output: &Path, opts: &ClipOptions) -> Result<()> {
    if !input.exists() {
        anyhow::bail!("输入视频不存在: {}", input.display());
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).context("创建输出目录失败")?;
    }

    let watermark = maybe_make_watermark(&opts.timestamp_text);

    let mut args: Vec<String> = vec![
        "-y".to_string(),
        "-i".to_string(),
        input.to_string_lossy().to_string(),
    ];
    // overlay 输入: 必须先有 -i 才能用 [1:v]
    if let Some(w) = &watermark {
        args.push("-i".to_string());
        args.push(w.png_path.to_string_lossy().to_string());
    }
    args.push("-ss".to_string());
    args.push(format!("{:.3}", opts.start_secs.max(0.0)));
    args.push("-t".to_string());
    args.push(format!("{:.3}", opts.duration_secs.max(0.1)));

    if watermark.is_some() {
        // 视频在右下角叠加水印 PNG (距右/下 20px)
        args.push("-filter_complex".to_string());
        args.push("[0:v][1:v]overlay=W-w-20:H-h-20[outv]".to_string());
        args.push("-map".to_string());
        args.push("[outv]".to_string());
        // 音频流如果存在则 map 进来 (用 ? 让缺失不报错)
        args.push("-map".to_string());
        args.push("0:a?".to_string());
    }

    args.push("-c:v".to_string());
    args.push("libx264".to_string());
    args.push("-preset".to_string());
    args.push("ultrafast".to_string());
    args.push("-crf".to_string());
    args.push("23".to_string());
    args.push("-pix_fmt".to_string());
    args.push("yuv420p".to_string());
    args.push("-c:a".to_string());
    args.push("copy".to_string());
    args.push(output.to_string_lossy().to_string());

    tracing::info!(
        input = %input.display(),
        output = %output.display(),
        start_secs = opts.start_secs,
        duration_secs = opts.duration_secs,
        watermark = watermark.is_some(),
        "ffmpeg 剪辑开始"
    );
    let output_res = Command::new("ffmpeg")
        .args(&args)
        .output()
        .context("启动 ffmpeg 失败 (确认 ffmpeg 在 PATH 内)")?;

    if !output_res.status.success() {
        let stderr = String::from_utf8_lossy(&output_res.stderr);
        let stderr_tail = tail(&stderr, 2000);
        let cmd_str = format_cmd("ffmpeg", &args);
        tracing::error!(
            cmd = %cmd_str,
            exit = ?output_res.status.code(),
            stderr = %stderr_tail,
            "ffmpeg 剪辑失败"
        );
        // 音频映射失败时降级
        if stderr.contains("Could not find tag")
            || stderr.contains("does not contain any stream")
            || stderr.contains("Could not write header")
            || (stderr.contains("aac") && stderr.contains("Invalid"))
        {
            tracing::warn!("尝试去掉音频流降级重试");
            return clip_without_audio(input, output, opts, watermark);
        }
        anyhow::bail!(
            "ffmpeg 退出码 {:?}, stderr:\n{}",
            output_res.status.code(),
            stderr_tail
        );
    }
    tracing::info!(output = %output.display(), "ffmpeg 剪辑完成");
    drop(watermark); // 显式 drop 以删 PNG (Drop 也会做)
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

    let watermark =
        maybe_make_watermark(&timestamp_text.map(|s| s.to_string()));

    let mut args: Vec<String> = vec![
        "-y".to_string(),
        "-i".to_string(),
        input.to_string_lossy().to_string(),
    ];
    if let Some(w) = &watermark {
        args.push("-i".to_string());
        args.push(w.png_path.to_string_lossy().to_string());
    }
    args.push("-ss".to_string());
    args.push(format!("{:.3}", snapshot_secs.max(0.0)));
    args.push("-frames:v".to_string());
    args.push("1".to_string());

    if watermark.is_some() {
        args.push("-filter_complex".to_string());
        args.push("[0:v][1:v]overlay=W-w-20:H-h-20".to_string());
    }

    args.push("-q:v".to_string());
    args.push("2".to_string());
    args.push(output.to_string_lossy().to_string());

    tracing::info!(
        input = %input.display(),
        output = %output.display(),
        secs = snapshot_secs,
        watermark = watermark.is_some(),
        "ffmpeg 截图开始"
    );
    let output_res = Command::new("ffmpeg")
        .args(&args)
        .output()
        .context("启动 ffmpeg 失败")?;
    if !output_res.status.success() {
        let stderr = String::from_utf8_lossy(&output_res.stderr);
        let stderr_tail = tail(&stderr, 2000);
        let cmd_str = format_cmd("ffmpeg", &args);
        tracing::error!(
            cmd = %cmd_str,
            exit = ?output_res.status.code(),
            stderr = %stderr_tail,
            "ffmpeg 截图失败"
        );
        anyhow::bail!(
            "截图 ffmpeg 退出码 {:?}, stderr:\n{}",
            output_res.status.code(),
            stderr_tail
        );
    }
    tracing::info!(output = %output.display(), "ffmpeg 截图完成");
    drop(watermark);
    Ok(())
}

fn clip_without_audio(
    input: &Path,
    output: &Path,
    opts: &ClipOptions,
    watermark: Option<WatermarkAsset>,
) -> Result<()> {
    let mut args: Vec<String> = vec![
        "-y".to_string(),
        "-i".to_string(),
        input.to_string_lossy().to_string(),
    ];
    if let Some(w) = &watermark {
        args.push("-i".to_string());
        args.push(w.png_path.to_string_lossy().to_string());
    }
    args.push("-ss".to_string());
    args.push(format!("{:.3}", opts.start_secs.max(0.0)));
    args.push("-t".to_string());
    args.push(format!("{:.3}", opts.duration_secs.max(0.1)));
    args.push("-an".to_string());
    if watermark.is_some() {
        args.push("-filter_complex".to_string());
        args.push("[0:v][1:v]overlay=W-w-20:H-h-20[outv]".to_string());
        args.push("-map".to_string());
        args.push("[outv]".to_string());
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

    tracing::info!(
        input = %input.display(),
        output = %output.display(),
        "ffmpeg 剪辑 (无音频降级) 开始"
    );
    let r = Command::new("ffmpeg").args(&args).output()?;
    if !r.status.success() {
        let stderr = String::from_utf8_lossy(&r.stderr);
        let stderr_tail = tail(&stderr, 2000);
        let cmd_str = format_cmd("ffmpeg", &args);
        tracing::error!(
            cmd = %cmd_str,
            exit = ?r.status.code(),
            stderr = %stderr_tail,
            "ffmpeg 无音频降级也失败"
        );
        anyhow::bail!("clip_without_audio 失败:\n{}", stderr_tail);
    }
    tracing::info!(output = %output.display(), "ffmpeg 剪辑 (无音频) 完成");
    drop(watermark);
    Ok(())
}

/// 把 ffmpeg 命令格式化为可重现的 shell 字符串 (用于日志)
fn format_cmd(prog: &str, args: &[String]) -> String {
    let mut out = String::with_capacity(prog.len() + 256);
    out.push_str(prog);
    for a in args {
        out.push(' ');
        if a.contains(|c: char| c.is_whitespace() || "&|;<>()'\"$`\\".contains(c)) {
            out.push('\'');
            out.push_str(&a.replace('\'', "'\\''"));
            out.push('\'');
        } else {
            out.push_str(a);
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
