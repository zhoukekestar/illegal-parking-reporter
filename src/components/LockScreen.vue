<script setup lang="ts">
import { ref } from "vue";
import { ElMessage } from "element-plus";

import { authApi } from "@/api/auth";

const emit = defineEmits<{
  unlocked: [];
}>();

const password = ref("");
const submitting = ref(false);

async function tryUnlock() {
  if (!password.value) {
    ElMessage.warning("请输入密码");
    return;
  }
  submitting.value = true;
  try {
    const ok = await authApi.unlock(password.value);
    if (ok) {
      emit("unlocked");
    } else {
      ElMessage.error("密码错误");
      password.value = "";
    }
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    submitting.value = false;
  }
}
</script>

<template>
  <div class="lock-screen">
    <el-card class="card">
      <template #header>
        <h2>路况记录助手 - 已锁定</h2>
        <p class="subtitle">输入密码解锁本次会话</p>
      </template>
      <el-input
        v-model="password"
        type="password"
        placeholder="密码"
        show-password
        @keyup.enter="tryUnlock"
        autofocus
      />
      <el-button
        class="btn"
        type="primary"
        :loading="submitting"
        @click="tryUnlock"
        block
      >
        解锁
      </el-button>
      <el-text class="hint" size="small">
        密码仅在本机 argon2 哈希存储, 不联网, 忘记密码可手动删除 auth.json (会丢加密数据)
      </el-text>
    </el-card>
  </div>
</template>

<style scoped>
.lock-screen {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100vh;
  background: var(--el-bg-color-page);
}
.card {
  width: 380px;
}
h2 {
  margin: 0;
  font-size: 18px;
}
.subtitle {
  margin: 6px 0 0;
  color: var(--el-text-color-secondary);
  font-size: 12px;
}
.btn {
  margin-top: 12px;
  width: 100%;
}
.hint {
  display: block;
  margin-top: 12px;
  color: var(--el-text-color-secondary);
}
</style>
