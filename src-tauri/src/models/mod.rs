// 跨模块共享数据结构
// P0: detection (YOLOv8 单图输出)
// P1: event (ParkingEvent) + observation (Frame/Vehicle/Plate)
// P3+: config, settings

pub mod detection;
pub mod event;
pub mod job;
pub mod observation;
