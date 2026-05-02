<script setup lang="ts">
import { ref, computed } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { ElMessage } from "element-plus";

import { detectDemo, type DetectionResult } from "@/api/detection";

const imagePath = ref<string | null>(null);
const detecting = ref(false);
const result = ref<DetectionResult | null>(null);
const errorMsg = ref<string | null>(null);

async function pickImage() {
  errorMsg.value = null;
  const selected = await open({
    multiple: false,
    directory: false,
    filters: [
      {
        name: "图片",
        extensions: ["jpg", "jpeg", "png", "webp"],
      },
    ],
  });
  if (typeof selected === "string") {
    imagePath.value = selected;
    result.value = null;
  }
}

async function runDetect() {
  if (!imagePath.value) {
    ElMessage.warning("请先选择一张图片");
    return;
  }
  detecting.value = true;
  errorMsg.value = null;
  result.value = null;
  try {
    result.value = await detectDemo(imagePath.value);
    ElMessage.success(`检测完成, 共 ${result.value.detections.length} 个目标`);
  } catch (e) {
    errorMsg.value = String(e);
  } finally {
    detecting.value = false;
  }
}

const fileName = computed(() => {
  if (!imagePath.value) return "";
  const parts = imagePath.value.split(/[/\\]/);
  return parts[parts.length - 1] || imagePath.value;
});

function formatBbox(b: [number, number, number, number]): string {
  return `[${b[0].toFixed(0)}, ${b[1].toFixed(0)}, ${b[2].toFixed(0)}, ${b[3].toFixed(0)}]`;
}
</script>

<template>
  <div class="upload">
    <el-card>
      <template #header>
        <h2>图片检测 (P0 demo)</h2>
        <p class="subtitle">选一张含车辆的 JPG/PNG, 调用 YOLOv8 输出检测结果</p>
      </template>

      <el-space wrap>
        <el-button type="primary" @click="pickImage">选择图片...</el-button>
        <el-button
          type="success"
          :disabled="!imagePath"
          :loading="detecting"
          @click="runDetect"
        >
          运行检测
        </el-button>
      </el-space>

      <div v-if="imagePath" class="file-info">
        <el-tag type="info">已选: {{ fileName }}</el-tag>
        <span class="full-path">{{ imagePath }}</span>
      </div>

      <el-alert
        v-if="errorMsg"
        class="alert"
        type="error"
        :closable="false"
        show-icon
        :title="`检测失败`"
        :description="errorMsg"
      />

      <template v-if="result">
        <el-divider />
        <el-descriptions :column="3" border>
          <el-descriptions-item label="原图尺寸">
            {{ result.image_width }} × {{ result.image_height }}
          </el-descriptions-item>
          <el-descriptions-item label="推理耗时">
            <el-tag :type="result.inference_ms < 200 ? 'success' : 'warning'">
              {{ result.inference_ms }} ms
            </el-tag>
          </el-descriptions-item>
          <el-descriptions-item label="检测目标数">
            {{ result.detections.length }}
          </el-descriptions-item>
        </el-descriptions>

        <el-table :data="result.detections" stripe class="results">
          <el-table-column type="index" label="#" width="50" />
          <el-table-column prop="class_name" label="类别" width="180" />
          <el-table-column prop="class_id" label="类别 ID" width="80" />
          <el-table-column label="置信度" width="120">
            <template #default="{ row }">
              <el-progress
                :percentage="Math.round(row.score * 100)"
                :status="row.score > 0.6 ? 'success' : 'warning'"
              />
            </template>
          </el-table-column>
          <el-table-column label="边界框 [x1, y1, x2, y2]">
            <template #default="{ row }">
              <code>{{ formatBbox(row.bbox) }}</code>
            </template>
          </el-table-column>
        </el-table>
      </template>
    </el-card>
  </div>
</template>

<style scoped>
.upload {
  max-width: 1100px;
}

h2 {
  margin: 0;
  font-size: 18px;
}

.subtitle {
  margin: 6px 0 0;
  color: var(--el-text-color-secondary);
  font-size: 13px;
}

.file-info {
  margin-top: 16px;
  display: flex;
  align-items: center;
  gap: 12px;
}

.full-path {
  color: var(--el-text-color-secondary);
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
}

.alert {
  margin-top: 16px;
}

.results {
  margin-top: 16px;
}

code {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
}
</style>
