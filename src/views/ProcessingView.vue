<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, reactive } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { ElMessage, ElMessageBox } from "element-plus";

import {
  listenPipeline,
  startBatchPipeline,
  resumePendingJobs,
  listJobs,
  listPendingJobs,
  type VideoJob,
  type PipelineEvent,
  type Stage,
} from "@/api/pipeline";

interface JobProgress {
  job_id: string;
  video: string;
  status: "pending" | "running" | "success" | "failed";
  stage: Stage | null;
  processed: number;
  total: number;
  error: string | null;
  events_count: number;
}

const liveJobs = reactive<Map<string, JobProgress>>(new Map());
const dbJobs = ref<VideoJob[]>([]);
const pendingJobs = ref<VideoJob[]>([]);
const currentBatch = ref<string | null>(null);
const batchTotal = ref(0);
const batchSuccess = ref(0);
const batchFailed = ref(0);
const batchRunning = ref(false);
const errMsg = ref<string | null>(null);

let unlisten: (() => void) | null = null;

onMounted(async () => {
  // 订阅事件
  unlisten = await listenPipeline(handleEvent);
  await refreshJobs();
  await refreshPending();
});

onUnmounted(() => {
  unlisten?.();
});

function handleEvent(ev: PipelineEvent) {
  switch (ev.type) {
    case "batch_started":
      currentBatch.value = ev.batch_id;
      batchTotal.value = ev.total;
      batchSuccess.value = 0;
      batchFailed.value = 0;
      batchRunning.value = ev.total > 0;
      break;
    case "job_started":
      liveJobs.set(ev.job_id, {
        job_id: ev.job_id,
        video: ev.video,
        status: "running",
        stage: null,
        processed: 0,
        total: 0,
        error: null,
        events_count: 0,
      });
      break;
    case "job_progress": {
      const j = liveJobs.get(ev.job_id);
      if (j) {
        j.stage = ev.stage;
        j.processed = ev.processed;
        j.total = ev.total;
        j.status = "running";
      }
      break;
    }
    case "job_succeeded": {
      const j = liveJobs.get(ev.job_id);
      if (j) {
        j.status = "success";
        j.events_count = ev.events_count;
        j.processed = j.total;
      }
      batchSuccess.value++;
      refreshJobs();
      break;
    }
    case "job_failed": {
      const j = liveJobs.get(ev.job_id);
      if (j) {
        j.status = "failed";
        j.error = ev.error;
      }
      batchFailed.value++;
      refreshJobs();
      break;
    }
    case "batch_finished":
      batchRunning.value = false;
      ElMessage.success(
        `批处理完成: 成功 ${ev.success_count} / 失败 ${ev.fail_count} / 耗时 ${(ev.duration_ms / 1000).toFixed(1)}s`
      );
      refreshJobs();
      refreshPending();
      break;
  }
}

async function pickAndStart() {
  errMsg.value = null;
  const selected = await open({
    multiple: true,
    directory: false,
    filters: [{ name: "视频", extensions: ["mp4", "mov", "m4v"] }],
  });
  if (!selected) return;
  const paths = Array.isArray(selected) ? selected : [selected];
  if (!paths.length) return;
  await startBatch(paths);
}

async function startBatch(paths: string[]) {
  liveJobs.clear();
  batchRunning.value = true;
  try {
    const outcome = await startBatchPipeline(paths);
    if (outcome.job_count === 0) {
      ElMessage.info("选中的视频都已处理过, 无新任务");
      batchRunning.value = false;
    } else {
      ElMessage.success(`已派发 ${outcome.job_count} 个任务`);
    }
  } catch (e) {
    errMsg.value = String(e);
    batchRunning.value = false;
  }
}

async function resume() {
  try {
    const outcome = await resumePendingJobs();
    if (outcome.job_count === 0) {
      ElMessage.info("没有待续跑的任务");
    } else {
      ElMessage.success(`已续跑 ${outcome.job_count} 个任务`);
    }
  } catch (e) {
    errMsg.value = String(e);
  }
}

async function retryFailed(job: VideoJob) {
  await ElMessageBox.confirm(
    `重试视频「${shortPath(job.source_video)}」?`,
    "确认重试",
    { type: "warning" }
  );
  await startBatch([job.source_video]);
}

async function refreshJobs() {
  try {
    dbJobs.value = await listJobs();
  } catch (e) {
    errMsg.value = String(e);
  }
}

async function refreshPending() {
  try {
    pendingJobs.value = await listPendingJobs();
  } catch (_) {}
}

const liveJobsArr = computed(() => Array.from(liveJobs.values()));

const overallPercent = computed(() => {
  if (!batchTotal.value) return 0;
  return Math.round(((batchSuccess.value + batchFailed.value) * 100) / batchTotal.value);
});

function shortPath(p: string): string {
  const parts = p.split(/[/\\]/);
  return parts[parts.length - 1] || p;
}

function jobPercent(j: JobProgress): number {
  if (j.status === "success") return 100;
  if (!j.total) return 0;
  return Math.min(100, Math.round((j.processed * 100) / j.total));
}

function stageLabel(s: Stage | null): string {
  if (!s) return "排队中";
  return s === "extract" ? "抽帧" : s === "infer" ? "AI 推理" : "聚合";
}

function tagTypeForStatus(s: string): "info" | "success" | "warning" | "danger" | "primary" {
  switch (s) {
    case "running":
      return "primary";
    case "success":
      return "success";
    case "failed":
      return "danger";
    case "pending":
      return "warning";
    default:
      return "info";
  }
}
</script>

<template>
  <div class="processing">
    <el-card>
      <template #header>
        <h2>批量处理 (P2)</h2>
        <p class="subtitle">
          一次选多个视频, 三阶段并发流水线 (抽帧 4 / AI 1 / 聚合 4), kill 后可续跑
        </p>
      </template>

      <el-space wrap>
        <el-button type="primary" :disabled="batchRunning" @click="pickAndStart">
          选择多个视频开始处理
        </el-button>
        <el-button
          v-if="pendingJobs.length"
          type="warning"
          :disabled="batchRunning"
          @click="resume"
        >
          续跑 {{ pendingJobs.length }} 个未完成任务
        </el-button>
        <el-button @click="refreshJobs">刷新历史</el-button>
      </el-space>

      <el-alert
        v-if="errMsg"
        class="alert"
        type="error"
        :closable="false"
        show-icon
        :title="errMsg"
      />

      <template v-if="batchTotal > 0">
        <el-divider content-position="left">
          整体进度 (Batch {{ currentBatch?.slice(0, 8) }})
        </el-divider>
        <el-row :gutter="16" class="overview">
          <el-col :span="6">
            <el-statistic title="总任务" :value="batchTotal" />
          </el-col>
          <el-col :span="6">
            <el-statistic title="成功" :value="batchSuccess" />
          </el-col>
          <el-col :span="6">
            <el-statistic title="失败" :value="batchFailed" />
          </el-col>
          <el-col :span="6">
            <el-statistic
              title="完成率"
              :value="overallPercent"
              suffix="%"
            />
          </el-col>
        </el-row>
        <el-progress
          :percentage="overallPercent"
          :status="batchFailed > 0 ? 'warning' : (batchRunning ? undefined : 'success')"
        />
      </template>

      <template v-if="liveJobsArr.length">
        <el-divider content-position="left">实时任务进度</el-divider>
        <el-table :data="liveJobsArr" stripe>
          <el-table-column type="index" label="#" width="50" />
          <el-table-column label="视频" min-width="200">
            <template #default="{ row }">
              <el-tooltip :content="row.video">
                <span>{{ shortPath(row.video) }}</span>
              </el-tooltip>
            </template>
          </el-table-column>
          <el-table-column label="状态" width="100">
            <template #default="{ row }">
              <el-tag :type="tagTypeForStatus(row.status)">{{ row.status }}</el-tag>
            </template>
          </el-table-column>
          <el-table-column label="阶段" width="120">
            <template #default="{ row }">
              {{ stageLabel(row.stage) }}
            </template>
          </el-table-column>
          <el-table-column label="进度" min-width="240">
            <template #default="{ row }">
              <div class="progress-cell">
                <el-progress
                  :percentage="jobPercent(row)"
                  :status="row.status === 'failed' ? 'exception' : (row.status === 'success' ? 'success' : undefined)"
                />
                <span v-if="row.total" class="progress-text">
                  {{ row.processed }} / {{ row.total }}
                </span>
              </div>
            </template>
          </el-table-column>
          <el-table-column label="事件数" width="80">
            <template #default="{ row }">
              <span v-if="row.status === 'success'">{{ row.events_count }}</span>
              <span v-else>—</span>
            </template>
          </el-table-column>
          <el-table-column label="错误" min-width="180">
            <template #default="{ row }">
              <el-tooltip v-if="row.error" :content="row.error">
                <el-text type="danger" truncated>{{ row.error }}</el-text>
              </el-tooltip>
            </template>
          </el-table-column>
        </el-table>
      </template>

      <el-divider content-position="left">历史任务 (DB)</el-divider>
      <el-empty v-if="!dbJobs.length" description="暂无历史" />
      <el-table v-else :data="dbJobs" stripe>
        <el-table-column type="index" label="#" width="50" />
        <el-table-column label="视频" min-width="200">
          <template #default="{ row }">
            <el-tooltip :content="row.source_video">
              <span>{{ shortPath(row.source_video) }}</span>
            </el-tooltip>
          </template>
        </el-table-column>
        <el-table-column label="状态" width="100">
          <template #default="{ row }">
            <el-tag :type="tagTypeForStatus(row.status)">{{ row.status }}</el-tag>
          </template>
        </el-table-column>
        <el-table-column label="进度" width="160">
          <template #default="{ row }">
            <span v-if="row.estimated_frames">
              {{ row.processed_frames }} / {{ row.estimated_frames }}
            </span>
            <span v-else>—</span>
          </template>
        </el-table-column>
        <el-table-column prop="events_count" label="事件" width="80" />
        <el-table-column prop="created_at" label="入队时间" min-width="180" />
        <el-table-column prop="finished_at" label="完成时间" min-width="180" />
        <el-table-column label="错误 / 操作" min-width="180">
          <template #default="{ row }">
            <el-tooltip v-if="row.last_error" :content="row.last_error" placement="top">
              <el-text type="danger" truncated>{{ row.last_error }}</el-text>
            </el-tooltip>
            <el-button
              v-if="row.status === 'failed'"
              size="small"
              type="warning"
              :disabled="batchRunning"
              @click="retryFailed(row)"
            >
              重试
            </el-button>
          </template>
        </el-table-column>
      </el-table>
    </el-card>
  </div>
</template>

<style scoped>
.processing {
  max-width: 1400px;
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

.alert {
  margin-top: 16px;
}

.overview {
  margin: 16px 0;
}

.progress-cell {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.progress-text {
  font-size: 11px;
  color: var(--el-text-color-secondary);
}
</style>
