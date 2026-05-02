// Tauri 命令层: 仅做参数转换 + 调用业务逻辑 + 错误转换
// 业务逻辑放在 ai/ video/ 等领域模块, 命令层保持薄

pub mod detection;
pub mod system;
pub mod video;
