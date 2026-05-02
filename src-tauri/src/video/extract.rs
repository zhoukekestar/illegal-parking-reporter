// 视频抽帧 (1 fps + EXIF 旋转)
//
// 关键设计:
//   - 解码全部帧, 但只在 pts >= next_target 时保留, 实现 1fps 等效抽样
//   - 旋转必须在 RGB 转换 *之后* 应用 (image crate 旋转算子作用于像素 buffer)
//   - sws_scale 把任意输入像素格式 -> RGB24, 再封装为 image::RgbImage

use std::path::Path;

use anyhow::{Context, Result};
use ffmpeg_next as ffmpeg;
use ffmpeg::format::Pixel;
use ffmpeg::media::Type;
use ffmpeg::software::scaling::{Context as SwsContext, Flags};
use ffmpeg::util::frame::video::Video as VideoFrame;
use image::RgbImage;

use crate::video::metadata::detect_rotation;

/// 抽样得到的单帧
pub struct ExtractedFrame {
    /// 在抽样序列中的索引 (0 开始, 不是原始帧号)
    pub frame_index: usize,
    /// 距视频起点的毫秒数
    pub timestamp_ms: i64,
    /// 已经过 EXIF 旋转的 RGB 图像 (即 "正立" 显示)
    pub image: RgbImage,
}

/// 抽帧选项
pub struct ExtractOptions {
    /// 目标抽样帧率 (例如 1.0 = 1fps)
    pub target_fps: f32,
    /// 最多抽取多少帧, None 表示无限
    pub max_frames: Option<usize>,
}

impl Default for ExtractOptions {
    fn default() -> Self {
        Self {
            target_fps: 1.0,
            max_frames: None,
        }
    }
}

/// 从视频抽帧, 返回所有抽样到的帧 (内存中)
///
/// **注意**: 默认会把全部抽样帧持有在内存中, 适合短视频或受 max_frames 限制的场景。
/// P2 的批处理流水线会改用流式 callback 接口避免内存峰值。
pub fn extract_frames(path: &Path, opts: &ExtractOptions) -> Result<Vec<ExtractedFrame>> {
    let mut ictx = ffmpeg::format::input(&path)
        .with_context(|| format!("无法打开视频: {}", path.display()))?;

    let stream = ictx
        .streams()
        .best(Type::Video)
        .context("视频文件中没有视频流")?;
    let stream_index = stream.index();
    let time_base = stream.time_base();
    let rotation_cw = detect_rotation(&stream);

    let codec_params = stream.parameters();
    let codec_ctx = ffmpeg::codec::Context::from_parameters(codec_params.clone())?;
    let mut decoder = codec_ctx.decoder().video()?;

    let mut scaler = SwsContext::get(
        decoder.format(),
        decoder.width(),
        decoder.height(),
        Pixel::RGB24,
        decoder.width(),
        decoder.height(),
        Flags::BILINEAR,
    )
    .context("创建 sws_scale 上下文失败")?;

    let interval_secs = 1.0 / opts.target_fps as f64;
    let mut next_target_secs: f64 = 0.0;
    let mut extracted: Vec<ExtractedFrame> = Vec::new();
    let mut decoded = VideoFrame::empty();

    let try_keep = |decoded: &VideoFrame,
                        scaler: &mut SwsContext,
                        next_target_secs: &mut f64,
                        extracted: &mut Vec<ExtractedFrame>|
     -> Result<bool> {
        let pts = match decoded.pts() {
            Some(p) => p,
            None => return Ok(false),
        };
        let t_secs = pts as f64 * f64::from(time_base);
        if t_secs < *next_target_secs {
            return Ok(false);
        }

        let mut rgb = VideoFrame::empty();
        scaler.run(decoded, &mut rgb).context("sws_scale 失败")?;

        let img = rgb_frame_to_image(&rgb).context("RGB 帧转 RgbImage 失败")?;
        let img = apply_rotation(img, rotation_cw);

        let frame_index = extracted.len();
        extracted.push(ExtractedFrame {
            frame_index,
            timestamp_ms: (t_secs * 1000.0) as i64,
            image: img,
        });
        *next_target_secs += interval_secs;

        if let Some(max) = opts.max_frames {
            if extracted.len() >= max {
                return Ok(true);
            }
        }
        Ok(false)
    };

    'demux: for (s, packet) in ictx.packets() {
        if s.index() != stream_index {
            continue;
        }
        decoder.send_packet(&packet).context("解码 send_packet 失败")?;
        loop {
            match decoder.receive_frame(&mut decoded) {
                Ok(()) => {
                    if try_keep(&decoded, &mut scaler, &mut next_target_secs, &mut extracted)? {
                        break 'demux;
                    }
                }
                Err(ffmpeg::Error::Other { errno: ffmpeg::error::EAGAIN }) => break,
                Err(ffmpeg::Error::Eof) => break 'demux,
                Err(e) => return Err(e).context("解码 receive_frame 失败"),
            }
        }
    }

    // flush
    decoder.send_eof().ok();
    loop {
        match decoder.receive_frame(&mut decoded) {
            Ok(()) => {
                if try_keep(&decoded, &mut scaler, &mut next_target_secs, &mut extracted)? {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    tracing::info!(
        path = %path.display(),
        extracted = extracted.len(),
        rotation_cw,
        "抽帧完成"
    );

    Ok(extracted)
}

fn rgb_frame_to_image(frame: &VideoFrame) -> Result<RgbImage> {
    let w = frame.width();
    let h = frame.height();
    let stride = frame.stride(0);
    let data = frame.data(0);

    let mut img = RgbImage::new(w, h);
    for y in 0..h {
        let row_off = y as usize * stride;
        for x in 0..w {
            let off = row_off + (x as usize) * 3;
            // sws_scale 输出 RGB24, 且 stride 可能 > w*3 (对齐填充), 所以必须按 stride 索引
            let r = data[off];
            let g = data[off + 1];
            let b = data[off + 2];
            img.put_pixel(x, y, image::Rgb([r, g, b]));
        }
    }
    Ok(img)
}

/// 按"顺时针应用"语义旋转图像
fn apply_rotation(img: RgbImage, cw_degrees: i32) -> RgbImage {
    match cw_degrees {
        0 => img,
        90 => image::imageops::rotate90(&img),
        180 => image::imageops::rotate180(&img),
        270 => image::imageops::rotate270(&img),
        _ => img,
    }
}
