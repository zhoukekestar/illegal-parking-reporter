<script setup lang="ts">
import { useRouter } from "vue-router";

const router = useRouter();
</script>

<template>
  <div class="home">
    <el-card>
      <template #header>
        <h2>路况记录助手</h2>
        <p class="subtitle">桌面端违停证据包生成器 (本地处理, 不联网)</p>
      </template>

      <el-alert
        type="info"
        :closable="false"
        title="当前阶段: P0 - 工程脚手架"
        description="本阶段只验证 Tauri 前后端通信 + Rust 调用 ONNX 推理。完整的视频识别、审核、导出功能在 P1-P7 实现。"
        show-icon
      />

      <el-divider>P0 验收四条</el-divider>

      <el-descriptions :column="1" border>
        <el-descriptions-item label="① 模型就绪">
          打开「设置」, YOLOv8 显示绿色已就绪
        </el-descriptions-item>
        <el-descriptions-item label="② 选 JPG">
          打开「图片检测」, 选一张含车辆的 JPG
        </el-descriptions-item>
        <el-descriptions-item label="③ 检测结果">
          表格列出 car/truck/bus 等结果, 含置信度和边界框
        </el-descriptions-item>
        <el-descriptions-item label="④ 性能">
          M1 推理耗时 &lt; 200ms (页面顶部会显示)
        </el-descriptions-item>
      </el-descriptions>

      <div class="actions">
        <el-button type="primary" @click="router.push('/settings')">检查模型状态</el-button>
        <el-button type="success" @click="router.push('/detect')">开始图片检测</el-button>
      </div>
    </el-card>
  </div>
</template>

<style scoped>
.home {
  max-width: 800px;
}

h2 {
  margin: 0;
  font-size: 20px;
}

.subtitle {
  margin: 6px 0 0;
  color: var(--el-text-color-secondary);
  font-size: 13px;
}

.actions {
  margin-top: 24px;
  display: flex;
  gap: 12px;
}
</style>
