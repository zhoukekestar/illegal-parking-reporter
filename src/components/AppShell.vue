<script setup lang="ts">
import { computed, ref, onMounted } from "vue";
import { useRoute, useRouter } from "vue-router";
import { ElMessage } from "element-plus";

import { authApi } from "@/api/auth";

const router = useRouter();
const route = useRoute();
const hasPassword = ref(false);

const activeMenu = computed(() => route.path);

interface MenuItem {
  path: string;
  title: string;
}

const items: MenuItem[] = [
  { path: "/home", title: "首页" },
  { path: "/processing", title: "批量处理 (P2)" },
  { path: "/review", title: "审核 (P4)" },
  { path: "/export", title: "导出 (P5)" },
  { path: "/events", title: "事件列表" },
  { path: "/process", title: "单视频调试 (P1)" },
  { path: "/detect", title: "图片检测 (P0)" },
  { path: "/settings", title: "设置" },
  { path: "/about", title: "关于" },
];

function go(path: string) {
  if (path !== route.path) {
    router.push(path);
  }
}

onMounted(async () => {
  try {
    const a = await authApi.state();
    hasPassword.value = a.has_password;
  } catch (_) {}
});

async function lock() {
  if (!hasPassword.value) {
    ElMessage.info("请先在设置中设置密码再锁定");
    return;
  }
  try {
    await authApi.lock();
    location.reload();
  } catch (e) {
    ElMessage.error(String(e));
  }
}
</script>

<template>
  <el-container class="shell">
    <el-aside width="200px" class="aside">
      <div class="logo">
        <div class="logo-name">路况记录助手</div>
        <div class="logo-stage">P0 → P6 (本地优先 / 不联网)</div>
      </div>
      <el-menu :default-active="activeMenu" class="menu" @select="go">
        <el-menu-item v-for="it in items" :key="it.path" :index="it.path">
          {{ it.title }}
        </el-menu-item>
      </el-menu>
      <div class="footer">
        <el-button v-if="hasPassword" size="small" @click="lock">锁定</el-button>
      </div>
    </el-aside>
    <el-container direction="vertical">
      <el-alert
        type="warning"
        :closable="false"
        show-icon
        title="合法性提醒"
        description="不得在驾驶过程中拍摄, 不得进入机动车道, 举报有效期 ≤ 拍摄后 72 小时。本软件仅辅助识别, 用户对最终提交内容负责。"
        class="legal-banner"
      />
      <el-main class="main">
        <router-view />
      </el-main>
    </el-container>
  </el-container>
</template>

<style scoped>
.shell {
  height: 100vh;
}

.aside {
  background: var(--el-bg-color-page);
  border-right: 1px solid var(--el-border-color-light);
  display: flex;
  flex-direction: column;
}

.logo {
  padding: 16px 20px;
  border-bottom: 1px solid var(--el-border-color-light);
}

.logo-name {
  font-size: 16px;
  font-weight: 600;
  color: var(--el-text-color-primary);
}

.logo-stage {
  margin-top: 4px;
  font-size: 12px;
  color: var(--el-text-color-secondary);
}

.menu {
  border-right: none;
  flex: 1;
}

.footer {
  padding: 12px 16px;
  border-top: 1px solid var(--el-border-color-light);
}

.legal-banner {
  border-radius: 0;
}

.main {
  background: var(--el-bg-color);
  padding: 24px;
  overflow: auto;
}
</style>
