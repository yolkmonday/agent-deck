import type { FeedEvent, Session } from "@/lib/types";

const toolText = (s: Session): string | null => {
  const a = s.activity;
  if (!a || a.kind !== "tool") return null;
  return a.detail ? `${a.label} ${a.detail}` : a.label;
};

export const diffEvents = (prev: Session[] | null, next: Session[], nowMs: number): FeedEvent[] => {
  if (prev === null) return [];
  const before = new Map(prev.map((s) => [s.id, s]));
  const events: FeedEvent[] = [];
  const push = (s: Session, color: FeedEvent["color"], text: string) =>
    events.push({ id: `${s.id}-${nowMs}-${events.length}`, timeMs: nowMs, project: s.project, agent: s.agent, color, text });

  for (const s of next) {
    const old = before.get(s.id);
    if (!old) {
      const color = s.status === "waiting" ? "waiting" : s.status === "idle" ? "idle" : "busy";
      push(s, color, "sesi dimulai");
      continue;
    }
    if (old.status !== s.status) {
      if (s.status === "waiting") push(s, "waiting", "butuh jawaban");
      else if (s.status === "idle") push(s, "idle", "diam");
      else push(s, "busy", "sibuk lagi");
      continue;
    }
    const tool = toolText(s);
    if (tool && tool !== toolText(old)) push(s, "busy", tool);
  }
  return events;
};
