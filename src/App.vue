<script setup lang="ts">
import { ref } from "vue";
import { getVersion } from "@tauri-apps/api/app";

// MVU 1: 仅验证 Tauri 前后端通信链路
// 真正的 UI (Upload/Processing/Review/Export/Settings) 在 MVU 8 加入
const tauriVersion = ref<string>("加载中...");

getVersion()
  .then((v) => {
    tauriVersion.value = v;
  })
  .catch((err) => {
    tauriVersion.value = `错误: ${err}`;
  });
</script>

<template>
  <div class="container">
    <el-card>
      <template #header>
        <h1>路况记录助手</h1>
        <p class="subtitle">P0 工程脚手架验证</p>
      </template>
      <el-descriptions :column="1" border>
        <el-descriptions-item label="Tauri App 版本">
          {{ tauriVersion }}
        </el-descriptions-item>
        <el-descriptions-item label="当前阶段">
          P0 - 工程脚手架 / MVU 1 项目初始化
        </el-descriptions-item>
        <el-descriptions-item label="下一步">
          MVU 2 开始引入 ort/ffmpeg-next 等完整依赖
        </el-descriptions-item>
      </el-descriptions>
    </el-card>
  </div>
</template>

<style scoped>
.container {
  max-width: 800px;
  margin: 40px auto;
  padding: 0 20px;
}

h1 {
  margin: 0;
  font-size: 24px;
}

.subtitle {
  margin: 8px 0 0;
  color: var(--el-text-color-secondary);
  font-size: 14px;
}
</style>
