// P1 单视频识别流水线
//
// 当前形态 (P1 简化版):
//   抽帧 -> YOLOv8 车辆检测 -> [MVU 5: HyperLPR3 车牌] -> [MVU 6: 60s 聚合]
//
// 后续演进:
//   - P2: 改为 tokio 三阶段并发 (抽帧 / AI / 聚合) + 进度回调
//   - P3: 加入 SegFormer 人行道判定, IoU 计算

pub mod aggregate;
pub mod orchestrator;
pub mod parallel;
