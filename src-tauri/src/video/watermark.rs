// 时间戳水印 PNG 生成
//
// 背景: Homebrew 默认 ffmpeg 8.1 不含 libfreetype, 所以 drawtext filter 不可用
//   ([AVFilterGraph] No such filter: 'drawtext')
//
// 方案: Rust 端用 ab_glyph + image 把文本渲染成带半透明黑底的 PNG,
//   ffmpeg 用内置 overlay filter 把 PNG 烧到视频/截图右下角.
//   overlay 是 libavfilter 默认带的, 不依赖 freetype.

use std::path::{Path, PathBuf};

use ab_glyph::{Font, FontRef, Glyph, Point, ScaleFont};
use anyhow::{Context, Result};
use image::{ImageBuffer, Rgba};

const FONT_CANDIDATES: &[&str] = &[
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/System/Library/Fonts/Helvetica.ttc", // ttc 一般 ab_glyph 也能跑第 0 个 face
    "/Library/Fonts/Arial Unicode.ttf",
];

const FONT_SIZE_PX: f32 = 36.0;
const PAD_X: i32 = 12;
const PAD_Y: i32 = 8;
const FG: [u8; 4] = [255, 255, 255, 255];
const BG: [u8; 4] = [0, 0, 0, 160]; // 半透明黑

fn load_font_bytes() -> Result<Vec<u8>> {
    for path in FONT_CANDIDATES {
        if Path::new(path).exists() {
            if let Ok(b) = std::fs::read(path) {
                return Ok(b);
            }
        }
    }
    anyhow::bail!(
        "找不到任何系统字体 (尝试: {:?}). 时间戳水印无法生成.",
        FONT_CANDIDATES
    )
}

/// 把文本渲染为 RGBA PNG, 写入 out_path
///
/// 文本只用 ASCII (数字 + 横线 + 冒号 + 空格), 即使 ttc 也只用第 0 face
pub fn render_timestamp_png(text: &str, out_path: &Path) -> Result<(u32, u32)> {
    let bytes = load_font_bytes()?;
    let font = FontRef::try_from_slice(&bytes)
        .context("解析字体失败 (ttc 多 face 文件可能不被 ab_glyph 支持, 请装 .ttf 字体)")?;
    let scale = ab_glyph::PxScale::from(FONT_SIZE_PX);
    let scaled = font.as_scaled(scale);

    let h_adv: f32 = text
        .chars()
        .map(|c| scaled.h_advance(font.glyph_id(c)))
        .sum();
    let ascent = scaled.ascent();
    let descent = scaled.descent();
    let line_h = (ascent - descent).ceil() as i32;

    let img_w = (h_adv.ceil() as i32 + PAD_X * 2).max(1) as u32;
    let img_h = (line_h + PAD_Y * 2).max(1) as u32;

    let mut img = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_pixel(img_w, img_h, Rgba(BG));

    let mut pen_x = PAD_X as f32;
    let baseline_y = PAD_Y as f32 + ascent;
    for ch in text.chars() {
        let gid = font.glyph_id(ch);
        let advance = scaled.h_advance(gid);
        let glyph = Glyph {
            id: gid,
            scale,
            position: Point { x: pen_x, y: baseline_y },
        };
        if let Some(outlined) = font.outline_glyph(glyph) {
            let bbox = outlined.px_bounds();
            outlined.draw(|gx, gy, coverage| {
                let dx = bbox.min.x as i32 + gx as i32;
                let dy = bbox.min.y as i32 + gy as i32;
                if dx < 0 || dy < 0 {
                    return;
                }
                let dx = dx as u32;
                let dy = dy as u32;
                if dx >= img_w || dy >= img_h {
                    return;
                }
                // alpha-over: bg + fg*coverage
                let alpha = (coverage * 255.0).clamp(0.0, 255.0) as u8;
                if alpha == 0 {
                    return;
                }
                let p = img.get_pixel(dx, dy);
                let inv = 255 - alpha;
                let r = ((p[0] as u16 * inv as u16 + FG[0] as u16 * alpha as u16) / 255) as u8;
                let g = ((p[1] as u16 * inv as u16 + FG[1] as u16 * alpha as u16) / 255) as u8;
                let b = ((p[2] as u16 * inv as u16 + FG[2] as u16 * alpha as u16) / 255) as u8;
                let a = (p[3] as u16).max(alpha as u16) as u8;
                img.put_pixel(dx, dy, Rgba([r, g, b, a]));
            });
        }
        pen_x += advance;
    }

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    img.save(out_path).with_context(|| format!("写入水印 PNG 失败: {}", out_path.display()))?;
    Ok((img_w, img_h))
}

/// 在临时目录生成时间戳 PNG, 返回路径 (调用方负责删除)
pub fn make_temp_timestamp_png(text: &str) -> Result<PathBuf> {
    let mut tmp = std::env::temp_dir();
    let name = format!(
        "ipr_ts_{}_{}.png",
        std::process::id(),
        uuid::Uuid::new_v4()
    );
    tmp.push(name);
    render_timestamp_png(text, &tmp)?;
    Ok(tmp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_basic_ascii() {
        let tmp = std::env::temp_dir().join("ipr_ts_test.png");
        let r = render_timestamp_png("2026-05-02 14:23:05", &tmp);
        // 没字体环境下也算合理失败 (CI 上可能没 Arial), 但本地 macOS 应有
        if let Ok((w, h)) = r {
            assert!(w > 50);
            assert!(h > 20);
            assert!(tmp.exists());
            std::fs::remove_file(&tmp).ok();
        }
    }
}
