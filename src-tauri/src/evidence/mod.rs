// 证据包生成 (P3)
//
// 单事件输出文件夹结构 (DEVELOPMENT_PLAN.md §三 §五 P3):
//   {车牌}_{源视频名}_{HHMMSS}/
//     ├── 截图.jpg      原分辨率, 烧录时间戳水印
//     ├── 视频.mp4      原分辨率, 6 秒, 烧录时间戳水印
//     └── 信息.txt      纯文本可复制 (车牌/时间/违法类型/置信度)
//
// P5 会在此之上加索引 CSV + 上传指引 PDF + 顶层"违停举报包_YYYY-MM-DD_HH-MM"

pub mod builder;
pub mod csv_index;
pub mod exporter;
pub mod html_guide;

use std::path::PathBuf;

/// 证据包根目录
///
/// dev: <crate>/.local/evidence/
/// release: $HOME/Library/Application Support/路况记录助手/evidence/
pub fn evidence_root() -> anyhow::Result<PathBuf> {
    if let Ok(p) = std::env::var("IPR_EVIDENCE_DIR") {
        return Ok(PathBuf::from(p));
    }
    #[cfg(debug_assertions)]
    {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push(".local");
        p.push("evidence");
        std::fs::create_dir_all(&p)?;
        return Ok(p);
    }
    #[cfg(not(debug_assertions))]
    {
        let home = std::env::var("HOME").map_err(|_| anyhow::anyhow!("HOME 未设置"))?;
        let mut p = PathBuf::from(home);
        #[cfg(target_os = "macos")]
        {
            p.push("Library");
            p.push("Application Support");
            p.push("路况记录助手");
        }
        #[cfg(not(target_os = "macos"))]
        {
            p.push(".local");
            p.push("share");
            p.push("illegal-parking-reporter");
        }
        p.push("evidence");
        std::fs::create_dir_all(&p)?;
        Ok(p)
    }
}
