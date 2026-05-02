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
    meta: { title: "单视频调试", icon: "VideoPlay" },
  },
  {
    path: "/processing",
    name: "processing",
    component: () => import("@/views/ProcessingView.vue"),
    meta: { title: "批量处理", icon: "Loading" },
  },
  {
    path: "/review",
    name: "review",
    component: () => import("@/views/ReviewView.vue"),
    meta: { title: "审核 (P4)", icon: "View" },
  },
  {
    path: "/export",
    name: "export",
    component: () => import("@/views/ExportView.vue"),
    meta: { title: "导出 (P5)", icon: "Download" },
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
  {
    path: "/upload-helper",
    name: "upload-helper",
    component: () => import("@/views/UploadHelperView.vue"),
    meta: { title: "上传助手 (P8.1)", icon: "Upload" },
  },
  {
    path: "/about",
    name: "about",
    component: () => import("@/views/AboutView.vue"),
    meta: { title: "关于", icon: "InfoFilled" },
  },
];

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
});

export default router;
