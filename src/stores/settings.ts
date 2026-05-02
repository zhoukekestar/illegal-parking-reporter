import { defineStore } from "pinia";
import { ref } from "vue";

import { checkSystemStatus, type SystemStatus } from "@/api/system";

/**
 * 设置 / 系统状态 store
 * P0 阶段只缓存 SystemStatus, P6 起会扩到完整设置项
 */
export const useSettingsStore = defineStore("settings", () => {
  const status = ref<SystemStatus | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function refresh() {
    loading.value = true;
    error.value = null;
    try {
      status.value = await checkSystemStatus();
    } catch (e) {
      error.value = String(e);
    } finally {
      loading.value = false;
    }
  }

  return { status, loading, error, refresh };
});
