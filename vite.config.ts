import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { fileURLToPath, URL } from "node:url";

// Tauri 期望前端 dev server 监听 1420 端口 (固定, 不能换)
// 详见 https://tauri.app/start/frontend/vite/
const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [vue()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  // Tauri 不允许 vite 在调试时清屏, 否则看不到 Rust 编译错误
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 不要监听 Rust 后端文件 (Tauri CLI 自己处理)
      ignored: ["**/src-tauri/**"],
    },
  },
}));
