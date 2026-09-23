import { t } from "@/i18n";
import type { Session } from "@/lib/types";

/** The one-line activity a session is showing right now. */
export const activityText = (s: Session): { label: string; detail: string | null } => {
  const a = s.activity;
  if (!a) return { label: t("activity.idle"), detail: null };
  if (a.kind === "waiting") return { label: t("activity.waiting"), detail: a.detail };
  if (a.kind === "thinking") return { label: t("activity.thinking"), detail: null };
  if (a.kind === "done") return { label: t("activity.done"), detail: null };
  return { label: a.label, detail: a.detail };
};
