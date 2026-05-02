<script setup lang="ts">
import { computed } from "vue";
import { useRoute, useRouter } from "vue-router";

const router = useRouter();
const route = useRoute();

const activeMenu = computed(() => route.path);

interface MenuItem {
  path: string;
  title: string;
}

const items: MenuItem[] = [
  { path: "/home", title: "首页" },
  { path: "/detect", title: "图片检测" },
  { path: "/settings", title: "设置" },
];

function go(path: string) {
  if (path !== route.path) {
    router.push(path);
  }
}
</script>

<template>
  <el-container class="shell">
    <el-aside width="200px" class="aside">
      <div class="logo">
        <div class="logo-name">路况记录助手</div>
        <div class="logo-stage">P0 / 工程脚手架</div>
      </div>
      <el-menu :default-active="activeMenu" class="menu" @select="go">
        <el-menu-item v-for="it in items" :key="it.path" :index="it.path">
          {{ it.title }}
        </el-menu-item>
      </el-menu>
    </el-aside>
    <el-main class="main">
      <router-view />
    </el-main>
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

.main {
  background: var(--el-bg-color);
  padding: 24px;
  overflow: auto;
}
</style>
