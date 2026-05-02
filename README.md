# 路况记录助手

桌面端违停证据包生成器, 本地 AI 识别视频中违停车辆, 生成标准证据包供手动上传至举报平台。

> **当前阶段**: P0 - 工程脚手架。完整开发计划见 `DEVELOPMENT_PLAN.md`。

## 特性 (规划)

- 本地 AI 识别 (YOLOv8 + SegFormer + HyperLPR3), 不联网
- 支持 iPhone 横屏/竖屏视频, 自动旋转
- 输出符合警察叔叔 App 要求的证据包 (截图 + 6s 视频 + 时间戳水印)
- 键盘流审核 UI, 30 事件 5 分钟审完
- 本地数据加密存储 (SQLCipher)
- 可选 iPhone Mirroring 上传助手

## 技术栈

- Tauri 2.x + Vue 3 + TypeScript + Element Plus
- Rust 后端: ort (ONNX) / ffmpeg-next / tokio / SQLite + SQLCipher
- 平台: macOS M1 优先, 跨平台见 V2

许可证: AGPL-3.0 (受 YOLOv8 传染)

## 快速开始

详见 [docs/SETUP.md](docs/SETUP.md) 与 [docs/MODELS.md](docs/MODELS.md)。

```bash
# 1. 装工具链 (Rust / Node / onnxruntime / ffmpeg) — 见 SETUP.md
# 2. 准备模型文件 — 见 MODELS.md
# 3. 安装依赖
npm install
# 4. 启动开发模式
npm run tauri:dev
```

## 文档

- [DEVELOPMENT_PLAN.md](DEVELOPMENT_PLAN.md) - 完整开发计划 (做什么)
- [AI_PROMPT.md](AI_PROMPT.md) - AI 协作规范 (怎么做)
- [docs/SETUP.md](docs/SETUP.md) - 环境搭建
- [docs/MODELS.md](docs/MODELS.md) - 模型准备
- [docs/ROADMAP.md](docs/ROADMAP.md) - 路线图

## 法律与合规

- 不得驾驶过程中拍摄
- 不得进入机动车道拍摄
- 软件仅生成证据包, 最终举报由用户在手机 App 完成
- 软件不传输任何数据到第三方
- 误报/虚假举报责任由用户承担
