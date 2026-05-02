// 索引 CSV 生成 (P5)
//
// 字段 (DEVELOPMENT_PLAN.md §五 P5):
//   序号, 车牌号, 时间, 来源视频, 违法类型, 文件夹路径, 已上传(Y/N)
//
// UTF-8 BOM (\xef\xbb\xbf), 让 Excel 在 macOS 双击也能正常显示中文

use std::fs::File;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};

use crate::models::event::ParkingEvent;

pub fn write_index_csv(events: &[ParkingEvent], out: &Path) -> Result<()> {
    let mut f = File::create(out).with_context(|| format!("创建 CSV 失败: {}", out.display()))?;
    // UTF-8 BOM
    f.write_all(&[0xEF, 0xBB, 0xBF])?;
    // 表头
    writeln!(
        f,
        "序号,车牌号,时间,来源视频,违法类型,文件夹路径,已上传(Y/N)"
    )?;
    // 行
    for (i, e) in events.iter().enumerate() {
        let plate = e.plate_manual_corrected.clone().unwrap_or(e.plate_number.clone());
        let time = e.event_time.clone().unwrap_or_else(|| {
            format!(
                "视频偏移 {:.1}s",
                e.timestamp_ms as f64 / 1000.0
            )
        });
        let source = e.source_video.clone();
        let category = "占用人行道".to_string();
        let folder = e.export_path.clone().unwrap_or_default();
        let uploaded = "N".to_string();

        let row = vec![
            (i + 1).to_string(),
            csv_quote(&plate),
            csv_quote(&time),
            csv_quote(&source),
            csv_quote(&category),
            csv_quote(&folder),
            uploaded,
        ];
        writeln!(f, "{}", row.join(","))?;
    }
    f.flush()?;
    Ok(())
}

/// 简易 CSV 字段引用: 含 , " \n 时双引号包裹, 内部 " 转 ""
fn csv_quote(s: &str) -> String {
    let needs = s.contains(',') || s.contains('"') || s.contains('\n');
    if needs {
        let escaped = s.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_handles_special_chars() {
        assert_eq!(csv_quote("foo"), "foo");
        assert_eq!(csv_quote("a,b"), "\"a,b\"");
        assert_eq!(csv_quote("a\"b"), "\"a\"\"b\"");
        assert_eq!(csv_quote("a\nb"), "\"a\nb\"");
    }
}
