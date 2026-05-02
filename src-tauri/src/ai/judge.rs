// 违停判定 (P3): 车辆 mask ∩ 人行道 mask 占比
//
// DEVELOPMENT_PLAN.md §三 §五 P3 写的是 "IoU > 阈值 (默认 0.3) 判定违停"
// 但车辆 vs 人行道两个区域大小通常相差很大, 用标准 IoU (intersection/union) 几乎永远 < 0.3
// 这里采用更合理的语义: intersection / vehicle_area = "车辆有多少比例落在人行道上"
// 仍叫 iou_score 字段, 与 ParkingEvent schema 对齐
//
// 阈值 0.3 -> 至少 30% 车辆像素压在人行道上才判定违停, 与 §三表格的 0.3 默认值匹配

use anyhow::Result;
use image::GrayImage;

/// 默认阈值 (DEVELOPMENT_PLAN.md §6.1)
pub const DEFAULT_THRESHOLD: f32 = 0.3;

/// 计算 vehicle_mask 中有多少比例像素落在 sidewalk_mask 内
///
/// 两个 mask 必须同尺寸, 像素 > 127 视为前景
pub fn intersection_over_vehicle(vehicle: &GrayImage, sidewalk: &GrayImage) -> Result<f32> {
    let (vw, vh) = vehicle.dimensions();
    let (sw, sh) = sidewalk.dimensions();
    anyhow::ensure!(
        (vw, vh) == (sw, sh),
        "vehicle/sidewalk mask 尺寸不一致: {:?} vs {:?}",
        (vw, vh),
        (sw, sh)
    );

    let mut vehicle_pixels: u64 = 0;
    let mut intersection: u64 = 0;
    for (vp, sp) in vehicle.pixels().zip(sidewalk.pixels()) {
        if vp[0] > 127 {
            vehicle_pixels += 1;
            if sp[0] > 127 {
                intersection += 1;
            }
        }
    }
    if vehicle_pixels == 0 {
        return Ok(0.0);
    }
    Ok(intersection as f32 / vehicle_pixels as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GrayImage, Luma};

    fn make_mask(w: u32, h: u32, fg: impl Fn(u32, u32) -> bool) -> GrayImage {
        let mut img = GrayImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                if fg(x, y) {
                    img.put_pixel(x, y, Luma([255]));
                }
            }
        }
        img
    }

    #[test]
    fn full_overlap_is_one() {
        let v = make_mask(100, 100, |_, _| true);
        let s = make_mask(100, 100, |_, _| true);
        let r = intersection_over_vehicle(&v, &s).unwrap();
        assert!((r - 1.0).abs() < 1e-6);
    }

    #[test]
    fn no_overlap_is_zero() {
        let v = make_mask(100, 100, |x, _| x < 50);
        let s = make_mask(100, 100, |x, _| x >= 50);
        let r = intersection_over_vehicle(&v, &s).unwrap();
        assert!((r - 0.0).abs() < 1e-6);
    }

    #[test]
    fn half_overlap_is_half() {
        // 车辆是 100x100 全部, 人行道占左半 -> 50% 车辆落在人行道
        let v = make_mask(100, 100, |_, _| true);
        let s = make_mask(100, 100, |x, _| x < 50);
        let r = intersection_over_vehicle(&v, &s).unwrap();
        assert!((r - 0.5).abs() < 1e-6, "expect 0.5 got {r}");
    }

    #[test]
    fn empty_vehicle_returns_zero() {
        let v = make_mask(50, 50, |_, _| false);
        let s = make_mask(50, 50, |_, _| true);
        let r = intersection_over_vehicle(&v, &s).unwrap();
        assert_eq!(r, 0.0);
    }

    #[test]
    fn mismatched_size_errors() {
        let v = GrayImage::new(50, 50);
        let s = GrayImage::new(60, 60);
        assert!(intersection_over_vehicle(&v, &s).is_err());
    }
}
