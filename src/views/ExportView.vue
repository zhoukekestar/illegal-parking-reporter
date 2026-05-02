<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { ElMessage, ElMessageBox } from "element-plus";

import {
  listEvents,
  exportAcceptedEvents,
  type ParkingEvent,
  type ExportSummary,
} from "@/api/video";
import { openInFileManager } from "@/api/system";

const events = ref<ParkingEvent[]>([]);
const loading = ref(false);
const errMsg = ref<string | null>(null);
const targetDir = ref<string | null>(null);
const exporting = ref(false);
const summary = ref<ExportSummary | null>(null);
const selected = ref<string[]>([]);

const acceptedEvents = computed(() =>
  events.value.filter((e) => e.review_status === "accepted")
);

const notYetExported = computed(() =>
  acceptedEvents.value.filter((e) => !e.exported_at)
);

onMounted(async () => {
  await refresh();
  // 默认选中所有未导出的已采纳事件
  selected.value = notYetExported.value.map((e) => e.id);
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

async function pickTarget() {
  const folder = await open({
    multiple: false,
    directory: true,
  });
  if (typeof folder === "string") {
    targetDir.value = folder;
  }
}

async function runExport() {
  if (!targetDir.value) {
    ElMessage.warning("请先选择目标目录");
    return;
  }
  if (!selected.value.length) {
    ElMessage.warning("请至少选中一个事件");
    return;
  }
  // 二次确认 (覆盖警告): 当目标目录已存在同名 bundle 时
  await ElMessageBox.confirm(
    `将导出 ${selected.value.length} 个事件到 ${targetDir.value} ?`,
    "确认导出",
    { type: "info" }
  );

  exporting.value = true;
  errMsg.value = null;
  summary.value = null;
  try {
    summary.value = await exportAcceptedEvents(selected.value, targetDir.value);
    ElMessage.success(
      `导出完成: ${summary.value.exported_count} 个事件, ${summary.value.skipped.length} 个跳过`
    );
    await refresh();
  } catch (e) {
    errMsg.value = String(e);
  } finally {
    exporting.value = false;
  }
}

async function openBundle() {
  if (!summary.value) return;
  try {
    await openInFileManager(summary.value.bundle_path);
  } catch (e) {
    ElMessage.error(String(e));
  }
}

async function openGuide() {
  if (!summary.value) return;
  try {
    await openInFileManager(summary.value.guide_html);
  } catch (e) {
    ElMessage.error(String(e));
  }
}

function shortPath(p: string): string {
  const parts = p.split(/[/\\]/);
  return parts[parts.length - 1] || p;
}

function tagTypeForStatus(s: string) {
  if (s === "accepted") return "success" as const;
  if (s === "rejected") return "danger" as const;
  return "info" as const;
}
</script>

<template>
  <div class="export-view">
    <el-card>
      <template #header>
        <div class="card-header">
          <h2>导出证据包 (P5)</h2>
          <el-button :loading="loading" @click="refresh">刷新</el-button>
        </div>
        <p class="subtitle">
          选中已采纳事件 → 选目标目录 → 一键生成
          <code>违停举报包_YYYY-MM-DD_HH-MM/</code> 含索引 CSV + 上传指引 HTML
        </p>
      </template>

      <el-alert v-if="errMsg" type="error" :closable="false" show-icon :title="errMsg" />

      <el-row :gutter="12" class="stats">
        <el-col :span="6">
          <el-statistic title="已采纳" :value="acceptedEvents.length" value-style="{color: '#67c23a'}" />
        </el-col>
        <el-col :span="6">
          <el-statistic title="未导出" :value="notYetExported.length" value-style="{color: '#e6a23c'}" />
        </el-col>
        <el-col :span="6">
          <el-statistic title="已选中" :value="selected.length" value-style="{color: '#409eff'}" />
        </el-col>
        <el-col :span="6">
          <el-statistic title="总事件" :value="events.length" />
        </el-col>
      </el-row>

      <div class="actions-row">
        <el-button @click="pickTarget">{{ targetDir ?? "选择目标目录" }}</el-button>
        <el-button
          type="primary"
          :loading="exporting"
          :disabled="!targetDir || !selected.length"
          @click="runExport"
        >
          导出选中 {{ selected.length }} 个事件
        </el-button>
      </div>

      <el-divider content-position="left">已采纳事件 ({{ acceptedEvents.length }})</el-divider>

      <el-empty v-if="!acceptedEvents.length" description="还没有已采纳事件,请先去「审核 (P4)」" />
      <el-table
        v-else
        :data="acceptedEvents"
        stripe
        @selection-change="(rows: ParkingEvent[]) => selected = rows.map((r) => r.id)"
      >
        <el-table-column type="selection" :selectable="(row: ParkingEvent) => !row.exported_at" width="50" />
        <el-table-column type="index" label="#" width="50" />
        <el-table-column label="车牌" width="140">
          <template #default="{ row }">
            <el-tag :type="row.plate_number === '<待确认>' ? 'warning' : 'primary'" size="large">
              {{ row.plate_manual_corrected ?? row.plate_number }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="vehicle_class" label="车型" width="180" />
        <el-table-column label="时间窗" width="180">
          <template #default="{ row }">
            {{ (row.first_seen_ms / 1000).toFixed(1) }}s ~ {{ (row.last_seen_ms / 1000).toFixed(1) }}s
          </template>
        </el-table-column>
        <el-table-column label="证据完整" width="100">
          <template #default="{ row }">
            <el-tag v-if="row.snapshot_path && row.clip_path" type="success">齐全</el-tag>
            <el-tag v-else type="danger">缺</el-tag>
          </template>
        </el-table-column>
        <el-table-column label="状态" width="100">
          <template #default="{ row }">
            <el-tag :type="tagTypeForStatus(row.review_status)">{{ row.review_status }}</el-tag>
          </template>
        </el-table-column>
        <el-table-column label="导出状态" width="180">
          <template #default="{ row }">
            <el-tag v-if="row.exported_at" type="success">已导出 {{ row.exported_at?.slice(5, 16) }}</el-tag>
            <el-tag v-else type="info">未导出</el-tag>
          </template>
        </el-table-column>
        <el-table-column label="来源" min-width="200">
          <template #default="{ row }">
            <el-tooltip :content="row.source_video">
              <span>{{ shortPath(row.source_video) }}</span>
            </el-tooltip>
          </template>
        </el-table-column>
      </el-table>

      <template v-if="summary">
        <el-divider content-position="left">本次导出结果</el-divider>
        <el-descriptions :column="1" border>
          <el-descriptions-item label="顶层文件夹">
            <code>{{ summary.bundle_path }}</code>
          </el-descriptions-item>
          <el-descriptions-item label="实际导出">
            {{ summary.exported_count }} 个事件
          </el-descriptions-item>
          <el-descriptions-item label="索引 CSV">
            <code>{{ summary.index_csv }}</code>
          </el-descriptions-item>
          <el-descriptions-item label="上传指引">
            <code>{{ summary.guide_html }}</code>
            <el-text class="hint" size="small">在浏览器中按 ⌘P 可打印为 PDF</el-text>
          </el-descriptions-item>
        </el-descriptions>

        <div class="actions-row">
          <el-button type="primary" @click="openBundle">在 Finder 打开包</el-button>
          <el-button @click="openGuide">打开上传指引 HTML</el-button>
        </div>

        <template v-if="summary.skipped.length">
          <el-alert
            class="skipped"
            type="warning"
            :closable="false"
            show-icon
            :title="`跳过 ${summary.skipped.length} 个事件`"
          />
          <el-table :data="summary.skipped" size="small">
            <el-table-column prop="event_id" label="事件 ID" />
            <el-table-column prop="reason" label="原因" />
          </el-table>
        </template>
      </template>
    </el-card>
  </div>
</template>

<style scoped>
.export-view {
  max-width: 1300px;
}
.card-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
h2 { margin: 0; font-size: 18px; }
.subtitle {
  margin: 6px 0 0;
  color: var(--el-text-color-secondary);
  font-size: 13px;
}
.stats { margin: 16px 0; }
.actions-row {
  display: flex;
  gap: 12px;
  align-items: center;
  margin: 16px 0;
  flex-wrap: wrap;
}
.skipped { margin-top: 16px; }
.hint { margin-left: 8px; }
</style>
