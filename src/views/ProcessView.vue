<script setup lang="ts">
import { ref, computed } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { ElMessage } from "element-plus";

import {
  readVideoMetadata,
  processVideo,
  type VideoMetadata,
  type ProcessOutcome,
  type ParkingEvent,
} from "@/api/video";
import { openInFileManager } from "@/api/system";

const videoPath = ref<string | null>(null);
const metaLoading = ref(false);
const metadata = ref<VideoMetadata | null>(null);
const processing = ref(false);
const outcome = ref<ProcessOutcome | null>(null);
const errMsg = ref<string | null>(null);

async function pickVideo() {
  errMsg.value = null;
  const selected = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "视频", extensions: ["mp4", "mov", "m4v"] }],
  });
  if (typeof selected === "string") {
    videoPath.value = selected;
    metadata.value = null;
    outcome.value = null;
    await loadMetadata();
  }
}

async function loadMetadata() {
  if (!videoPath.value) return;
  metaLoading.value = true;
  try {
    metadata.value = await readVideoMetadata(videoPath.value);
  } catch (e) {
    errMsg.value = String(e);
  } finally {
    metaLoading.value = false;
  }
}

async function runProcess() {
  if (!videoPath.value) {
    ElMessage.warning("请先选择一个视频");
    return;
  }
  processing.value = true;
  errMsg.value = null;
  outcome.value = null;
  try {
    outcome.value = await processVideo(videoPath.value);
    ElMessage.success(`处理完成: ${outcome.value.events.length} 个候选事件`);
  } catch (e) {
    errMsg.value = String(e);
  } finally {
    processing.value = false;
  }
}

const fileName = computed(() => {
  if (!videoPath.value) return "";
  const parts = videoPath.value.split(/[/\\]/);
  return parts[parts.length - 1] || videoPath.value;
});

function formatBytes(n: number): string {
  if (n > 1e9) return `${(n / 1e9).toFixed(2)} GB`;
  if (n > 1e6) return `${(n / 1e6).toFixed(2)} MB`;
  if (n > 1e3) return `${(n / 1e3).toFixed(1)} KB`;
  return `${n} B`;
}

function formatSec(s: number): string {
  const m = Math.floor(s / 60);
  const r = (s - m * 60).toFixed(1);
  return `${m}:${r.padStart(4, "0")}`;
}

function formatTimestamp(ms: number): string {
  const s = ms / 1000;
  return `${s.toFixed(1)}s`;
}

function evidenceFolder(e: ParkingEvent): string | null {
  const p = e.snapshot_path ?? e.clip_path;
  if (!p) return null;
  const idx = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
  if (idx <= 0) return null;
  return p.substring(0, idx);
}

async function openEvidence(e: ParkingEvent) {
  const f = evidenceFolder(e);
  if (!f) {
    ElMessage.warning("该事件未生成证据包");
    return;
  }
  try {
    await openInFileManager(f);
  } catch (err) {
    ElMessage.error(String(err));
  }
}
</script>

<template>
  <div class="process">
    <el-card>
      <template #header>
        <h2>视频处理 (P1 demo)</h2>
        <p class="subtitle">选择 mp4/mov → 读元数据 → 运行 pipeline (抽帧 → 车辆 → 车牌 → 60s 聚合)</p>
      </template>

      <el-space wrap>
        <el-button type="primary" @click="pickVideo">选择视频...</el-button>
        <el-button
          type="success"
          :disabled="!videoPath"
          :loading="processing"
          @click="runProcess"
        >
          运行 Pipeline
        </el-button>
      </el-space>

      <div v-if="videoPath" class="file-info">
        <el-tag type="info">已选: {{ fileName }}</el-tag>
        <span class="full-path">{{ videoPath }}</span>
      </div>

      <el-alert
        v-if="errMsg"
        class="alert"
        type="error"
        :closable="false"
        show-icon
        title="出错"
        :description="errMsg"
      />

      <template v-if="metadata">
        <el-divider content-position="left">视频元数据</el-divider>
        <el-skeleton v-if="metaLoading" :rows="4" animated />
        <el-descriptions v-else :column="3" border>
          <el-descriptions-item label="时长">
            {{ formatSec(metadata.duration_seconds) }}
          </el-descriptions-item>
          <el-descriptions-item label="帧率">
            {{ metadata.frame_rate.toFixed(2) }} fps
          </el-descriptions-item>
          <el-descriptions-item label="编码">
            {{ metadata.codec_name }}
          </el-descriptions-item>
          <el-descriptions-item label="原始分辨率">
            {{ metadata.width }} × {{ metadata.height }}
          </el-descriptions-item>
          <el-descriptions-item label="显示分辨率">
            <el-tag type="success">
              {{ metadata.display_width }} × {{ metadata.display_height }}
            </el-tag>
          </el-descriptions-item>
          <el-descriptions-item label="旋转 (顺时针)">
            <el-tag :type="metadata.rotation_degrees === 0 ? 'info' : 'warning'">
              {{ metadata.rotation_degrees }}°
            </el-tag>
          </el-descriptions-item>
          <el-descriptions-item label="拍摄时间">
            {{ metadata.creation_time ?? "—" }}
          </el-descriptions-item>
          <el-descriptions-item label="文件大小">
            {{ formatBytes(metadata.file_size_bytes) }}
          </el-descriptions-item>
        </el-descriptions>
      </template>

      <template v-if="outcome">
        <el-divider content-position="left">事件列表 ({{ outcome.events.length }} 个)</el-divider>
        <el-table v-if="outcome.events.length" :data="outcome.events" stripe>
          <el-table-column type="index" label="#" width="50" />
          <el-table-column prop="plate_number" label="车牌" width="140">
            <template #default="{ row }">
              <el-tag
                :type="row.plate_number === '<待确认>' ? 'warning' : 'primary'"
                size="large"
              >
                {{ row.plate_number }}
              </el-tag>
            </template>
          </el-table-column>
          <el-table-column label="车牌置信度" width="140">
            <template #default="{ row }">
              <el-progress
                v-if="row.plate_confidence > 0"
                :percentage="Math.round(row.plate_confidence * 100)"
                :status="row.plate_confidence > 0.85 ? 'success' : (row.plate_confidence > 0.6 ? 'warning' : 'exception')"
              />
              <span v-else>—</span>
            </template>
          </el-table-column>
          <el-table-column prop="vehicle_class" label="车型" width="180" />
          <el-table-column label="时间窗" width="180">
            <template #default="{ row }">
              {{ formatTimestamp(row.first_seen_ms) }} ~ {{ formatTimestamp(row.last_seen_ms) }}
            </template>
          </el-table-column>
          <el-table-column prop="frame_hits" label="帧数" width="80" />
          <el-table-column label="IoU" width="80">
            <template #default="{ row }">
              <span v-if="row.iou_score !== null">
                {{ (row.iou_score * 100).toFixed(0) }}%
              </span>
              <span v-else>—</span>
            </template>
          </el-table-column>
          <el-table-column prop="event_time" label="事件时间" />
          <el-table-column label="证据" width="100">
            <template #default="{ row }">
              <el-button
                v-if="evidenceFolder(row)"
                size="small"
                type="primary"
                @click="openEvidence(row)"
              >
                打开
              </el-button>
              <span v-else>—</span>
            </template>
          </el-table-column>
        </el-table>
        <el-empty v-else description="未识别到符合条件的事件" />

        <el-divider content-position="left">中间观测 ({{ outcome.observations.length }} 帧)</el-divider>
        <el-collapse>
          <el-collapse-item
            v-for="frame in outcome.observations"
            :key="frame.frame_index"
            :title="`帧 #${frame.frame_index} @ ${formatTimestamp(frame.timestamp_ms)} - ${frame.vehicles.length} 辆车`"
          >
            <el-table :data="frame.vehicles" size="small">
              <el-table-column prop="class_name" label="类别" width="180" />
              <el-table-column label="检测置信度" width="160">
                <template #default="{ row }">
                  {{ (row.vehicle_score * 100).toFixed(1) }}%
                </template>
              </el-table-column>
              <el-table-column label="车牌">
                <template #default="{ row }">
                  <el-tag v-if="row.plate" type="primary">
                    {{ row.plate.text }} ({{ (row.plate.confidence * 100).toFixed(1) }}%)
                  </el-tag>
                  <span v-else class="muted">—</span>
                </template>
              </el-table-column>
            </el-table>
          </el-collapse-item>
        </el-collapse>
      </template>
    </el-card>
  </div>
</template>

<style scoped>
.process {
  max-width: 1200px;
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

.muted {
  color: var(--el-text-color-secondary);
}
</style>
