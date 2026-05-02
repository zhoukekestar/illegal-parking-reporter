<script setup lang="ts">
import { onMounted, computed } from "vue";

import { useSettingsStore } from "@/stores/settings";

const store = useSettingsStore();

onMounted(() => {
  store.refresh();
});

const ortReady = computed(() => !!store.status?.ort_dylib_path);

function formatBytes(n: number | null): string {
  if (!n) return "—";
  const mb = n / 1024 / 1024;
  if (mb >= 1) return `${mb.toFixed(2)} MB`;
  return `${(n / 1024).toFixed(1)} KB`;
}
</script>

<template>
  <div class="settings">
    <el-card>
      <template #header>
        <div class="card-header">
          <h2>设置 / 模型状态</h2>
          <el-button :loading="store.loading" @click="store.refresh()">刷新</el-button>
        </div>
      </template>

      <el-alert
        v-if="store.error"
        type="error"
        :closable="false"
        :title="`检查失败: ${store.error}`"
        show-icon
      />

      <el-skeleton v-if="store.loading" :rows="4" animated />

      <template v-else-if="store.status">
        <el-divider content-position="left">运行时</el-divider>
        <el-descriptions :column="1" border>
          <el-descriptions-item label="软件版本">
            {{ store.status.app_version }}
          </el-descriptions-item>
          <el-descriptions-item label="ONNX Runtime">
            <el-tag v-if="ortReady" type="success">已就绪</el-tag>
            <el-tag v-else type="danger">未就绪</el-tag>
            <span class="path">{{ store.status.ort_dylib_path ?? "ORT_DYLIB_PATH 未设置" }}</span>
          </el-descriptions-item>
        </el-descriptions>

        <el-divider content-position="left">模型文件</el-divider>
        <el-table :data="store.status.models" stripe>
          <el-table-column prop="name" label="名称" width="120" />
          <el-table-column label="状态" width="120">
            <template #default="{ row }">
              <el-tag v-if="row.ready" type="success">已就绪</el-tag>
              <el-tag v-else type="danger">未就绪</el-tag>
            </template>
          </el-table-column>
          <el-table-column label="大小" width="120">
            <template #default="{ row }">
              {{ formatBytes(row.size_bytes) }}
            </template>
          </el-table-column>
          <el-table-column prop="path" label="路径" />
          <el-table-column prop="error" label="错误" />
        </el-table>

        <el-alert
          v-if="store.status.models.some((m) => !m.ready)"
          class="hint"
          type="warning"
          :closable="false"
          show-icon
          title="部分模型未就绪"
          description="请按 docs/MODELS.md 准备模型文件后, 点击右上角刷新"
        />
      </template>
    </el-card>
  </div>
</template>

<style scoped>
.settings {
  max-width: 1000px;
}

.card-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

h2 {
  margin: 0;
  font-size: 18px;
}

.path {
  margin-left: 12px;
  color: var(--el-text-color-secondary);
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
}

.hint {
  margin-top: 16px;
}
</style>
