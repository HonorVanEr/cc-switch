import { getVersion } from "@tauri-apps/api/app";

export type UpdateChannel = "stable" | "beta";

export interface UpdateInfo {
  currentVersion: string;
  availableVersion: string;
  notes?: string;
  pubDate?: string;
}

export interface CheckOptions {
  timeout?: number;
  channel?: UpdateChannel;
}

export async function getCurrentVersion(): Promise<string> {
  try {
    return await getVersion();
  } catch {
    return "";
  }
}

export async function checkForUpdate(
  _opts: CheckOptions = {},
): Promise<
  { status: "up-to-date" } | { status: "available"; info: UpdateInfo }
> {
  const { checkUpdate } = await import("@tauri-apps/api/updater");

  const currentVersion = await getCurrentVersion();
  const result = await checkUpdate();

  if (!result.shouldUpdate || !result.manifest) {
    return { status: "up-to-date" };
  }

  const manifest = result.manifest;
  const info: UpdateInfo = {
    currentVersion,
    availableVersion: manifest.version ?? "",
    notes: manifest.body ?? undefined,
    pubDate: manifest.date ?? undefined,
  };

  return { status: "available", info };
}

/**
 * 下载并安装可用更新，然后重启应用（Tauri v1 前端直连方式）。
 * 无可用更新时返回 false；成功安装后应用会重启，通常不会返回。
 */
export async function installUpdateAndRestart(): Promise<boolean> {
  const { checkUpdate, installUpdate } = await import(
    "@tauri-apps/api/updater"
  );
  const { relaunch } = await import("@tauri-apps/api/process");

  const result = await checkUpdate();
  if (!result.shouldUpdate || !result.manifest) {
    return false;
  }

  await installUpdate();
  await relaunch();
  return true;
}
