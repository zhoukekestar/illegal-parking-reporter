<script setup lang="ts">
import { ref, onMounted, computed } from "vue";

import { listEvents, type ParkingEvent } from "@/api/video";
import { openInFileManager } from "@/api/system";
import { ElMessage } from "element-plus";

const events = ref<ParkingEvent[]>([]);
const loading = ref(false);
const errMsg = ref<string | null>(null);

const filterPlate = ref("");
const filterStatus = ref<string>("");

const filtered = computed(() => {
  return events.value.filter((e) => {
    if (filterPlate.value && !e.plate_number.includes(filterPlate.value)) return false;
    if (filterStatus.value && e.review_status !== filterStatus.value) return false;
    return true;
  });
});

async function refresh() {
  loading.value = true;
  errMsg.value = null;
  try {
    events.value = await listEvents();
  } catch (e) {
    errMsg.value = String(e);
  } finally {
    loading.value = false;
  }
}

onMounted(refresh);

function formatTimestamp(ms: number): string {
  const s = ms / 1000;
  return `${s.toFixed(1)}s`;
}

function shortPath(p: string): string {
  const parts = p.split(/[/\\]/);
  return parts[parts.length - 1] || p;
}

function evidenceFolder(e: ParkingEvent): string | null {
  // snapshot_path / clip_path 同处一个目录, 取 snapshot 的 parent
  const p = e.snapshot_path ?? e.clip_path;
  if (!p) return null;
  const idx = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
  if (idx <= 0) return null;
  return p.substring(0, idx);
}

async function openEvidence(e: ParkingEvent) {
  const folder = evidenceFolder(e);
  if (!folder) {
    ElMessage.warning("该事件未生成证据包");
    return;
  }
  try {
    await openInFileManager(folder);
  } catch (err) {
    ElMessage.error(String(err));
  }
}
</script>

<template>
  <div class="events">
    <el-card>
      <template #header>
        <div class="card-header">
          <h2>事件列表 (DB 持久化)</h2>
          <el-button :loading="loading" @click="refresh">刷新</el-button>
        </div>
        <p class="subtitle">所有 process_video 处理过的事件都在这里, 重启后仍可见</p>
      </template>

      <el-alert
        v-if="errMsg"
        type="error"
        :closable="false"
        show-icon
        title="查询失败"
        :description="errMsg"
      />

      <div class="filters">
        <el-input v-model="filterPlate" placeholder="按车牌过滤" clearable style="width: 200px" />
        <el-select v-model="filterStatus" placeholder="按状态过滤" clearable style="width: 160px">
          <el-option label="待审" value="pending" />
          <el-option label="已采纳" value="accepted" />
          <el-option label="已丢弃" value="rejected" />
          <el-option label="待定" value="deferred" />
        </el-select>
        <el-tag>共 {{ filtered.length }} 条</el-tag>
      </div>

      <el-skeleton v-if="loading" :rows="6" animated />
      <el-empty v-else-if="!filtered.length" description="暂无事件,先去「视频处理」页跑一个视频" />
      <el-table v-else :data="filtered" stripe>
        <el-table-column type="index" label="#" width="50" />
        <el-table-column label="车牌" width="140">
          <template #default="{ row }">
            <el-tag
              :type="row.plate_number === '<待确认>' ? 'warning' : 'primary'"
              size="large"
            >
              {{ row.plate_number }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column label="置信度" width="100">
          <template #default="{ row }">
            <span v-if="row.plate_confidence > 0">
              {{ (row.plate_confidence * 100).toFixed(0) }}%
            </span>
            <span v-else>—</span>
          </template>
        </el-table-column>
        <el-table-column prop="vehicle_class" label="车型" width="180" />
        <el-table-column label="来源视频" width="200">
          <template #default="{ row }">
            <el-tooltip :content="row.source_video">
              <span>{{ shortPath(row.source_video) }}</span>
            </el-tooltip>
          </template>
        </el-table-column>
        <el-table-column label="时间窗" width="180">
          <template #default="{ row }">
            {{ formatTimestamp(row.first_seen_ms) }} ~ {{ formatTimestamp(row.last_seen_ms) }}
          </template>
        </el-table-column>
        <el-table-column prop="frame_hits" label="帧" width="60" />
        <el-table-column prop="event_time" label="拍摄时间" />
        <el-table-column label="状态" width="100">
          <template #default="{ row }">
            <el-tag :type="row.review_status === 'accepted' ? 'success' : (row.review_status === 'rejected' ? 'danger' : 'info')">
              {{ row.review_status }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column label="IoU" width="80">
          <template #default="{ row }">
            <span v-if="row.iou_score !== null">
              {{ (row.iou_score * 100).toFixed(0) }}%
            </span>
            <span v-else>—</span>
          </template>
        </el-table-column>
        <el-table-column label="证据" width="120">
          <template #default="{ row }">
            <el-button
              v-if="evidenceFolder(row)"
              size="small"
              type="primary"
              @click="openEvidence(row)"
            >
              打开
            </el-button>
            <span v-else class="muted-inline">—</span>
          </template>
        </el-table-column>
      </el-table>
    </el-card>
  </div>
</template>

<style scoped>
.events {
  max-width: 1300px;
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

.subtitle {
  margin: 6px 0 0;
  color: var(--el-text-color-secondary);
  font-size: 13px;
}

.filters {
  display: flex;
  gap: 12px;
  align-items: center;
  margin: 16px 0;
}

.muted-inline {
  color: var(--el-text-color-secondary);
}
</style>
