// AI 推理领域模块
// P0: vehicle (YOLOv8) + model_path
// P1+: plate (HyperLPR3)
// P3+: sidewalk (SegFormer) + judge (IoU)

pub mod judge;
pub mod model_path;
pub mod plate;
pub mod sidewalk;
pub mod vehicle;
