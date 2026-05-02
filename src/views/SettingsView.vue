<script setup lang="ts">
import { onMounted, ref, computed } from "vue";
import { ElMessage, ElMessageBox } from "element-plus";

import { useSettingsStore } from "@/stores/settings";
import { authApi, type AppSettings, type AuthState } from "@/api/auth";

const store = useSettingsStore();

const auth = ref<AuthState | null>(null);
const settings = ref<AppSettings | null>(null);
const settingsDirty = ref(false);
const oldPwd = ref("");
const newPwd = ref("");
const newPwd2 = ref("");

onMounted(async () => {
  await store.refresh();
  await loadAll();
});

async function loadAll() {
  try {
    auth.value = await authApi.state();
    settings.value = await authApi.getSettings();
    settingsDirty.value = false;
  } catch (e) {
    ElMessage.error(String(e));
  }
}

async function saveSettings() {
  if (!settings.value) return;
  try {
    await authApi.saveSettings(settings.value);
    settingsDirty.value = false;
    ElMessage.success("设置已保存");
  } catch (e) {
    ElMessage.error(String(e));
  }
}

async function setPassword() {
  if (!auth.value) return;
  if (newPwd.value !== newPwd2.value) {
    ElMessage.warning("两次密码不一致");
    return;
  }
  if (!newPwd.value) {
    ElMessage.warning("新密码不能为空");
    return;
  }
  try {
    await authApi.setPassword(auth.value.has_password ? oldPwd.value : null, newPwd.value);
    ElMessage.success("密码已设置");
    oldPwd.value = "";
    newPwd.value = "";
    newPwd2.value = "";
    auth.value = await authApi.state();
  } catch (e) {
    ElMessage.error(String(e));
  }
}

async function purgeData() {
  await ElMessageBox.confirm(
    "确认清空所有事件 + 任务 + 证据包? 此操作不可恢复",
    "高危操作",
    { type: "warning", confirmButtonText: "确认清空", cancelButtonText: "取消" }
  );
  try {
    await authApi.purgeData();
    ElMessage.success("已清空所有数据");
    location.reload();
  } catch (e) {
    ElMessage.error(String(e));
  }
}

const ortReady = computed(() => !!store.status?.ort_dylib_path);

function formatBytes(n: number | null): string {
  if (!n) return "—";
  const mb = n / 1024 / 1024;
  if (mb >= 1) return `${mb.toFixed(2)} MB`;
  return `${(n / 1024).toFixed(1)} KB`;
}

function markDirty() {
  settingsDirty.value = true;
}
</script>

<template>
  <div class="settings">
    <el-card>
      <template #header>
        <div class="card-header">
          <h2>设置</h2>
          <el-button @click="loadAll">刷新</el-button>
        </div>
      </template>

      <!-- 模型状态 -->
      <el-divider content-position="left">运行时 / 模型</el-divider>
      <el-skeleton v-if="store.loading" :rows="4" animated />
      <template v-else-if="store.status">
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
        <el-table :data="store.status.models" stripe class="models-table">
          <el-table-column prop="name" label="名称" width="180" />
          <el-table-column label="状态" width="100">
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
        </el-table>
      </template>

      <!-- Pipeline 参数 -->
      <el-divider content-position="left">Pipeline 参数</el-divider>
      <template v-if="settings">
        <el-form label-width="180px" class="form">
          <el-form-item label="人行道占用阈值 (IoU)">
            <el-slider
              v-model="settings.iou_threshold"
              :min="0.1"
              :max="0.7"
              :step="0.05"
              show-input
              @change="markDirty"
              style="max-width: 460px"
            />
            <el-text size="small" class="hint">高于阈值判定违停 (默认 0.3 = 30%)</el-text>
          </el-form-item>
          <el-form-item label="片段时长 (前秒)">
            <el-input-number v-model="settings.clip_pre_secs" :min="0" :max="10" :step="0.5" @change="markDirty" />
          </el-form-item>
          <el-form-item label="片段时长 (后秒)">
            <el-input-number v-model="settings.clip_post_secs" :min="0" :max="10" :step="0.5" @change="markDirty" />
          </el-form-item>
          <el-form-item label="抽帧频率 (fps)">
            <el-input-number v-model="settings.sample_fps" :min="0.5" :max="5" :step="0.5" @change="markDirty" />
          </el-form-item>
          <el-form-item label="车牌置信度阈值">
            <el-slider
              v-model="settings.plate_conf_threshold"
              :min="0.4"
              :max="0.9"
              :step="0.05"
              show-input
              @change="markDirty"
              style="max-width: 460px"
            />
          </el-form-item>
          <el-form-item label="事件聚合窗口 (秒)">
            <el-input-number
              v-model="settings.aggregate_window_secs"
              :min="10"
              :max="600"
              :step="5"
              @change="markDirty"
            />
          </el-form-item>
          <el-form-item>
            <el-button
              type="primary"
              :disabled="!settingsDirty"
              @click="saveSettings"
            >
              保存
            </el-button>
            <el-text v-if="settingsDirty" size="small" class="hint" type="warning">
              有未保存的修改
            </el-text>
            <el-text v-else size="small" class="hint">注: 部分参数需要重启应用或重新处理才生效</el-text>
          </el-form-item>
        </el-form>
      </template>

      <!-- 密码 / 锁定 -->
      <el-divider content-position="left">本地账号</el-divider>
      <template v-if="auth">
        <el-alert
          v-if="!auth.has_password"
          type="info"
          :closable="false"
          show-icon
          title="未设置启动密码"
          description="数据库当前用本机随机 secret 加密。设置密码后, 启动需要输入密码解锁。"
        />
        <el-form label-width="180px" class="form" style="margin-top: 16px">
          <el-form-item v-if="auth.has_password" label="旧密码">
            <el-input v-model="oldPwd" type="password" show-password style="max-width: 320px" />
          </el-form-item>
          <el-form-item label="新密码">
            <el-input v-model="newPwd" type="password" show-password style="max-width: 320px" />
          </el-form-item>
          <el-form-item label="再次输入">
            <el-input v-model="newPwd2" type="password" show-password style="max-width: 320px" />
          </el-form-item>
          <el-form-item>
            <el-button type="primary" @click="setPassword">
              {{ auth.has_password ? '修改密码' : '设置密码' }}
            </el-button>
          </el-form-item>
        </el-form>
      </template>

      <!-- 危险区 -->
      <el-divider content-position="left">危险操作</el-divider>
      <el-button type="danger" @click="purgeData">清空所有事件 + 任务 + 证据</el-button>
      <el-text size="small" class="hint">删除 events/video_jobs/settings/evidence 目录, 不删模型, 不删密码</el-text>
    </el-card>
  </div>
</template>

<style scoped>
.settings {
  max-width: 1100px;
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
.models-table {
  margin-top: 12px;
}
.form {
  max-width: 720px;
  margin-top: 12px;
}
.hint {
  margin-left: 12px;
  color: var(--el-text-color-secondary);
}
</style>
