// 视频处理领域模块
// P1: 元数据读取 / 1fps 抽帧 / EXIF 旋转
// P3: 剪辑 / 时间戳水印

pub mod clip;
pub mod extract;
pub mod metadata;

/// 全局初始化 ffmpeg 库 (注册 codec/format)
///
/// 必须在使用任何其他 ffmpeg API 之前调用一次, 之后重复调用是幂等的
pub fn init() -> anyhow::Result<()> {
    ffmpeg_next::init().map_err(|e| anyhow::anyhow!("ffmpeg 初始化失败: {e}"))?;
    tracing::info!(
        version = ffmpeg_next::util::version(),
        "ffmpeg 已初始化"
    );
    Ok(())
}
