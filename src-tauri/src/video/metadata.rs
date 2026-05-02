// 视频元数据读取
// 关键字段:
//   - creation_time: 拍摄时间 (取 format metadata 优先, fallback stream metadata)
//   - duration: 时长(秒)
//   - frame_rate: 平均帧率
//   - rotation: EXIF/displaymatrix 旋转度数 (顺时针, iPhone 竖屏视频是 90)
//   - width/height: 原始(未旋转)分辨率, 仅用于 UI 显示

use std::path::Path;

use anyhow::{Context, Result};
use ffmpeg_next as ffmpeg;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct VideoMetadata {
    /// ISO 8601 格式拍摄时间, 来自容器 metadata.creation_time
    pub creation_time: Option<String>,
    /// 总时长 (秒)
    pub duration_seconds: f64,
    /// 平均帧率
    pub frame_rate: f64,
    /// 旋转角度 (顺时针度数, 0/90/180/270 之一, 0 表示不旋转)
    pub rotation_degrees: i32,
    /// 原始分辨率 (未应用旋转)
    pub width: u32,
    pub height: u32,
    /// 应用旋转后的"显示分辨率"
    pub display_width: u32,
    pub display_height: u32,
    /// 视频编码 (h264 / hevc / ...)
    pub codec_name: String,
    /// 文件大小 (字节)
    pub file_size_bytes: u64,
}

/// 从视频文件读取元数据 (不解码任何帧)
pub fn read_metadata(path: &Path) -> Result<VideoMetadata> {
    let file_size_bytes = std::fs::metadata(path)
        .with_context(|| format!("无法读取文件信息: {}", path.display()))?
        .len();

    let ictx = ffmpeg::format::input(&path)
        .with_context(|| format!("无法打开视频: {}", path.display()))?;

    // 容器级 duration (微秒)
    let duration_us = ictx.duration();
    let duration_seconds = if duration_us > 0 {
        duration_us as f64 / f64::from(ffmpeg::ffi::AV_TIME_BASE)
    } else {
        0.0
    };

    // 容器 metadata
    let creation_time = ictx
        .metadata()
        .get("creation_time")
        .map(|s| s.to_string());

    // 找视频流
    let stream = ictx
        .streams()
        .best(ffmpeg::media::Type::Video)
        .context("视频文件中没有视频流")?;

    let codec_params = stream.parameters();
    let codec_ctx = ffmpeg::codec::Context::from_parameters(codec_params.clone())?;
    let decoder = codec_ctx.decoder().video()?;

    let width = decoder.width();
    let height = decoder.height();
    let codec_name = decoder
        .codec()
        .map(|c| c.name().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // 帧率: 优先 avg_frame_rate, fallback r_frame_rate
    let avg = stream.avg_frame_rate();
    let frame_rate = if avg.denominator() != 0 {
        avg.numerator() as f64 / avg.denominator() as f64
    } else {
        let rfr = stream.rate();
        if rfr.denominator() != 0 {
            rfr.numerator() as f64 / rfr.denominator() as f64
        } else {
            0.0
        }
    };

    let rotation_degrees = detect_rotation(&stream);

    let (display_width, display_height) = match rotation_degrees {
        90 | 270 => (height, width),
        _ => (width, height),
    };

    Ok(VideoMetadata {
        creation_time,
        duration_seconds,
        frame_rate,
        rotation_degrees,
        width,
        height,
        display_width,
        display_height,
        codec_name,
        file_size_bytes,
    })
}

/// 探测视频流的旋转角度 (顺时针度数, 0-359)
///
/// 顺序:
///   1. legacy stream metadata "rotate" tag (旧 ffmpeg / 部分 iPhone 导出)
///   2. AV_PKT_DATA_DISPLAYMATRIX 边数据 (现代 iPhone 视频, 走 sys API)
///
/// 返回 0 表示无旋转或检测失败
pub(crate) fn detect_rotation(stream: &ffmpeg::Stream<'_>) -> i32 {
    // 1. 兼容旧 metadata tag
    if let Some(s) = stream.metadata().get("rotate") {
        if let Ok(r) = s.trim().parse::<i32>() {
            return normalize_cw(r);
        }
    }

    // 2. displaymatrix side data (FFmpeg 8 在 codecpar 上)
    unsafe {
        let par_ptr = stream.parameters().as_ptr();
        let nb = (*par_ptr).nb_coded_side_data;
        let arr = (*par_ptr).coded_side_data;
        if !arr.is_null() && nb > 0 {
            for i in 0..nb {
                let sd = arr.offset(i as isize);
                if (*sd).type_ == ffmpeg::ffi::AVPacketSideDataType::AV_PKT_DATA_DISPLAYMATRIX {
                    let matrix = (*sd).data as *const i32;
                    let rot_ccw = ffmpeg::ffi::av_display_rotation_get(matrix);
                    if rot_ccw.is_finite() {
                        // av_display_rotation_get 返回逆时针度数,
                        // 我们存"顺时针应用以正常显示"的度数
                        let cw = -rot_ccw.round() as i32;
                        return normalize_cw(cw);
                    }
                }
            }
        }
    }

    0
}

fn normalize_cw(deg: i32) -> i32 {
    let n = ((deg % 360) + 360) % 360;
    // 只对齐到 90 倍数 (其他角度极少且我们不支持任意角度旋转)
    let aligned = ((n + 45) / 90) * 90 % 360;
    aligned
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_handles_negatives_and_overflow() {
        assert_eq!(normalize_cw(0), 0);
        assert_eq!(normalize_cw(90), 90);
        assert_eq!(normalize_cw(-90), 270);
        assert_eq!(normalize_cw(450), 90);
        assert_eq!(normalize_cw(-270), 90);
        // 接近 90 的角度对齐到 90
        assert_eq!(normalize_cw(89), 90);
        assert_eq!(normalize_cw(91), 90);
    }
}
