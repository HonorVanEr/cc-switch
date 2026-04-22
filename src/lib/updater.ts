import { getVersion } from "@tauri-apps/api/app";

export type UpdateChannel = "stable" | "beta";

export type UpdaterPhase =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "installing"
  | "restarting"
  | "upToDate"
  | "error";

export interface UpdateInfo {
  currentVersion: string;
  availableVersion: string;
  notes?: string;
  pubDate?: string;
}

export interface UpdateProgressEvent {
  event: "Started" | "Progress" | "Finished";
  total?: number;
  downloaded?: number;
}

export interface UpdateHandle {
  version: string;
  notes?: string;
  date?: string;
  downloadAndInstall: (
    onProgress?: (e: UpdateProgressEvent) => void,
  ) => Promise<void>;
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
  | { status: "up-to-date" }
  | { status: "available"; info: UpdateInfo; update: UpdateHandle }
> {
  const { checkUpdate, installUpdate } = await import("@tauri-apps/api/updater");

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

  const updateHandle: UpdateHandle = {
    version: manifest.version ?? "",
    notes: manifest.body ?? undefined,
    date: manifest.date ?? undefined,
    async downloadAndInstall(_onProgress?: (e: UpdateProgressEvent) => void) {
      await installUpdate();
    },
  };

  return { status: "available", info, update: updateHandle };
}

export async function relaunchApp(): Promise<void> {
  const { relaunch } = await import("@tauri-apps/api/process");
  await relaunch();
}
