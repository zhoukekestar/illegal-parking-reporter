<script setup lang="ts">
import { ref } from "vue";
import { ElMessage } from "element-plus";

import { authApi } from "@/api/auth";

const emit = defineEmits<{
  done: [];
}>();

const step = ref(0);
const readLegal = ref(false);
const enablePassword = ref(false);
const newPwd = ref("");
const confirmPwd = ref("");
const submitting = ref(false);

async function finish() {
  if (!readLegal.value) {
    ElMessage.warning("必须勾选已阅读使用约定");
    return;
  }
  submitting.value = true;
  try {
    if (enablePassword.value) {
      if (!newPwd.value || newPwd.value !== confirmPwd.value) {
        ElMessage.warning("两次密码必须一致");
        submitting.value = false;
        return;
      }
      await authApi.setPassword(null, newPwd.value);
    }
    const settings = await authApi.getSettings();
    settings.first_run_done = true;
    await authApi.saveSettings(settings);
    emit("done");
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    submitting.value = false;
  }
}
</script>

<template>
  <div class="wizard">
    <el-card class="card">
      <template #header>
        <h2>欢迎使用路况记录助手</h2>
        <p class="subtitle">3 步完成首次启动配置 ({{ step + 1 }} / 3)</p>
      </template>

      <el-steps :active="step" finish-status="success" simple>
        <el-step title="使用约定" />
        <el-step title="模型 & 数据" />
        <el-step title="是否设密码" />
      </el-steps>

      <div v-if="step === 0" class="step">
        <h3>合法性与免责</h3>
        <ul class="legal">
          <li>本软件仅辅助识别违停车辆, 生成证据包, <strong>不替你举报</strong></li>
          <li><strong>不得</strong>在驾驶过程中拍摄</li>
          <li><strong>不得</strong>进入机动车道拍摄</li>
          <li>举报有效期: 拍摄后 ≤ 72 小时</li>
          <li>软件不保证 100% 识别准确, 用户对最终提交内容负责; 误报/虚假举报责任由用户承担</li>
          <li>所有数据仅在本机处理, 不联网, 不上传第三方</li>
        </ul>
        <el-checkbox v-model="readLegal">
          我已阅读并同意以上约定
        </el-checkbox>
      </div>

      <div v-if="step === 1" class="step">
        <h3>模型与数据存储</h3>
        <p>软件需要 4 个本地 ONNX 模型:</p>
        <ul>
          <li><code>yolov8n-seg.onnx</code> 车辆检测 + 实例分割</li>
          <li><code>hyperlpr3/y5fu_320x_sim.onnx</code> 车牌检测</li>
          <li><code>hyperlpr3/rpv3_mdict_160_r3.onnx</code> 车牌识别</li>
          <li><code>segformer/model.onnx</code> 人行道分割</li>
        </ul>
        <p>详见 <code>docs/MODELS.md</code>。模型文件放在 <code>src-tauri/models/</code> 下。</p>
        <p>
          数据库存储在 <code>~/Library/Application Support/路况记录助手/parking.sqlite</code>
          (开发模式: <code>src-tauri/.local/parking.sqlite</code>),
          <strong>SQLCipher 加密</strong>, 文本工具打开为二进制。
        </p>
      </div>

      <div v-if="step === 2" class="step">
        <h3>是否设置启动密码?</h3>
        <p>不设置也可使用 (数据库仍加密, 默认密钥从本机 secret 派生)。</p>
        <el-checkbox v-model="enablePassword">设置启动密码</el-checkbox>
        <template v-if="enablePassword">
          <el-input
            v-model="newPwd"
            type="password"
            placeholder="新密码"
            show-password
            class="pwd-input"
          />
          <el-input
            v-model="confirmPwd"
            type="password"
            placeholder="再次输入密码"
            show-password
            class="pwd-input"
          />
        </template>
      </div>

      <div class="actions">
        <el-button v-if="step > 0" @click="step--">上一步</el-button>
        <el-button
          v-if="step < 2"
          type="primary"
          :disabled="step === 0 && !readLegal"
          @click="step++"
        >
          下一步
        </el-button>
        <el-button
          v-else
          type="success"
          :loading="submitting"
          @click="finish"
        >
          开始使用
        </el-button>
      </div>
    </el-card>
  </div>
</template>

<style scoped>
.wizard {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 100vh;
  background: var(--el-bg-color-page);
  padding: 24px;
}
.card {
  width: 580px;
  max-width: 100%;
}
h2 { margin: 0; font-size: 20px; }
.subtitle {
  margin: 6px 0 0;
  color: var(--el-text-color-secondary);
  font-size: 12px;
}
.step {
  margin: 24px 0;
}
.step h3 {
  font-size: 16px;
  margin: 0 0 12px;
}
.legal {
  background: #fff8e1;
  border-left: 4px solid #ffb300;
  padding: 12px 18px;
  border-radius: 4px;
  margin: 12px 0;
}
.legal li {
  margin: 6px 0;
  line-height: 1.7;
}
.pwd-input {
  margin-top: 8px;
  max-width: 320px;
}
.actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
  margin-top: 16px;
}
code {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  background: var(--el-color-info-light-9);
  padding: 1px 6px;
  border-radius: 3px;
  font-size: 12px;
}
</style>
