<script setup lang="ts">
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { ElMessage } from "element-plus";

import { openInFileManager } from "@/api/system";

interface DiagnosticReport {
  bundle_path: string;
  size_bytes: number;
  log_files_included: number;
  failed_jobs_count: number;
}

const exporting = ref(false);
const lastReport = ref<DiagnosticReport | null>(null);

async function exportDiag() {
  const folder = await open({ multiple: false, directory: true });
  if (typeof folder !== "string") return;
  exporting.value = true;
  try {
    lastReport.value = await invoke<DiagnosticReport>("export_diagnostic", {
      targetDir: folder,
    });
    ElMessage.success(`诊断包已导出: ${formatSize(lastReport.value.size_bytes)}`);
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    exporting.value = false;
  }
}

async function openZip() {
  if (!lastReport.value) return;
  await openInFileManager(lastReport.value.bundle_path);
}

function formatSize(n: number): string {
  if (n > 1e6) return `${(n / 1e6).toFixed(2)} MB`;
  if (n > 1e3) return `${(n / 1e3).toFixed(1)} KB`;
  return `${n} B`;
}
</script>

<template>
  <div class="about">
    <el-card>
      <template #header>
        <h2>关于</h2>
      </template>

      <el-descriptions :column="1" border class="info">
        <el-descriptions-item label="软件名称">路况记录助手</el-descriptions-item>
        <el-descriptions-item label="定位">桌面端违停证据包生成器</el-descriptions-item>
        <el-descriptions-item label="许可证">AGPL-3.0 (受 YOLOv8 传染)</el-descriptions-item>
        <el-descriptions-item label="技术栈">
          Tauri 2 + Vue 3 + ort + ffmpeg-next + SQLCipher
        </el-descriptions-item>
        <el-descriptions-item label="平台">macOS Apple Silicon 优先</el-descriptions-item>
        <el-descriptions-item label="数据隐私">
          全部本地处理, 不联网, 不上传第三方
        </el-descriptions-item>
      </el-descriptions>

      <el-divider content-position="left">使用提醒</el-divider>
      <ul class="reminders">
        <li>不得在驾驶过程中拍摄</li>
        <li>不得进入机动车道拍摄</li>
        <li>举报有效期 ≤ 拍摄后 72 小时</li>
        <li>软件仅辅助识别, 用户对最终提交内容负责</li>
        <li>所有数据本地加密 (SQLCipher) 存储</li>
      </ul>

      <el-divider content-position="left">诊断</el-divider>
      <p class="hint">
        遇到问题时可导出诊断包 (zip) 反馈。
        包含日志 / 模型清单 / 系统信息 / 失败任务统计 (脱敏车牌)。
      </p>
      <el-button type="primary" :loading="exporting" @click="exportDiag">
        选择目录并导出诊断包
      </el-button>

      <el-descriptions v-if="lastReport" :column="1" border class="result">
        <el-descriptions-item label="文件">
          <code>{{ lastReport.bundle_path }}</code>
        </el-descriptions-item>
        <el-descriptions-item label="大小">
          {{ formatSize(lastReport.size_bytes) }}
        </el-descriptions-item>
        <el-descriptions-item label="包含日志文件数">
          {{ lastReport.log_files_included }}
        </el-descriptions-item>
        <el-descriptions-item label="失败任务数">
          {{ lastReport.failed_jobs_count }}
        </el-descriptions-item>
      </el-descriptions>
      <el-button v-if="lastReport" @click="openZip">在 Finder 显示</el-button>

      <el-divider content-position="left">阶段进度 (P0 → P7)</el-divider>
      <el-timeline>
        <el-timeline-item type="success">P0 工程脚手架 (Tauri + Vue + ort)</el-timeline-item>
        <el-timeline-item type="success">P1 单视频识别 pipeline (ffmpeg + HyperLPR3)</el-timeline-item>
        <el-timeline-item type="success">P2 批量并发流水线 (tokio + 续跑)</el-timeline-item>
        <el-timeline-item type="success">P3 证据生成 (YOLOv8-seg + SegFormer + IoU)</el-timeline-item>
        <el-timeline-item type="success">P4 审核 UI (键盘流 + 撤销 + 修正)</el-timeline-item>
        <el-timeline-item type="success">P5 导出 + 索引 (CSV + HTML 上传指引)</el-timeline-item>
        <el-timeline-item type="success">P6 本地登录 + 设置 (SQLCipher + argon2)</el-timeline-item>
        <el-timeline-item type="primary">P7 打磨 + 诊断 (本阶段)</el-timeline-item>
        <el-timeline-item type="info">P8 iPhone Mirroring 助手 (加分项)</el-timeline-item>
      </el-timeline>
    </el-card>
  </div>
</template>

<style scoped>
.about {
  max-width: 900px;
}
h2 {
  margin: 0;
  font-size: 18px;
}
.info {
  margin-top: 8px;
}
.reminders {
  background: #fff8e1;
  border-left: 4px solid #ffb300;
  padding: 12px 18px;
  border-radius: 4px;
}
.reminders li {
  margin: 4px 0;
}
.hint {
  color: var(--el-text-color-secondary);
  font-size: 13px;
  margin: 8px 0;
}
.result {
  margin: 12px 0;
}
code {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
  word-break: break-all;
}
</style>
