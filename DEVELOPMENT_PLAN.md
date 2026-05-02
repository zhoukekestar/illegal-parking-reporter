# 路况记录助手 - 完整开发计划

> 桌面端违停证据包生成器
> 本地 AI 识别视频中违停车辆,生成标准证据包供手动上传至举报平台
> 目标平台: macOS (M1 优先) | 后期可扩展 Windows / Linux

---

## 一、项目背景与定位

### 1.1 用户场景

杭州地区用户拍摄手机视频后,希望批量识别视频中违停车辆(占用人行道),
自动生成包含车牌、截图、短视频片段的标准证据包,然后通过手机上传到
"警察叔叔" App 完成举报。

### 1.2 软件定位

**违停证据包生成器**,不是"自动举报工具"。

- 软件做的事: 识别 + 复核 + 证据生成 + 上传辅助
- 用户做的事: 最终在手机 App 提交举报(可选 iPhone Mirroring 半自动)

### 1.3 调研结论 (重要前提)

杭州的违停举报渠道**全部是移动端**,没有 PC 网页入口:

- 警察叔叔 App (杭州市公安局)
- 支付宝"城市服务" → 车主服务 → 交通违法有奖举报
- 交通拍客 App (浙江省公安厅交管局)

三个平台数据互通,选其一即可。**桌面端无法直接完成举报提交**,
必须有"从桌面到手机"的桥梁。

举报材料硬性要求:
- 内嵌日期时间的原始视频/照片
- 车牌清晰
- 违法行为过程完整体现
- 举报时间 ≤ 拍摄时间 + 72 小时

### 1.4 合规与法律边界

- 不得驾驶过程中拍摄
- 不得进入机动车道拍摄
- 不替用户提交,最终提交动作由用户完成
- 不联网传输车牌/视频等个人信息
- 本地数据加密 (《个人信息保护法》要求)

---

## 二、技术选型

### 2.1 整体架构

```
┌─────────────────────────────────────────────────┐
│  Tauri 前端 (Vue 3 + TypeScript + Element Plus) │
│  ├─ Upload     视频上传/批量选择/拖拽            │
│  ├─ Processing 实时进度展示                      │
│  ├─ Review     事件审核(键盘流 + 车牌修正)      │
│  ├─ Export     批量导出证据包                    │
│  └─ Settings   参数配置/模型检查                 │
└──────────────┬──────────────────────────────────┘
               │ Tauri IPC + Event
┌──────────────▼──────────────────────────────────┐
│  Rust 后端                                       │
│  ├─ Pipeline    tokio 流水线(抽帧/AI/聚合)      │
│  ├─ AI          ort 调用 ONNX 模型               │
│  │   ├─ YOLOv8-seg     车辆检测(掩膜)           │
│  │   ├─ SegFormer-B0   人行道分割                │
│  │   └─ HyperLPR3      车牌识别                  │
│  ├─ Video       ffmpeg-next 抽帧/剪辑/水印      │
│  ├─ Judge       违停判定(IoU 计算)              │
│  ├─ Evidence    证据包生成                       │
│  ├─ DB          SQLite + SQLCipher 加密存储     │
│  └─ Mirror      iPhone Mirroring 助手 (P8)      │
└──────────────────────────────────────────────────┘
```

### 2.2 技术栈清单

| 模块 | 选型 | 协议 | 备注 |
|------|------|------|------|
| 桌面框架 | Tauri 2.x | MIT/Apache | 体积小、性能好 |
| 前端 | Vue 3 + TypeScript | MIT | |
| UI 库 | Element Plus | MIT | |
| 状态管理 | Pinia | MIT | |
| Rust ONNX 推理 | ort 2.0 | MIT/Apache | M1 上启用 CoreML 加速 |
| 视频处理 | ffmpeg-next | LGPL (FFmpeg) | |
| 车辆检测/分割 | YOLOv8n-seg | **AGPL-3.0** | 软件最终也走 AGPL |
| 人行道分割 | SegFormer-B0 ADE20K | NVIDIA SCL | 学术/开源友好 |
| 车牌识别 | HyperLPR3 | Apache 2.0 | 专为中国车牌优化 |
| 数据库 | SQLite + SQLCipher | Public Domain / BSD | 加密本地存储 |
| 异步运行时 | tokio | MIT | |

### 2.3 软件最终许可证

**AGPL-3.0** (受 YOLOv8 传染),全开源分发。

---

## 三、需求规格

### 3.1 输入

- **视频来源**: iPhone 原始视频 (用户主动选择,不经压缩,保留 EXIF 元数据)
- **批量规模**: 单批 10 个左右
- **格式**: mp4 / mov / m4v
- **方向**: 横屏 / 竖屏均支持

### 3.2 核心识别逻辑

**违停判定 = "车辆 + 占用人行道"**

1. YOLOv8-seg 输出每辆车的实例分割掩膜
2. SegFormer 输出人行道语义分割掩膜
3. 计算车辆掩膜 ∩ 人行道掩膜 IoU
4. IoU > 阈值 (默认 0.3) 判定为占用人行道
5. 对判定违停的车辆,用 HyperLPR3 识别车牌

### 3.3 输出

每个事件输出一个文件夹:

```
违停举报包_2026-05-02_18-30/
├── 索引.csv                       # 全部事件汇总 + 已上传状态
├── 上传指引.pdf                   # 每个事件一页,含填表对照信息
├── 浙A12345_IMG-001_14-23-05/
│   ├── 截图.jpg                   # 1080p,带时间戳水印
│   ├── 视频.mp4                   # 6 秒,带时间戳水印
│   └── 信息.txt                   # 车牌/时间/违法类型(可复制)
├── 浙B88888_IMG-001_14-25-12/
└── ...
```

### 3.4 关键参数 (默认值,可调)

| 参数 | 默认值 | 说明 |
|------|--------|------|
| 视频片段时长 | 前 3s + 后 3s = 6s | 警察叔叔要求"过程完整" |
| 抽帧频率 | 1 fps | 违停车辆静止,1fps 性能最优 |
| IoU 阈值 | 0.3 | 实测调整 |
| 车牌置信度阈值 | 60% / 85% | 红/黄/绿三级标记 |
| 事件聚合窗口 | 60 秒 | 同视频同车牌 60s 内视为同一事件 |
| 视频处理分辨率 | 1080p | AI 推理用,证据视频保留原分辨率 |
| 单视频处理时长 | 15-25 秒 | 30 秒 1080p 视频参考值 |

### 3.5 审核 UI 关键交互

- **键盘流**: ← → 切换 / ↑ 采纳 / ↓ 丢弃 / Space 播放
- **三态**: Pending / Accepted / Rejected / Deferred
- **AI 标注框可视化**: 视频上叠加车辆框 + 人行道半透明掩膜
- **车牌可手动修正**: HyperLPR3 偶尔识别 0/O、1/I、8/B 错位
- **跨视频同车牌提示**: 不自动合并,UI 高亮提示

---

## 四、项目结构

```
illegal-parking-reporter/
├── src/                           # Vue 前端
│   ├── views/
│   │   ├── UploadView.vue         # 视频上传
│   │   ├── ProcessingView.vue     # 处理进度
│   │   ├── ReviewView.vue         # 事件审核
│   │   ├── ExportView.vue         # 批量导出
│   │   └── SettingsView.vue       # 设置/模型检查
│   ├── components/
│   │   ├── EventCard.vue
│   │   ├── EventDetailDrawer.vue
│   │   ├── VideoPlayer.vue
│   │   └── KeyboardShortcuts.ts   # composable
│   ├── stores/                    # Pinia
│   │   ├── events.ts
│   │   ├── pipeline.ts
│   │   └── settings.ts
│   ├── api/                       # Tauri invoke 封装
│   │   ├── detection.ts
│   │   ├── pipeline.ts
│   │   └── evidence.ts
│   ├── router/index.ts
│   ├── App.vue
│   └── main.ts
├── src-tauri/                     # Rust 后端
│   ├── src/
│   │   ├── commands/              # Tauri 命令层
│   │   │   ├── detection.rs
│   │   │   ├── pipeline.rs
│   │   │   ├── evidence.rs
│   │   │   ├── system.rs
│   │   │   └── mirror.rs          # P8
│   │   ├── ai/                    # AI 推理
│   │   │   ├── vehicle.rs         # YOLOv8-seg
│   │   │   ├── sidewalk.rs        # SegFormer
│   │   │   ├── plate.rs           # HyperLPR3
│   │   │   └── judge.rs           # IoU 判定
│   │   ├── video/
│   │   │   ├── extract.rs         # 抽帧
│   │   │   ├── clip.rs            # 剪辑
│   │   │   ├── watermark.rs       # 时间戳水印
│   │   │   └── metadata.rs        # 读取 creation_time/旋转
│   │   ├── pipeline/              # 流水线编排
│   │   │   ├── stage.rs
│   │   │   ├── orchestrator.rs
│   │   │   └── progress.rs
│   │   ├── evidence/
│   │   │   ├── builder.rs
│   │   │   ├── exporter.rs
│   │   │   ├── csv_index.rs
│   │   │   └── pdf_guide.rs
│   │   ├── db/
│   │   │   ├── schema.rs
│   │   │   ├── events.rs
│   │   │   └── settings.rs
│   │   ├── models/                # 数据结构
│   │   │   ├── event.rs
│   │   │   ├── config.rs
│   │   │   └── settings.rs
│   │   ├── mirror/                # P8: iPhone Mirroring 助手
│   │   │   ├── clipboard.rs       # P8.1 剪贴板助手
│   │   │   ├── applescript.rs     # P8.2 自动化
│   │   │   └── ocr.rs             # Vision 框架 OCR
│   │   ├── lib.rs
│   │   └── main.rs
│   ├── models/                    # ONNX 模型 (gitignore)
│   ├── capabilities/              # Tauri 权限
│   ├── icons/
│   ├── Cargo.toml
│   ├── build.rs
│   └── tauri.conf.json
├── docs/
│   ├── SETUP.md                   # 环境搭建 (M1)
│   ├── MODELS.md                  # 模型准备
│   ├── ROADMAP.md                 # 路线图
│   └── ARCHITECTURE.md            # 架构详解
├── package.json
├── tsconfig.json
├── vite.config.ts
├── README.md
└── .gitignore
```

---

## 五、分阶段开发路线

### 总览

| 阶段 | 周期 | 性质 | 交付物 | 验收标准 |
|------|------|------|--------|---------|
| P0 | 3 天 | 必做 | 工程脚手架 | 选图片 → YOLO 检测 → 表格展示 |
| P1 | 1 周 | 必做 | 单视频识别 pipeline | 一个视频跑出违停事件列表 |
| P2 | 4 天 | 必做 | 批量流水线 | 10 视频并行,可断点续传 |
| P3 | 4 天 | 必做 | 证据生成 + 人行道判定 | 输出标准证据文件夹 |
| P4 | 1 周 | 必做 | 审核 UI | 30 事件 5 分钟审完 |
| P5 | 2 天 | 必做 | 导出 + 索引 | 一键打包文件夹 |
| P6 | 2 天 | 必做 | 本地登录 + 设置 | 加密存储 + 参数可调 |
| P7 | 3 天 | 必做 | 打磨 | 可发版给朋友试用 |
| P8 | 1-2 周 | 加分 | iPhone Mirroring 助手 | 上传效率 +50% |

**P0-P7 核心工期约 4-5 周**

---

### P0 - 工程脚手架 (3 天)

#### 目标
验证技术链路: Tauri 前后端通信 + Rust 调用 ONNX 推理。

#### 任务清单

- [ ] Tauri 2.x + Vue 3 + TypeScript + Element Plus 项目初始化
- [ ] Rust 后端目录结构、依赖配置 (Cargo.toml)
- [ ] ONNX Runtime 集成 (ort crate, M1 启用 CoreML)
- [ ] YOLOv8 完整推理流水线:
  - [ ] preprocess: Letterbox 缩放 + 归一化 + CHW 转换
  - [ ] inference: ort Session 推理
  - [ ] postprocess: 解析 [1, 84, 8400] 输出 + 坐标还原
  - [ ] NMS (Non-Maximum Suppression)
- [ ] 前端 demo 页面: 选图片 → invoke 后端 → 展示检测结果表格
- [ ] 模型文件路径解析与状态检查 (开发模式 / Release 模式)
- [ ] 日志框架 (tracing + EnvFilter)
- [ ] 文档: README / SETUP.md / MODELS.md / ROADMAP.md

#### 验收
1. 打开"设置"页面,YOLOv8 显示绿色"已就绪"
2. "上传"页面选 JPG → 点"运行检测"
3. 表格列出 car/truck/bus 等结果,带置信度和边界框
4. M1 推理耗时 < 200ms

#### 关键技术决策

- ort 用 `load-dynamic` 模式,onnxruntime 通过 `brew install onnxruntime` 安装
- 环境变量 `ORT_DYLIB_PATH=/opt/homebrew/lib/libonnxruntime.dylib`
- COCO 类别中关心的: car(2) / motorcycle(3) / bus(5) / truck(7)
- 输入尺寸 640×640,置信度阈值 0.25,NMS IoU 0.45

---

### P1 - 单视频识别 pipeline (1 周)

#### 目标
完整跑通"上传一个视频 → 输出违停事件列表"。

#### 任务清单

- [ ] FFmpeg 集成 (`ffmpeg-next` crate)
- [ ] **视频元数据读取**:
  - [ ] creation_time (拍摄时间)
  - [ ] duration (时长)
  - [ ] frame_rate
  - [ ] **旋转标志** (EXIF rotation,iPhone 竖屏视频关键)
- [ ] **抽帧**:
  - [ ] 1 fps 抽帧
  - [ ] **应用 EXIF 旋转** (否则 YOLO 识别横躺画面)
  - [ ] 输出 RGB 帧序列
- [ ] HyperLPR3 集成:
  - [ ] 车牌检测器 (输入帧 → 输出车牌候选区域)
  - [ ] 字符识别器 (输入车牌区域 → 输出文本 + 置信度)
  - [ ] 支持新能源车牌、警牌、教练车牌
- [ ] **事件聚合** (P1 简化版):
  - [ ] 同视频内同车牌 60 秒内合并为一事件
  - [ ] 取车牌识别置信度最高的帧作代表
  - [ ] 记录最早出现到最晚出现的时间窗
- [ ] SQLite 表设计 + 简化持久化 (完整版 P2)
- [ ] Pipeline 接口: `process_video(path) -> Vec<ParkingEvent>`

#### 数据模型

```rust
struct ParkingEvent {
    id: String,                    // UUID
    source_video: String,          // 源视频文件名
    frame_index: u32,              // 关键帧索引
    timestamp: DateTime<Utc>,      // 拍摄时间(从元数据计算)
    plate_number: String,          // 车牌号
    plate_confidence: f32,
    iou_score: f32,                // P3 才有,P1 用占位
    bbox: [f32; 4],                // 车辆框
    snapshot_path: PathBuf,
    clip_path: PathBuf,            // P3 才生成
    review_status: ReviewStatus,
    plate_manual_corrected: Option<String>,
}

enum ReviewStatus { Pending, Accepted, Rejected, Deferred }
```

#### 验收
- 单视频运行,产出违停事件列表
- 车牌识别准确率 > 80% (中国常见车牌)
- 支持横屏 + 竖屏视频
- **P1 阶段判定逻辑可用占位**: 车辆 + 简单位置规则,P3 才升级到人行道判定

#### 风险点
- HyperLPR3 ONNX 在中国车牌的实测精度需要验证
- iPhone 竖屏视频的旋转处理必须在 P1 解决,不能拖延

---

### P2 - 批量流水线 (4 天)

#### 目标
10 个视频并行处理,中途崩溃可恢复,前端实时进度。

#### 任务清单

- [ ] tokio + mpsc + Semaphore 三级流水线:
  - [ ] Stage 1: 抽帧 (concurrency 4)
  - [ ] Stage 2: AI 推理 (concurrency 2,模型常驻)
  - [ ] Stage 3: 事件聚合 (concurrency 4)
- [ ] 进度上报: Tauri Event 实时推送
  - [ ] 每个视频独立进度
  - [ ] 整体进度 (X/10)
  - [ ] 当前阶段标识
  - [ ] 预计剩余时间
- [ ] **断点续传**:
  - [ ] 每完成一个视频立即写 SQLite
  - [ ] 启动时扫描未完成任务
  - [ ] 重新进入 ProcessingView 显示已完成事件
- [ ] ProcessingView UI:
  - [ ] 实时进度条 × N
  - [ ] 整体状态卡片
  - [ ] 错误视频可单独重试

#### 流水线设计

```
视频文件                                                   
   │                                                      
   ▼                                                      
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│  Stage 1:    │    │  Stage 2:    │    │  Stage 3:    │
│  抽帧(CPU)   │ ──▶│  AI 推理     │ ──▶│  事件聚合    │
│              │    │  (CPU/GPU)   │    │              │
│  并发 4      │    │  并发 1-2    │    │  并发 4      │
└──────────────┘    └──────────────┘    └──────────────┘
```

#### 验收
- 10 个 30 秒 1080p 视频在 M1 Mac 上 3-4 分钟完成
- 强制 Kill 进程后重启,看到已完成事件,只重跑未完成视频
- 单视频失败不阻塞其他视频

---

### P3 - 证据生成 + 人行道判定 (4 天)

#### 目标
- 完成"占用人行道"判定逻辑
- 输出符合警察叔叔要求的证据包(截图 + 视频 + 时间戳水印)

#### 任务清单

- [ ] **SegFormer-B0 集成**:
  - [ ] ADE20K 类别中提取 sidewalk
  - [ ] ONNX 推理输出语义分割掩膜
  - [ ] 中国街景精度兜底方案 (备选 Cityscapes 或 SAM)
- [ ] **违停判定升级**:
  - [ ] 车辆掩膜 ∩ 人行道掩膜 IoU 计算
  - [ ] IoU > 阈值 (默认 0.3) 判定违停
  - [ ] P1 占位逻辑替换
- [ ] **视频片段剪辑**:
  - [ ] 从原始视频(原分辨率)剪切前 3 秒 + 后 3 秒
  - [ ] FFmpeg copy 模式或重编码
  - [ ] 处理边界情况 (视频开头/结尾)
- [ ] **时间戳水印**:
  - [ ] FFmpeg drawtext filter 烧录时间戳
  - [ ] 字体: 系统默认 + 可读性
  - [ ] 位置: 右下角 + 半透明背景
  - [ ] 格式: `2026-05-02 14:23:05`
- [ ] **截图生成**:
  - [ ] 高分辨率(原始)
  - [ ] 烧录时间戳水印
  - [ ] 选车牌识别置信度最高的那一帧
- [ ] **证据文件夹结构**:
  - [ ] 命名: `{车牌}_{源视频名}_{时分秒}/`
  - [ ] 内含: 截图.jpg / 视频.mp4 / 信息.txt

#### 关键决策: 双路径处理

为了兼顾 AI 推理速度和证据视频画质:

```
原始视频(4K) ──┬─▶ 降到 1080p ─▶ AI 推理 (快)
              └─▶ 保持原分辨率 ─▶ 剪辑证据 (画质保留)
```

#### 验收
- 输入一个视频,产出符合规范的证据文件夹
- 证据视频可在 QuickTime 正常播放
- 时间戳清晰可见
- 警察叔叔接受的格式自检通过

#### 风险点
- SegFormer 对中国街景的人行道识别精度
- 兜底方案: Mask2Former + Cityscapes / SAM 交互式分割

---

### P4 - 审核 UI (1 周)

#### 目标
30 个事件能在 5 分钟内审完。

#### 任务清单

- [ ] **事件列表表格**:
  - [ ] 缩略图 + 车牌 + 来源视频 + 时间 + 置信度
  - [ ] 按置信度排序 (低置信度优先)
  - [ ] 状态着色: Pending 灰 / Accepted 绿 / Rejected 红
  - [ ] 筛选: 按车牌、按视频、按状态
- [ ] **详情抽屉/分屏**:
  - [ ] 视频播放器 (auto play loop)
  - [ ] AI 标注框可视化 (车辆框 + 人行道半透明掩膜)
  - [ ] 元信息: 车牌 / 时间 / 来源 / 违法类型 / IoU / 置信度
  - [ ] 操作按钮: 采纳 / 丢弃 / 待定
- [ ] **键盘快捷键** (composable):
  - [ ] ← / → 切换事件
  - [ ] ↑ 采纳
  - [ ] ↓ 丢弃
  - [ ] Space 播放/暂停视频
  - [ ] D 标记待定
- [ ] **车牌手动修正**:
  - [ ] 内联编辑
  - [ ] 显示原识别值 + 修正值
  - [ ] 修正后保存到数据库
- [ ] **跨视频同车牌提示**:
  - [ ] UI 高亮: "该车牌在 X 个视频中出现"
  - [ ] 可点击查看其他出现位置
  - [ ] **不自动合并**
- [ ] **车牌识别失败的事件处理**:
  - [ ] 保留事件,标记"车牌待确认"
  - [ ] 强制要求用户手动输入才能采纳
- [ ] **进度统计**: 已审 / 已采纳 / 已丢弃 / 待定

#### 验收
- 30 个事件用键盘 5 分钟审完
- 误操作可撤销 (回到上一个事件)
- 车牌修正生效

---

### P5 - 导出 + 索引 (2 天)

#### 目标
一键导出完整证据包文件夹,含索引和上传指引。

#### 任务清单

- [ ] **批量导出**:
  - [ ] 选中已采纳事件 → 选目标文件夹
  - [ ] 顶层文件夹命名: `违停举报包_YYYY-MM-DD_HH-MM/`
  - [ ] 复制每个事件的子文件夹到目标
- [ ] **索引 CSV** (`索引.csv`):
  - [ ] 字段: 序号 / 车牌号 / 时间 / 来源视频 / 违法类型 / 文件夹路径 / 已上传(Y/N)
  - [ ] UTF-8 BOM (Excel 打开不乱码)
- [ ] **上传指引 PDF** (`上传指引.pdf`):
  - [ ] 每个事件一页
  - [ ] 含: 截图缩略图 + 车牌(大字号) + 时间 + 违法类型
  - [ ] 警察叔叔填表对照说明
  - [ ] 用 `printpdf` 或类似 Rust crate 生成
- [ ] **状态更新**:
  - [ ] 导出后事件状态: Accepted → Exported
  - [ ] 数据库记录导出时间和路径

#### 验收
- 选 10 事件 → 点导出 → 桌面出现规范文件夹
- 索引 CSV 在 Excel/Numbers 正常打开
- PDF 在 macOS Preview 正常显示

---

### P6 - 本地登录 + 设置 (2 天)

#### 目标
首次启动有引导和合规提示,数据加密存储,参数可调。

#### 任务清单

- [ ] **本地账号** (单用户即可):
  - [ ] 首次启动设置密码
  - [ ] argon2 哈希存储
  - [ ] 软件锁定 / 解锁
- [ ] **SQLCipher 加密数据库**:
  - [ ] rusqlite 启用 SQLCipher feature
  - [ ] 主密钥从用户密码派生
  - [ ] 透明加密所有事件数据
- [ ] **设置页面**:
  - [ ] 模型路径配置 (含浏览选择)
  - [ ] IoU 阈值 (滑块)
  - [ ] 视频片段时长 (前/后秒数)
  - [ ] 抽帧频率
  - [ ] 车牌置信度阈值
  - [ ] 事件聚合窗口
- [ ] **首次启动引导** (5 步):
  - [ ] 1. 准备模型文件 (检查或引导下载)
  - [ ] 2. 授权权限 (文件读写)
  - [ ] 3. **阅读使用说明** (合法性提示,必读)
  - [ ] 4. 选择第一个视频测试
  - [ ] 5. 看到识别结果
- [ ] **用户协议**:
  - [ ] 软件仅辅助识别,不保证 100% 准确
  - [ ] 用户对最终提交负责
  - [ ] 误报/虚假举报由用户承担法律责任
  - [ ] 不存储/上传任何数据到第三方
- [ ] **合法性提示** (UI 常驻):
  - [ ] 不得驾驶过程中拍摄
  - [ ] 不得进入机动车道拍摄
  - [ ] 举报时间 ≤ 拍摄后 72 小时
- [ ] **清空数据按钮**:
  - [ ] 确认弹窗
  - [ ] 删除数据库 + 临时文件 + 缓存

#### 验收
- 设置项可调,重启后保持
- 数据库文件用文本工具打开是加密二进制
- 首次启动流程顺畅

---

### P7 - 打磨 (3 天)

#### 目标
软件可发版给身边朋友试用,无明显 bug。

#### 任务清单

- [ ] **错误处理全覆盖**:
  - [ ] 模型加载失败 → 友好提示 + 引导修复
  - [ ] 视频解码失败 → 跳过 + 标记
  - [ ] 磁盘空间不足 → 检测并提示
  - [ ] 权限缺失 → 引导授权
- [ ] **日志导出**:
  - [ ] "导出诊断包"按钮
  - [ ] 含: 日志文件 + 系统信息 + 失败事件元数据
  - [ ] 隐私脱敏 (车牌/时间地点替换)
- [ ] **性能优化**:
  - [ ] 启动速度 < 3 秒
  - [ ] 内存峰值 < 2GB (10 视频批处理)
  - [ ] 模型预热 (避免首次推理慢)
- [ ] **UI 细节**:
  - [ ] 加载态动画
  - [ ] 空状态提示 (无视频/无事件)
  - [ ] 关键操作 toast 反馈
  - [ ] 暗色模式支持 (可选)
- [ ] **多分辨率视频测试**:
  - [ ] 4K 横屏
  - [ ] 1080p 横屏
  - [ ] 1080p 竖屏
  - [ ] iPhone 慢动作 (高帧率)
- [ ] **图标与品牌**:
  - [ ] 应用图标 (1024x1024 PNG)
  - [ ] 关于页面
  - [ ] 软件名称: **路况记录助手** (中性,避免"举报"敏感词)

#### 验收
- 给朋友安装试用,半小时内无障碍上手
- 处理 10 个视频全程无崩溃

---

### P8 - iPhone Mirroring 助手 (1-2 周, 加分项)

#### P8.1 剪贴板助手 (优先做)

**目标**: 你在 iPhone Mirroring 中手动操作警察叔叔,软件提供信息卡片 + 剪贴板支持。

- [ ] 审核界面新增"开始上传"按钮
- [ ] 上传向导窗口 (浮窗,始终置顶):
  - [ ] 当前事件信息: 车牌(大字号) / 时间 / 违法类型
  - [ ] [复制车牌] 按钮 → 写入剪贴板
  - [ ] [复制时间] 按钮
  - [ ] [复制全部] 按钮
  - [ ] [上一个] [下一个] 按钮
  - [ ] [标记已上传] 按钮
- [ ] 你切到镜像窗口手动粘贴提交
- [ ] 完成后软件标记事件为 Uploaded

**判定**: 上传效率比纯手工提升 50%+

#### P8.2 AppleScript 半自动 (后置,谨慎)

**目标**: 软件半自动驱动 iPhone Mirroring,关键步骤(车牌输入、最终提交)仍由用户。

⚠️ **风险提示**:
- iPhone Mirroring UI 大量元素对 System Events 不可见
- 警察叔叔大概率有反自动化检测
- 实名账号被封风险

**实现策略 (人机协作状态机)**:

```
软件做                        你做
─────────────────            ─────────────────
1. 调起 iPhone Mirroring         
2. 等你确认连接成功     ───▶  点"开始"
3. AppleScript 启动警察叔叔
4. AppleScript 导航到违法举报
5. 弹出信息卡片            ───▶  对照填表(你来)
6. 等你确认               ───▶  点"已填完"
7. screencapture + Vision OCR 检测"提交成功"
8. 标记已上传,进入下一个
```

- [ ] AppleScript runner (Rust 调 osascript)
- [ ] 截屏镜像窗口 (screencapture)
- [ ] Vision 框架 OCR (写独立 Swift binary, Rust 通过 stdin/stdout 通信)
- [ ] 状态机: 每步等用户确认才进下一步
- [ ] 失败降级: 任何步骤失败 → 弹"是否切换为完全手动?"
- [ ] 反自动化对策:
  - [ ] 操作间随机延迟 (800-1500ms)
  - [ ] 模拟人类鼠标轨迹
  - [ ] 最终"提交"始终由用户点
  - [ ] 频率控制 (每事件 ≥ 30 秒间隔)

**权限申请**:
- 屏幕录制 (截屏镜像窗口)
- 辅助功能 Accessibility (AppleScript 模拟点击)
- 自动化权限 (控制 iPhone Mirroring)

#### 验收
- P8.1 必达: 剪贴板助手稳定可用
- P8.2 选达: 跑得通就用,跑不通就降级 P8.1

---

## 六、关键参数与默认值

### 6.1 AI 参数

| 参数 | 默认 | 范围 | 备注 |
|------|------|------|------|
| YOLOv8 输入尺寸 | 640 | 固定 | |
| YOLOv8 置信度阈值 | 0.25 | 0.1-0.5 | |
| YOLOv8 NMS IoU | 0.45 | 固定 | |
| 车辆 ∩ 人行道 IoU 阈值 | 0.3 | 0.1-0.7 | 用户可调 |
| 车牌置信度阈值 | 0.6 | 0.4-0.9 | < 0.6 红, 0.6-0.85 黄, > 0.85 绿 |

### 6.2 视频参数

| 参数 | 默认 | 备注 |
|------|------|------|
| 抽帧频率 | 1 fps | |
| AI 推理分辨率 | 1080p | 4K 自动降采样 |
| 证据视频分辨率 | 原始 | 不降画质 |
| 证据视频时长 | 6s (前 3 + 后 3) | 用户可调 |
| 证据截图分辨率 | 原始 | |

### 6.3 聚合参数

| 参数 | 默认 | 备注 |
|------|------|------|
| 同视频同车牌窗口 | 60 秒 | 60s 内视为同一事件 |
| 跨视频同车牌 | 不合并,UI 提示 | |

### 6.4 性能预期 (M1 Mac)

| 操作 | 耗时 |
|------|------|
| 单帧 YOLOv8 推理 | < 200 ms |
| 单帧 SegFormer 推理 | 300-500 ms |
| 单帧 HyperLPR3 识别 | 50-100 ms |
| 30s 1080p 视频完整处理 | 15-25 秒 |
| 10 视频批处理 | 3-4 分钟 |

---

## 七、需要提前确认的事项 (开发前必读)

### 7.1 模型许可证

✅ 已确认: 走开源路线,接受 AGPL-3.0 (YOLOv8 传染),全开源分发。

### 7.2 举报渠道

✅ 已确认: 杭州只能用警察叔叔 / 支付宝城市服务 / 交通拍客,均为移动端。
软件不直接对接任何举报平台,只生成证据包。

### 7.3 用户协议必备条款

- 不保证 100% 识别准确
- 用户对最终提交负责
- 误报/诬告法律责任由用户承担
- 数据完全本地化处理
- 软件可能与平台后续政策不一致,以平台规则为准

### 7.4 软件命名

✅ 推荐: **路况记录助手** / **出行证据管理**
❌ 避免: "违停神器" / "一键举报" 类敏感词

### 7.5 模型分发策略

✅ 已确认: 打包进应用 (~150MB),开箱即用,避免下载失败售后。

---

## 八、开发环境要求

### 8.1 系统

- macOS 12+ (Monterey)
- Apple Silicon (M1/M2/M3) 优先,Intel Mac 备选
- 至少 8GB RAM,16GB 推荐
- 至少 5GB 磁盘 (含工具链 + 模型)

### 8.2 工具链版本

| 工具 | 版本 | 安装方式 |
|------|------|---------|
| Homebrew | 最新 | `/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"` |
| Rust | ≥ 1.77 | `rustup-init` |
| Node.js | ≥ 18 LTS | `brew install node` |
| ONNX Runtime | 最新 | `brew install onnxruntime` |
| FFmpeg | ≥ 7 | `brew install ffmpeg pkg-config` |
| Python 3 | ≥ 3.10 | (用于模型转换) |

### 8.3 关键环境变量

```bash
# 让 ort 找到 onnxruntime
export ORT_DYLIB_PATH=/opt/homebrew/lib/libonnxruntime.dylib

# 加到 ~/.zshrc 长期生效
echo 'export ORT_DYLIB_PATH=/opt/homebrew/lib/libonnxruntime.dylib' >> ~/.zshrc
```

### 8.4 模型文件准备

```bash
# YOLOv8n.onnx (P0 必需, ~12MB)
pip3 install ultralytics
python3 -c "from ultralytics import YOLO; YOLO('yolov8n.pt').export(format='onnx', imgsz=640, opset=12)"
mv yolov8n.onnx src-tauri/models/

# YOLOv8n-seg.onnx (P3 升级, 含掩膜)
python3 -c "from ultralytics import YOLO; YOLO('yolov8n-seg.pt').export(format='onnx', imgsz=640, opset=12)"
mv yolov8n-seg.onnx src-tauri/models/

# HyperLPR3 (P1 必需)
git clone https://github.com/szad670401/HyperLPR.git /tmp/HyperLPR
mkdir -p src-tauri/models/hyperlpr3
# 复制对应 ONNX 文件 (具体文件名以仓库为准)

# SegFormer-B0 (P3 必需, ~14MB)
pip3 install transformers optimum[exporters] torch
optimum-cli export onnx \
  --model nvidia/segformer-b0-finetuned-ade-512-512 \
  --task semantic-segmentation \
  src-tauri/models/segformer/
```

---

## 九、风险点清单

| 风险 | 阶段 | 影响 | 缓解 |
|------|------|------|------|
| ort + onnxruntime M1 链接配置 | P0 | 高 | 已有方案,环境变量 |
| iPhone 竖屏视频旋转处理 | P1 | 高 | 抽帧时显式应用 EXIF rotation |
| HyperLPR3 中国车牌精度 | P1 | 中 | 实测 + 备选 PaddleOCR |
| SegFormer 中国街景人行道精度 | P3 | 中 | 备选 Mask2Former / SAM |
| 大视频内存占用 | P2/P3 | 中 | 分块处理,流式抽帧 |
| 警察叔叔反自动化 | P8 | 高 | P8.2 不强求,P8.1 是兜底 |
| iPhone Mirroring 国区可用性 | P8 | 中 | 用户已确认可用,不强求 |

---

## 十、版本规划

### V0.1 - MVP (P0-P5)
- 核心识别 + 审核 + 导出
- **不含**: 加密、设置、引导
- **目标用户**: 自己 + 1-2 个早期测试者

### V0.5 - Beta (P0-P7)
- 完整必做功能,可发版给朋友
- 含合规提示、加密存储、错误处理
- **目标用户**: 5-10 个杭州朋友

### V1.0 - Release (P0-P8)
- 含 iPhone Mirroring 助手
- 完整文档、引导、安装包
- **目标用户**: 公开发布 (GitHub 开源)

### V2.0 - 扩展 (未来)
- 多城市举报模板 (温州永嘉浙里办、其他城市)
- 更多违法类型 (压实线、占用应急车道)
- 协作功能 (多账号、云同步,可选)

---

## 十一、开发执行建议

### 11.1 单人开发节奏

- **每天专注 1 个阶段任务**,避免跨阶段切换
- **每个阶段都有真实视频验证**,不是单元测试通过就算完
- **遇到卡点优先看现有开源项目实现**,不要自己造轮子
- **做完每个阶段就 git tag 一次**,方便回滚

### 11.2 阶段切换原则

每个阶段必须满足验收标准才进下一阶段。
**不要带着技术债往后走**,P0 链路不通就别开始 P1。

### 11.3 调试技巧

- Rust 后端日志: `RUST_LOG=debug npm run tauri:dev`
- 前端开发者工具: Tauri 窗口右键 → Inspect Element
- ONNX 模型可视化: 用 [Netron](https://netron.app/) 查看模型结构
- 视频元数据查看: `ffprobe -v quiet -print_format json -show_format -show_streams <video>`

### 11.4 与 AI 助手协作

将本文档放在项目根目录,与 AI 助手 (Claude Code / Cursor) 协作时:
- 每次任务先告知"当前阶段是 PX,任务是 XX"
- 让 AI 阅读 docs/ROADMAP.md 后再动手
- 验收前让 AI 自检对照本文档的"验收标准"

---

## 十二、附录

### A. 参考资料

- Tauri 2.x 文档: https://tauri.app/
- ort crate: https://github.com/pykeio/ort
- ultralytics YOLO: https://github.com/ultralytics/ultralytics
- HyperLPR3: https://github.com/szad670401/HyperLPR
- SegFormer: https://huggingface.co/nvidia/segformer-b0-finetuned-ade-512-512

### B. 已调研的杭州举报渠道

- 警察叔叔 App (杭州市公安局,移动端)
- 支付宝"城市服务" → 车主服务 → 交通违法有奖举报 (移动端)
- 交通拍客 App (浙江省公安厅交管局,移动端)
- 三个平台数据互通,选其一即可
- 110 / 12345 (电话)
- 现场提交 (各属地交警大队)
- **无 PC 网页端入口**

### C. 已 review 的工程盲点 (全部已纳入计划)

1. ✅ 跨视频同车牌处理 (不合并,UI 提示)
2. ✅ 车牌识别失败的事件 (保留 + 手动输入)
3. ✅ 同车牌不同帧识别不一致 (取最高置信度)
4. ✅ 视频原始大小 (双路径: AI 用 1080p, 证据用原始)
5. ✅ 视频朝向旋转 (P1 必处理)
6. ✅ macOS 权限申请 (文件 / 屏幕录制 / 辅助功能)
7. ✅ 模型文件分发 (打包进应用)
8. ✅ 错误恢复 (断点续传)
9. ✅ 日志与诊断 (P7 导出诊断包)
10. ✅ 软件免责声明 (P6 用户协议)
11. ✅ 个人信息保护 (SQLCipher 加密 + 不联网)
12. ✅ 拍摄合法性提示 (UI 常驻)
13. ✅ 软件名称中性化 (路况记录助手)
14. ✅ 首次启动引导 (P6 五步)
15. ✅ 性能预期管理 (实时进度 + 阶段标识)
16. ✅ 已上传状态追踪 (CSV + UI 三态)
17. ✅ 撤销与回收站 (软删除 30 天)
18. ✅ iPhone Mirroring 务实方案 (P8.1 剪贴板优先)
19. ✅ 多城市模板 (V2 扩展)
20. ✅ 多人协作 (DB schema 留 user_id 扩展位)

---

## 文档版本

- v1.0 - 2026-05-02 - 初稿,基于完整需求讨论
- 作者: Claude (Anthropic) 与 Zhou Keke 协作完成