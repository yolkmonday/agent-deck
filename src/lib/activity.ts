import type { Session } from "@/lib/types";

/** The one-line activity a session is showing right now, in Indonesian. */
export const activityText = (s: Session): { label: string; detail: string | null } => {
  const a = s.activity;
  if (!a) return { label: "Diam", detail: null };
  if (a.kind === "waiting") return { label: "Menunggu jawaban kamu", detail: a.detail };
  if (a.kind === "thinking") return { label: "Berpikir", detail: null };
  if (a.kind === "done") return { label: "Selesai", detail: null };
  return { label: a.label, detail: a.detail };
};
