<script setup lang="ts">
import { ref, computed, onMounted, watch, nextTick } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import { ElMessage, ElMessageBox } from "element-plus";

import {
  listEvents,
  updateEventStatus,
  updateEventPlate,
  cleanupInvalidEvents,
  type ParkingEvent,
} from "@/api/video";
import { openInFileManager } from "@/api/system";
import { useReviewShortcuts } from "@/composables/useReviewShortcuts";

type Status = ParkingEvent["review_status"];

const events = ref<ParkingEvent[]>([]);
const loading = ref(false);
const errMsg = ref<string | null>(null);

const filterStatus = ref<Status | "">("");
const filterPlate = ref("");
const sortBy = ref<"low_conf" | "time" | "iou">("low_conf");

const selectedId = ref<string | null>(null);
const editingPlate = ref<string>("");

// 撤销栈: { id, prevStatus, prevPlate }
interface UndoEntry {
  id: string;
  prevStatus: Status;
  prevPlate: string | null;
}
const undoStack = ref<UndoEntry[]>([]);
const UNDO_LIMIT = 10;

// 视频播放器
const videoRef = ref<HTMLVideoElement | null>(null);

onMounted(async () => {
  await refresh();
  if (filtered.value.length) {
    selectEvent(filtered.value[0].id);
  }
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

async function cleanupInvalid() {
  await ElMessageBox.confirm(
    "将删除所有车牌不符合中国格式的事件 (含 <待确认>, OCR 乱码),\n同时删除对应的证据文件夹. 此操作不可撤销.",
    "清理无效事件",
    { type: "warning", confirmButtonText: "确认清理", cancelButtonText: "取消" }
  );
  try {
    const r = await cleanupInvalidEvents();
    ElMessage.success(
      `已删除 ${r.deleted_count} 个无效事件 (含 ${r.deleted_evidence_dirs} 个证据目录)`
    );
    await refresh();
  } catch (e) {
    ElMessage.error(String(e));
  }
}

const videoEl = computed(() => videoRef.value);

async function toggleFullscreen() {
  const v = videoEl.value;
  if (!v) return;
  if (document.fullscreenElement === v) {
    await document.exitFullscreen();
  } else {
    await v.requestFullscreen();
  }
}

const filtered = computed(() => {
  let list = events.value.slice();
  if (filterStatus.value) list = list.filter((e) => e.review_status === filterStatus.value);
  if (filterPlate.value) list = list.filter((e) => e.plate_number.includes(filterPlate.value));
  switch (sortBy.value) {
    case "low_conf":
      list.sort((a, b) => a.plate_confidence - b.plate_confidence);
      break;
    case "time":
      list.sort((a, b) => b.timestamp_ms - a.timestamp_ms);
      break;
    case "iou":
      list.sort((a, b) => (b.iou_score ?? 0) - (a.iou_score ?? 0));
      break;
  }
  return list;
});

const selected = computed<ParkingEvent | null>(
  () => filtered.value.find((e) => e.id === selectedId.value) || null
);

// 同车牌跨视频提示
const plateOccurrences = computed(() => {
  const map = new Map<string, number>();
  for (const e of events.value) {
    map.set(e.plate_number, (map.get(e.plate_number) ?? 0) + 1);
  }
  return map;
});

function selectEvent(id: string) {
  selectedId.value = id;
  const e = filtered.value.find((x) => x.id === id);
  editingPlate.value = e?.plate_manual_corrected ?? e?.plate_number ?? "";
  // 自动播放视频
  nextTick(() => {
    const v = videoRef.value;
    if (v) {
      v.currentTime = 0;
      v.play().catch(() => {});
    }
    // 滚动当前条目到视野
    const el = document.querySelector(`[data-event-id='${id}']`);
    if (el && "scrollIntoView" in el) {
      (el as HTMLElement).scrollIntoView({ block: "nearest" });
    }
  });
}

function currentIndex(): number {
  if (!selectedId.value) return -1;
  return filtered.value.findIndex((e) => e.id === selectedId.value);
}

function next() {
  const idx = currentIndex();
  if (idx === -1 || idx >= filtered.value.length - 1) return;
  selectEvent(filtered.value[idx + 1].id);
}

function prev() {
  const idx = currentIndex();
  if (idx <= 0) return;
  selectEvent(filtered.value[idx - 1].id);
}

async function applyStatus(status: Status, advance = true) {
  const e = selected.value;
  if (!e) return;

  // P4 规则: 车牌「<待确认>」必须先手动输入才能 accept
  if (status === "accepted" && (e.plate_number === "<待确认>" || e.plate_number.trim() === "")) {
    if (!e.plate_manual_corrected || e.plate_manual_corrected.trim() === "") {
      ElMessage.warning("待确认车牌必须先手动输入才能采纳");
      return;
    }
  }

  // 入栈撤销 (旧状态)
  pushUndo({
    id: e.id,
    prevStatus: e.review_status,
    prevPlate: e.plate_manual_corrected,
  });

  // 乐观更新本地
  e.review_status = status;
  try {
    await updateEventStatus(e.id, status);
  } catch (err) {
    ElMessage.error(`保存失败: ${err}`);
    // 回滚
    e.review_status = undoStack.value.pop()?.prevStatus ?? "pending";
    return;
  }

  if (advance) next();
}

async function applyPlateCorrection() {
  const e = selected.value;
  if (!e) return;
  const trimmed = editingPlate.value.trim();
  // 只在和当前 corrected 不同时保存
  const corrected = trimmed === "" || trimmed === e.plate_number ? null : trimmed;
  if (corrected === e.plate_manual_corrected) return;

  pushUndo({
    id: e.id,
    prevStatus: e.review_status,
    prevPlate: e.plate_manual_corrected,
  });

  e.plate_manual_corrected = corrected;
  try {
    await updateEventPlate(e.id, corrected);
    ElMessage.success(corrected ? `车牌已修正: ${corrected}` : "已清除人工车牌");
  } catch (err) {
    ElMessage.error(`保存失败: ${err}`);
  }
}

function pushUndo(entry: UndoEntry) {
  undoStack.value.push(entry);
  if (undoStack.value.length > UNDO_LIMIT) undoStack.value.shift();
}

async function undo() {
  const entry = undoStack.value.pop();
  if (!entry) {
    ElMessage.info("无可撤销的操作");
    return;
  }
  const e = events.value.find((x) => x.id === entry.id);
  if (!e) return;
  e.review_status = entry.prevStatus;
  e.plate_manual_corrected = entry.prevPlate;
  try {
    await updateEventStatus(e.id, entry.prevStatus);
    await updateEventPlate(e.id, entry.prevPlate);
    selectEvent(e.id);
    ElMessage.success("已撤销");
  } catch (err) {
    ElMessage.error(`撤销失败: ${err}`);
  }
}

function togglePlay() {
  const v = videoRef.value;
  if (!v) return;
  if (v.paused) v.play().catch(() => {});
  else v.pause();
}

useReviewShortcuts({
  prev,
  next,
  accept: () => applyStatus("accepted"),
  reject: () => applyStatus("rejected"),
  defer: () => applyStatus("deferred"),
  togglePlay,
  undo,
});

// canvas overlay for vehicle bbox
const canvasRef = ref<HTMLCanvasElement | null>(null);

watch(selected, () => {
  drawOverlay();
});

function onVideoMeta() {
  drawOverlay();
}

function drawOverlay() {
  const e = selected.value;
  const c = canvasRef.value;
  const v = videoRef.value;
  if (!c || !v || !e) return;
  // 视频元素 box 大小 = 显示尺寸
  const dispW = v.clientWidth;
  const dispH = v.clientHeight;
  c.width = dispW;
  c.height = dispH;
  const ctx = c.getContext("2d");
  if (!ctx) return;
  ctx.clearRect(0, 0, dispW, dispH);

  // 视频原始尺寸 (videoWidth/videoHeight 是 metadata 加载后)
  const vw = v.videoWidth;
  const vh = v.videoHeight;
  if (!vw || !vh) return;
  const scaleX = dispW / vw;
  const scaleY = dispH / vh;

  // bbox 是基于"原始视频尺寸"的, 但证据视频已经是原分辨率, 所以一致
  const [x1, y1, x2, y2] = e.vehicle_bbox;
  const rx = x1 * scaleX;
  const ry = y1 * scaleY;
  const rw = (x2 - x1) * scaleX;
  const rh = (y2 - y1) * scaleY;
  ctx.strokeStyle = "rgba(64, 224, 208, 0.95)";
  ctx.lineWidth = 3;
  ctx.strokeRect(rx, ry, rw, rh);
  // 标签
  ctx.fillStyle = "rgba(64, 224, 208, 0.85)";
  ctx.fillRect(rx, Math.max(0, ry - 22), 110, 22);
  ctx.fillStyle = "#fff";
  ctx.font = "13px ui-monospace, Menlo, monospace";
  ctx.fillText(`${e.vehicle_class.split(" ")[0]}`, rx + 6, Math.max(14, ry - 6));
}

const stats = computed(() => {
  const total = events.value.length;
  let accepted = 0,
    rejected = 0,
    deferred = 0,
    pending = 0;
  for (const e of events.value) {
    switch (e.review_status) {
      case "accepted":
        accepted++;
        break;
      case "rejected":
        rejected++;
        break;
      case "deferred":
        deferred++;
        break;
      default:
        pending++;
    }
  }
  return { total, accepted, rejected, deferred, pending };
});

function tagTypeForStatus(s: Status): "info" | "success" | "warning" | "danger" | "primary" {
  switch (s) {
    case "accepted":
      return "success";
    case "rejected":
      return "danger";
    case "deferred":
      return "warning";
    default:
      return "info";
  }
}

function shortPath(p: string): string {
  const parts = p.split(/[/\\]/);
  return parts[parts.length - 1] || p;
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

function videoSrc(e: ParkingEvent): string | null {
  if (!e.clip_path) return null;
  return convertFileSrc(e.clip_path);
}

function snapshotSrc(e: ParkingEvent): string | null {
  if (!e.snapshot_path) return null;
  return convertFileSrc(e.snapshot_path);
}
</script>

<template>
  <div class="review">
    <el-card>
      <template #header>
        <div class="card-header">
          <h2>审核工作区 (P4)</h2>
          <div class="shortcuts-hint">
            <el-tag size="small">←/→ 切换</el-tag>
            <el-tag size="small" type="success">↑ 采纳</el-tag>
            <el-tag size="small" type="danger">↓ 丢弃</el-tag>
            <el-tag size="small" type="warning">D 待定</el-tag>
            <el-tag size="small">Space 播/停</el-tag>
            <el-tag size="small">U/⌘Z 撤销</el-tag>
          </div>
        </div>
      </template>

      <!-- 进度统计 -->
      <el-row :gutter="12" class="stats">
        <el-col :span="4"><el-statistic title="总数" :value="stats.total" /></el-col>
        <el-col :span="4"><el-statistic title="待审" :value="stats.pending" value-style="{color:'#909399'}" /></el-col>
        <el-col :span="4"><el-statistic title="已采纳" :value="stats.accepted" value-style="{color:'#67c23a'}" /></el-col>
        <el-col :span="4"><el-statistic title="已丢弃" :value="stats.rejected" value-style="{color:'#f56c6c'}" /></el-col>
        <el-col :span="4"><el-statistic title="待定" :value="stats.deferred" value-style="{color:'#e6a23c'}" /></el-col>
        <el-col :span="4">
          <el-statistic title="审核率" :value="stats.total > 0 ? Math.round(((stats.total - stats.pending) / stats.total) * 100) : 0" suffix="%" />
        </el-col>
      </el-row>

      <el-alert v-if="errMsg" type="error" :closable="false" show-icon :title="errMsg" />

      <!-- 筛选 / 排序 -->
      <div class="filters">
        <el-input v-model="filterPlate" placeholder="按车牌过滤" clearable style="width: 180px" />
        <el-select v-model="filterStatus" placeholder="状态" clearable style="width: 120px">
          <el-option label="待审" value="pending" />
          <el-option label="已采纳" value="accepted" />
          <el-option label="已丢弃" value="rejected" />
          <el-option label="待定" value="deferred" />
        </el-select>
        <el-select v-model="sortBy" style="width: 160px">
          <el-option label="低置信度优先" value="low_conf" />
          <el-option label="按时间倒序" value="time" />
          <el-option label="按 IoU 高优先" value="iou" />
        </el-select>
        <el-button @click="refresh">刷新</el-button>
        <el-button type="warning" plain @click="cleanupInvalid">
          清理无效事件
        </el-button>
        <el-tag>{{ filtered.length }} / {{ events.length }}</el-tag>
      </div>

      <!-- 主体: 列表 + 详情 -->
      <div class="layout">
        <div class="list">
          <el-empty v-if="!filtered.length" description="暂无事件" />
          <div
            v-for="e in filtered"
            :key="e.id"
            :data-event-id="e.id"
            class="list-item"
            :class="{ active: selectedId === e.id }"
            @click="selectEvent(e.id)"
          >
            <img
              v-if="snapshotSrc(e)"
              :src="snapshotSrc(e)!"
              class="thumb"
              alt="thumb"
            />
            <div v-else class="thumb-empty">无截图</div>
            <div class="meta">
              <div class="meta-row">
                <el-tag
                  :type="e.plate_number === '<待确认>' ? 'warning' : 'primary'"
                  size="small"
                >
                  {{ e.plate_manual_corrected ?? e.plate_number }}
                </el-tag>
                <el-tag size="small" :type="tagTypeForStatus(e.review_status)">
                  {{ e.review_status }}
                </el-tag>
              </div>
              <div class="meta-row sub">
                <span>{{ shortPath(e.source_video) }}</span>
              </div>
              <div class="meta-row sub">
                <span>conf {{ (e.plate_confidence * 100).toFixed(0) }}%</span>
                <span v-if="e.iou_score !== null">IoU {{ (e.iou_score * 100).toFixed(0) }}%</span>
                <span v-if="(plateOccurrences.get(e.plate_number) ?? 0) > 1" class="cross-video">
                  跨 {{ plateOccurrences.get(e.plate_number) }} 视频
                </span>
              </div>
            </div>
          </div>
        </div>

        <div class="detail">
          <el-empty v-if="!selected" description="请从左侧选择一个事件" />
          <template v-else>
            <!-- 视频 + 截图并排, 各 50% -->
            <div class="media-row">
              <div class="media-block video-block">
                <video
                  v-if="videoSrc(selected)"
                  ref="videoRef"
                  :src="videoSrc(selected)!"
                  autoplay
                  loop
                  muted
                  controls
                  @loadedmetadata="onVideoMeta"
                />
                <div v-else class="no-media">该事件没有证据视频</div>
                <canvas ref="canvasRef" class="overlay" />
                <el-button
                  v-if="videoSrc(selected)"
                  class="fullscreen-btn"
                  size="small"
                  @click="toggleFullscreen"
                >
                  全屏
                </el-button>
              </div>
              <div class="media-block snapshot-block">
                <el-image
                  v-if="snapshotSrc(selected)"
                  :src="snapshotSrc(selected)!"
                  :preview-src-list="[snapshotSrc(selected)!]"
                  :initial-index="0"
                  fit="contain"
                  hide-on-click-modal
                  preview-teleported
                  class="snapshot-img"
                />
                <div v-else class="no-media">该事件没有截图</div>
              </div>
            </div>

            <el-descriptions :column="2" border class="info">
              <el-descriptions-item label="车牌 (识别)">
                {{ selected.plate_number }}
              </el-descriptions-item>
              <el-descriptions-item label="人工修正">
                {{ selected.plate_manual_corrected ?? "—" }}
              </el-descriptions-item>
              <el-descriptions-item label="车型">
                {{ selected.vehicle_class }}
              </el-descriptions-item>
              <el-descriptions-item label="车牌置信度">
                {{ (selected.plate_confidence * 100).toFixed(1) }}%
              </el-descriptions-item>
              <el-descriptions-item label="IoU 占人行道率">
                {{ selected.iou_score !== null ? (selected.iou_score * 100).toFixed(1) + '%' : '—' }}
              </el-descriptions-item>
              <el-descriptions-item label="时间窗">
                {{ (selected.first_seen_ms / 1000).toFixed(1) }}s ~ {{ (selected.last_seen_ms / 1000).toFixed(1) }}s ({{ selected.frame_hits }} 帧)
              </el-descriptions-item>
              <el-descriptions-item label="拍摄时间" :span="2">
                {{ selected.event_time ?? "—" }}
              </el-descriptions-item>
              <el-descriptions-item label="源视频" :span="2">
                {{ selected.source_video }}
              </el-descriptions-item>
            </el-descriptions>

            <div class="plate-edit">
              <el-input
                v-model="editingPlate"
                placeholder="车牌识别错误? 在此修正后回车保存"
                clearable
                @change="applyPlateCorrection"
                style="max-width: 240px"
              >
                <template #prepend>修正车牌</template>
              </el-input>
              <el-button @click="applyPlateCorrection">保存修正</el-button>
              <el-tag v-if="(plateOccurrences.get(selected.plate_number) ?? 0) > 1" type="warning">
                同车牌「{{ selected.plate_number }}」在 {{ plateOccurrences.get(selected.plate_number) }} 个视频出现
              </el-tag>
            </div>

            <div class="actions">
              <el-button type="success" @click="applyStatus('accepted')">↑ 采纳</el-button>
              <el-button type="danger" @click="applyStatus('rejected')">↓ 丢弃</el-button>
              <el-button type="warning" @click="applyStatus('deferred')">D 待定</el-button>
              <el-divider direction="vertical" />
              <el-button @click="undo">U 撤销</el-button>
              <el-button @click="prev">← 上一个</el-button>
              <el-button @click="next">下一个 →</el-button>
              <el-button v-if="evidenceFolder(selected)" @click="openEvidence(selected)">
                打开证据文件夹
              </el-button>
            </div>
          </template>
        </div>
      </div>
    </el-card>
  </div>
</template>

<style scoped>
.review {
  max-width: 1500px;
}

.card-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 8px;
}

h2 {
  margin: 0;
  font-size: 18px;
}

.shortcuts-hint {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}

.stats {
  margin-bottom: 16px;
}

.filters {
  display: flex;
  gap: 12px;
  align-items: center;
  margin: 16px 0;
  flex-wrap: wrap;
}

.layout {
  display: grid;
  grid-template-columns: 320px 1fr;
  gap: 16px;
  height: calc(100vh - 320px);
  min-height: 480px;
}

.list {
  overflow-y: auto;
  border: 1px solid var(--el-border-color-light);
  border-radius: 4px;
  background: var(--el-bg-color-page);
}

.list-item {
  display: flex;
  gap: 8px;
  padding: 8px;
  border-bottom: 1px solid var(--el-border-color-lighter);
  cursor: pointer;
}

.list-item:hover {
  background: var(--el-color-info-light-9);
}

.list-item.active {
  background: var(--el-color-primary-light-9);
}

.thumb {
  width: 80px;
  height: 56px;
  object-fit: cover;
  border-radius: 4px;
  background: #000;
}

.thumb-empty {
  width: 80px;
  height: 56px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--el-color-info-light-7);
  color: var(--el-text-color-secondary);
  font-size: 11px;
  border-radius: 4px;
}

.meta {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.meta-row {
  display: flex;
  gap: 6px;
  align-items: center;
  flex-wrap: wrap;
}

.meta-row.sub {
  font-size: 11px;
  color: var(--el-text-color-secondary);
}

.cross-video {
  color: var(--el-color-warning);
  font-weight: 600;
}

.detail {
  display: flex;
  flex-direction: column;
  gap: 12px;
  overflow-y: auto;
}

.media-row {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}

.media-block {
  position: relative;
  width: 100%;
  background: #000;
  border-radius: 6px;
  aspect-ratio: 16 / 9;
  overflow: hidden;
}

.video-block video {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: contain;
}

.video-block .overlay {
  position: absolute;
  inset: 0;
  pointer-events: none;
}

.fullscreen-btn {
  position: absolute;
  top: 8px;
  right: 8px;
  z-index: 5;
}

.snapshot-block {
  cursor: zoom-in;
}

.snapshot-img {
  width: 100%;
  height: 100%;
  display: block;
}

.snapshot-img :deep(img) {
  width: 100%;
  height: 100%;
  object-fit: contain;
  display: block;
}

.no-media {
  display: flex;
  align-items: center;
  justify-content: center;
  color: #fff;
  height: 100%;
  font-size: 13px;
}

.plate-edit {
  display: flex;
  gap: 12px;
  align-items: center;
  flex-wrap: wrap;
}

.actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}
</style>
