import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import type { Session } from "@/lib/types";

let permission: boolean | null = null;
let asking = false;

export const ensurePermission = async (): Promise<boolean> => {
  if (permission !== null) return permission;
  try {
    if (await isPermissionGranted()) {
      permission = true;
      return true;
    }
    if (asking) return false;
    asking = true;
    const result = await requestPermission();
    permission = result === "granted";
    if (permission) asking = false;
    return permission;
  } catch {
    return false;
  }
};

export const notifyWaiting = async (session: Session): Promise<boolean> => {
  if (!(await ensurePermission())) return false;
  try {
    sendNotification({
      title: `${session.project} butuh jawaban`,
      body: `${session.model ?? session.agent} · menunggu input`,
    });
    return true;
  } catch {
    return false;
  }
};
