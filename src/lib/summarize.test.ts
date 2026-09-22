import { expect, test } from "bun:test";
import { summarize } from "@/lib/summarize";
import type { Session } from "@/lib/types";

const s = (status: Session["status"], out: number): Session => ({
  id: status + out, agent: "claude", pid: 1, project: "p", cwd: "/p", model: null, branch: null,
  status, activity: null, tokens: { input: 0, output: out, cacheRead: 0, cacheWrite: 0, reasoning: 0 },
  startedAtMs: null, updatedAtMs: 0,
});

test("summarize counts by status and sums tokens", () => {
  const r = summarize([s("busy", 10), s("busy", 5), s("waiting", 1), s("idle", 4)]);
  expect(r).toEqual({ active: 4, busy: 2, waiting: 1, idle: 1, tokens: 20 });
});
