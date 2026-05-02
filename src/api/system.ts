import { invoke } from "@tauri-apps/api/core";

/** 单个模型的状态报告, 与 Rust ModelStatus 一一对应 */
export interface ModelStatus {
  name: string;
  ready: boolean;
  path: string;
  size_bytes: number | null;
  error: string | null;
}

/** 系统整体状态, 与 Rust SystemStatus 对应 */
export interface SystemStatus {
  models: ModelStatus[];
  ort_dylib_path: string | null;
  app_version: string;
}

/** 调用后端 check_system_status 命令 */
export async function checkSystemStatus(): Promise<SystemStatus> {
  return invoke<SystemStatus>("check_system_status");
}

/** 在 Finder 打开证据文件夹 (P3) */
export async function openInFileManager(path: string): Promise<void> {
  await invoke("open_in_file_manager", { path });
}
