import type { TermSession } from "@/lib/api";
import { formatDuration } from "@/lib/format";
import type { Session, Orphan } from "@/lib/types";

export type JumpTarget = { kind: "terminal"; termId: string } | { kind: "live"; sessionId: string };

export const waitingSessions = (sessions: Session[]): Session[] =>
  sessions.filter((s) => s.status === "waiting").sort((a, b) => a.updatedAtMs - b.updatedAtMs);

export const newlyWaiting = (prev: Session[] | null, next: Session[]): Session[] => {
  if (prev === null) return [];
  const before = new Map(prev.map((s) => [s.id, s]));
  return next.filter((s) => s.status === "waiting" && before.get(s.id)?.status !== "waiting");
};

export const resolveTarget = (session: Session, terms: TermSession[]): JumpTarget => {
  const match = terms
    .filter((t) => t.alive && t.cwd === session.cwd)
    .reduce<TermSession | null>((best, t) => (best === null || t.startedAtMs > best.startedAtMs ? t : best), null);
  return match ? { kind: "terminal", termId: match.id } : { kind: "live", sessionId: session.id };
};

export const waitingLabel = (session: Session, nowMs: number): string =>
  `${session.project} butuh jawaban · ${formatDuration(Math.max(0, nowMs - session.updatedAtMs))}`;

// A stalled session is the louder problem, so it sorts ahead of a slow one; within
// a group the longest silence comes first.
const healthRank = (s: Session): number => (s.health === "stalled" ? 0 : 1);

export const unhealthySessions = (sessions: Session[]): Session[] =>
  sessions
    .filter((s) => s.health !== "ok")
    .sort((a, b) => healthRank(a) - healthRank(b) || b.quietMs - a.quietMs);

/// Only a genuine transition counts, so a session that stays stalled does not
/// re-notify on every snapshot.
export const newlyStalled = (prev: Session[] | null, next: Session[]): Session[] => {
  if (prev === null) return [];
  const before = new Map(prev.map((s) => [s.id, s]));
  return next.filter((s) => s.health === "stalled" && before.get(s.id)?.health !== "stalled");
};

export const healthLabel = (session: Session): string =>
  `${session.project} ${session.health === "stalled" ? "macet" : "lambat"} · ${session.healthReason ?? ""}`;

export const orphanLabel = (orphan: Orphan): string =>
  `${orphan.agent} jalan di ${orphan.cwd} tapi tidak ada sesi · ${formatDuration(orphan.ageMs)}`;
