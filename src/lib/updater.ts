import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";

export interface FoundUpdate {
  version: string;
  notes: string | null;
  install: (onProgress: (p: number) => void) => Promise<void>;
  /** Releases the underlying plugin resource. Safe to call once, before the next check. */
  dispose: () => Promise<void>;
}

export const findUpdate = async (): Promise<FoundUpdate | null> => {
  const update = await check();
  if (!update) return null;
  return {
    version: update.version,
    notes: update.body ?? null,
    install: async (onProgress) => {
      let total = 0;
      let received = 0;
      await update.downloadAndInstall((e) => {
        if (e.event === "Started") total = e.data.contentLength ?? 0;
        if (e.event === "Progress") {
          received += e.data.chunkLength;
          if (total > 0) onProgress(received / total);
        }
        if (e.event === "Finished") onProgress(1);
      });
    },
    dispose: () => update.close(),
  };
};

export const restartApp = () => relaunch();
export const currentVersion = () => getVersion();
