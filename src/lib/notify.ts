import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { t } from "@/i18n";
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
      title: t("notify.waiting.title", { project: session.project }),
      body: t("notify.waiting.body", { who: session.model ?? session.agent }),
    });
    return true;
  } catch {
    return false;
  }
};

export const notifyStalled = async (session: Session): Promise<boolean> => {
  if (!(await ensurePermission())) return false;
  try {
    sendNotification({
      title: t("notify.stalled.title", { project: session.project }),
      body: session.healthReason ?? t("notify.stalled.bodyFallback", { who: session.model ?? session.agent }),
    });
    return true;
  } catch {
    return false;
  }
};
