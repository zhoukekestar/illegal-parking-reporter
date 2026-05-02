<script setup lang="ts">
import { ref, onMounted } from "vue";

import AppShell from "@/components/AppShell.vue";
import LockScreen from "@/components/LockScreen.vue";
import FirstRunWizard from "@/components/FirstRunWizard.vue";
import { authApi } from "@/api/auth";

type AppState = "loading" | "wizard" | "locked" | "ready";

const state = ref<AppState>("loading");

onMounted(async () => {
  await refresh();
});

async function refresh() {
  try {
    const settings = await authApi.getSettings();
    if (!settings.first_run_done) {
      state.value = "wizard";
      return;
    }
    const auth = await authApi.state();
    if (auth.has_password && !auth.unlocked) {
      state.value = "locked";
      return;
    }
    state.value = "ready";
  } catch (_e) {
    // DB / 后端没就绪时也走 wizard
    state.value = "wizard";
  }
}
</script>

<template>
  <FirstRunWizard v-if="state === 'wizard'" @done="refresh" />
  <LockScreen v-else-if="state === 'locked'" @unlocked="refresh" />
  <AppShell v-else-if="state === 'ready'" />
  <div v-else class="loading">加载中...</div>
</template>

<style>
html,
body,
#app {
  margin: 0;
  padding: 0;
  height: 100%;
  font-family: -apple-system, "PingFang SC", "Microsoft YaHei", "Segoe UI", Roboto,
    "Helvetica Neue", Arial, sans-serif;
}
.loading {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100vh;
  color: var(--el-text-color-secondary);
}
</style>
