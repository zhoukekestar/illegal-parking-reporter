import { createRouter, createWebHashHistory, type RouteRecordRaw } from "vue-router";

// Tauri 默认走 hash 路由, 避免 file:// 协议下的路径问题
const routes: RouteRecordRaw[] = [
  {
    path: "/",
    redirect: "/home",
  },
  {
    path: "/home",
    name: "home",
    component: () => import("@/views/HomeView.vue"),
    meta: { title: "首页", icon: "House" },
  },
  {
    path: "/detect",
    name: "detect",
    component: () => import("@/views/UploadView.vue"),
    meta: { title: "图片检测", icon: "Picture" },
  },
  {
    path: "/process",
    name: "process",
    component: () => import("@/views/ProcessView.vue"),
    meta: { title: "视频处理", icon: "VideoPlay" },
  },
  {
    path: "/events",
    name: "events",
    component: () => import("@/views/EventsView.vue"),
    meta: { title: "事件列表", icon: "List" },
  },
  {
    path: "/settings",
    name: "settings",
    component: () => import("@/views/SettingsView.vue"),
    meta: { title: "设置", icon: "Setting" },
  },
];

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
});

export default router;
