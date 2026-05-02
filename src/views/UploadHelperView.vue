<script setup lang="ts">
// P8.1 剪贴板上传助手 (DEVELOPMENT_PLAN.md §五 P8.1)
//
// 用户在 iPhone Mirroring 中操作警察叔叔 / 支付宝, 软件提供:
// - 当前事件信息卡片
// - 一键复制车牌 / 时间 / 全部
// - 上一个 / 下一个
// - 标记已上传

import { ref, computed, onMounted } from "vue";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { ElMessage } from "element-plus";

import {
  listEvents,
  markEventUploaded,
  type ParkingEvent,
} from "@/api/video";

const events = ref<ParkingEvent[]>([]);
const loading = ref(false);
const errMsg = ref<string | null>(null);

// 只对已采纳 + 未上传的事件做循环
const queue = computed(() =>
  events.value.filter((e) => e.review_status === "accepted" && !e.uploaded_at)
);

const cursor = ref(0);
const current = computed<ParkingEvent | null>(() =>
  queue.value.length > 0 ? queue.value[Math.min(cursor.value, queue.value.length - 1)] : null
);

onMounted(async () => {
  await refresh();
});

async function refresh() {
  loading.value = true;
  errMsg.value = null;
  try {
    events.value = await listEvents();
    if (cursor.value >= queue.value.length) cursor.value = 0;
  } catch (e) {
    errMsg.value = String(e);
  } finally {
    loading.value = false;
  }
}

function nextEvent() {
  if (cursor.value < queue.value.length - 1) cursor.value++;
}
function prevEvent() {
  if (cursor.value > 0) cursor.value--;
}

const plate = computed(() =>
  current.value
    ? current.value.plate_manual_corrected ?? current.value.plate_number
    : ""
);
const eventTime = computed(() =>
  current.value?.event_time ?? `视频偏移 ${(current.value?.timestamp_ms ?? 0) / 1000}s`
);
const violationType = computed(() => "占用人行道");

const allText = computed(() => {
  if (!current.value) return "";
  return [
    `车牌: ${plate.value}`,
    `时间: ${eventTime.value}`,
    `违法类型: ${violationType.value}`,
    `视频文件: ${current.value.source_video}`,
  ].join("\n");
});

async function copyPlate() {
  if (!plate.value) return;
  await writeText(plate.value);
  ElMessage.success(`已复制: ${plate.value}`);
}
async function copyTime() {
  if (!current.value) return;
  await writeText(eventTime.value);
  ElMessage.success("时间已复制");
}
async function copyAll() {
  if (!current.value) return;
  await writeText(allText.value);
  ElMessage.success("信息已复制 (车牌+时间+类型+视频)");
}

async function markUploaded() {
  if (!current.value) return;
  try {
    await markEventUploaded(current.value.id);
    ElMessage.success("已标记 已上传");
    await refresh();
  } catch (e) {
    ElMessage.error(String(e));
  }
}
</script>

<template>
  <div class="helper">
    <el-card>
      <template #header>
        <div class="card-header">
          <h2>剪贴板上传助手 (P8.1)</h2>
          <el-button @click="refresh" :loading="loading">刷新</el-button>
        </div>
        <p class="subtitle">
          切到 iPhone Mirroring → 进入"警察叔叔" App → 在本面板复制信息粘贴到 App, 完成后点"已上传"。
        </p>
      </template>

      <el-alert v-if="errMsg" type="error" :closable="false" show-icon :title="errMsg" />

      <el-empty
        v-if="!queue.length"
        description="队列为空: 已采纳且未上传的事件没有了"
      >
        <template #default>
          <el-text size="small" type="info">先到「审核」采纳事件, 或所有事件都已上传</el-text>
        </template>
      </el-empty>

      <template v-else>
        <div class="progress">
          <el-progress
            :percentage="Math.round(((cursor + 1) / queue.length) * 100)"
            :status="cursor + 1 === queue.length ? 'success' : undefined"
          />
          <el-text class="counter">
            {{ cursor + 1 }} / {{ queue.length }}
          </el-text>
        </div>

        <div class="card-row">
          <div class="info">
            <div class="big-plate">{{ plate }}</div>
            <el-descriptions :column="1" border>
              <el-descriptions-item label="拍摄时间">
                {{ eventTime }}
              </el-descriptions-item>
              <el-descriptions-item label="违法类型">
                {{ violationType }}
              </el-descriptions-item>
              <el-descriptions-item label="车型">
                {{ current?.vehicle_class }}
              </el-descriptions-item>
              <el-descriptions-item label="占人行道率">
                {{ current?.iou_score !== null ? ((current?.iou_score ?? 0) * 100).toFixed(0) + '%' : '—' }}
              </el-descriptions-item>
              <el-descriptions-item label="证据视频" :class="'mono'">
                <code>{{ current?.clip_path }}</code>
              </el-descriptions-item>
              <el-descriptions-item label="事件 ID">
                <code>{{ current?.id }}</code>
              </el-descriptions-item>
            </el-descriptions>
          </div>
        </div>

        <div class="actions">
          <el-button-group>
            <el-button :disabled="cursor === 0" @click="prevEvent">← 上一个</el-button>
            <el-button :disabled="cursor + 1 >= queue.length" @click="nextEvent">下一个 →</el-button>
          </el-button-group>
          <el-divider direction="vertical" />
          <el-button type="primary" @click="copyPlate">复制车牌</el-button>
          <el-button @click="copyTime">复制时间</el-button>
          <el-button @click="copyAll">复制全部</el-button>
          <el-divider direction="vertical" />
          <el-button type="success" @click="markUploaded">已上传, 下一个</el-button>
        </div>
      </template>
    </el-card>
  </div>
</template>

<style scoped>
.helper {
  max-width: 900px;
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
.progress {
  display: flex;
  align-items: center;
  gap: 12px;
  margin: 16px 0;
}
.counter {
  white-space: nowrap;
}
.card-row {
  margin: 16px 0;
}
.big-plate {
  font-size: 56px;
  font-weight: 700;
  letter-spacing: 8px;
  color: #2080ff;
  text-align: center;
  background: var(--el-color-primary-light-9);
  padding: 24px;
  border-radius: 8px;
  margin-bottom: 16px;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
}
.actions {
  display: flex;
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
  padding-top: 16px;
  border-top: 1px solid var(--el-border-color-light);
}
code {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
  word-break: break-all;
}
</style>
