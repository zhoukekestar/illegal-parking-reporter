# 开发环境搭建 (macOS M1)

## 一、系统要求

- macOS 12+ (Monterey 或更高)
- Apple Silicon (M1/M2/M3) 优先
- 至少 8GB RAM, 推荐 16GB
- 至少 5GB 磁盘 (含工具链 + 模型)

## 二、工具链安装

```bash
# Homebrew
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# Rust (>= 1.77)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
source "$HOME/.cargo/env"

# Node.js (>= 18 LTS)
brew install node

# ONNX Runtime (P0 起必需)
brew install onnxruntime

# FFmpeg (P1 起必需)
brew install ffmpeg pkg-config

# Python (用于模型转换)
# 系统自带 python3 即可
```

## 三、关键环境变量

ort crate 用 `load-dynamic` 模式, 需要在运行时找到 onnxruntime 动态库:

```bash
echo 'export ORT_DYLIB_PATH=/opt/homebrew/lib/libonnxruntime.dylib' >> ~/.zshrc
source ~/.zshrc
```

验证: `echo $ORT_DYLIB_PATH` 应输出上面的路径。

## 四、项目首次运行

```bash
cd illegal-parking-reporter

# 安装前端依赖
npm install

# 准备模型文件 (P0 只需 yolov8n.onnx, 见 docs/MODELS.md)

# 启动开发模式
npm run tauri:dev
```

**首次启动**会编译 Rust 后端, 视机器性能 1-3 分钟。后续增量编译只要 1-3 秒。

## 五、调试技巧

```bash
# Rust 后端详细日志
RUST_LOG=debug npm run tauri:dev

# 前端开发者工具: Tauri 窗口右键 → Inspect Element

# 单独跑 Rust 单元测试
cd src-tauri && cargo test --lib

# 单独跑前端类型检查
npm run build
```

## 六、常见问题

### `cargo: command not found`

新终端没 source `~/.cargo/env`, 解决:
```bash
source "$HOME/.cargo/env"
# 或重启终端
```

### `failed to load onnxruntime`

`ORT_DYLIB_PATH` 没设置或路径错误。Apple Silicon 的 Homebrew 装在 `/opt/homebrew`, Intel Mac 在 `/usr/local`:
```bash
ls /opt/homebrew/lib/libonnxruntime.dylib   # M 系列
ls /usr/local/lib/libonnxruntime.dylib      # Intel
```

### Tauri 窗口启动后白屏

Vite dev server 没起来。检查 1420 端口:
```bash
lsof -ti:1420
```
被占用就 kill 后重启。

### `error: linker 'cc' not found`

macOS Xcode CLT 没装:
```bash
xcode-select --install
```
