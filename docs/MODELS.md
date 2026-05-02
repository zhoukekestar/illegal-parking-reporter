# 模型文件准备

模型文件不入库 (`.onnx` 已 ignore), 需自行下载或转换, 放入 `src-tauri/models/`。

## P0 - YOLOv8n (车辆检测)

体积 ~12 MB, 用 ultralytics 官方工具导出:

```bash
pip3 install ultralytics
python3 -c "from ultralytics import YOLO; YOLO('yolov8n.pt').export(format='onnx', imgsz=640, opset=12)"
mv yolov8n.onnx src-tauri/models/
```

第一次跑会从 GitHub 下载 yolov8n.pt (~6 MB), 之后 export 到 yolov8n.onnx。

**验证**:
```bash
ls -la src-tauri/models/yolov8n.onnx   # 应有 ~12 MB
```
然后启动应用, 进入「设置」, YOLOv8 显示绿色「已就绪」。

## P3 - YOLOv8n-seg (车辆实例分割)

P3 起需要带掩膜版本:

```bash
python3 -c "from ultralytics import YOLO; YOLO('yolov8n-seg.pt').export(format='onnx', imgsz=640, opset=12)"
mv yolov8n-seg.onnx src-tauri/models/
```

## P1 - HyperLPR3 (车牌识别)

```bash
git clone https://github.com/szad670401/HyperLPR.git /tmp/HyperLPR
mkdir -p src-tauri/models/hyperlpr3
# 具体文件名以仓库 release 资产为准, 通常是:
#   models/onnx/y5fu_320x_sim.onnx        # 检测
#   models/onnx/rpv3_mdict_160h.onnx     # 识别
cp /tmp/HyperLPR/path/to/*.onnx src-tauri/models/hyperlpr3/
```

## P3 - SegFormer-B0 (人行道分割)

体积 ~14 MB:

```bash
pip3 install transformers optimum[exporters] torch
optimum-cli export onnx \
  --model nvidia/segformer-b0-finetuned-ade-512-512 \
  --task semantic-segmentation \
  src-tauri/models/segformer/
```

## 路径规则

- 开发模式 (`npm run tauri:dev`): 模型从 `src-tauri/models/` 加载
- Release bundle: 模型从 `*.app/Contents/Resources/models/` 加载
- 临时覆盖: 设置环境变量 `IPR_MODELS_DIR=/some/path` 指向任意目录

源码定义在 `src-tauri/src/ai/model_path.rs`。

## 许可证提示

- YOLOv8 系列权重 → AGPL-3.0 (本项目最终许可证也为 AGPL-3.0)
- HyperLPR3 → Apache-2.0 (兼容)
- SegFormer-B0 → NVIDIA Source Code License (学术/开源友好)
