import { invoke } from "@tauri-apps/api/core";

export interface AuthState {
  has_password: boolean;
  unlocked: boolean;
}

export interface AppSettings {
  iou_threshold: number;
  clip_pre_secs: number;
  clip_post_secs: number;
  sample_fps: number;
  plate_conf_threshold: number;
  aggregate_window_secs: number;
  first_run_done: boolean;
}

export const authApi = {
  state: () => invoke<AuthState>("auth_state"),
  setPassword: (oldPassword: string | null, newPassword: string) =>
    invoke<void>("set_password", { oldPassword, newPassword }),
  unlock: (password: string) => invoke<boolean>("unlock", { password }),
  lock: () => invoke<void>("lock"),

  getSettings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) => invoke<void>("save_settings", { settings }),

  purgeData: () => invoke<void>("purge_data"),
};
