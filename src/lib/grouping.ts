import type { Session } from "@/lib/types";

export interface SessionGroup {
  key: string;
  label: string;
  sessions: Session[];
}

/// Folds the sessions of one project together. The incoming order is already
/// waiting-first then most-recently-active, so the only local reorder needed is
/// putting the main checkout above its worktrees.
const withinGroup = (a: Session, b: Session): number =>
  Number(a.isWorktree) - Number(b.isWorktree);

/// A group is as urgent as its best member, so one waiting session lifts the
/// whole project above the quiet ones.
const groupRank = (g: SessionGroup): number =>
  g.sessions.some((s) => s.status === "waiting") ? 1 : 0;

const newest = (g: SessionGroup): number =>
  Math.max(...g.sessions.map((s) => s.updatedAtMs));

export const groupSessions = (sessions: Session[]): SessionGroup[] => {
  const byKey = new Map<string, SessionGroup>();
  for (const s of sessions) {
    const existing = byKey.get(s.groupRoot);
    if (existing) {
      existing.sessions.push(s);
    } else {
      byKey.set(s.groupRoot, { key: s.groupRoot, label: s.group, sessions: [s] });
    }
  }
  const groups = [...byKey.values()];
  for (const g of groups) g.sessions.sort(withinGroup);
  return groups.sort((a, b) => groupRank(b) - groupRank(a) || newest(b) - newest(a));
};
