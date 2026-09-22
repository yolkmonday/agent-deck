import { expect, test } from "bun:test";
import { summarize } from "@/lib/summarize";
import type { Session } from "@/lib/types";

const s = (
  status: Session["status"],
  out: number,
  costUsd = 0,
  priced = true,
  billingMode: Session["billingMode"] = "payg",
): Session => ({
  id: status + out, agent: "claude", pid: 1, project: "p", cwd: "/p",
  group: "p", groupRoot: "/p", isWorktree: false, worktreeName: null, model: null, branch: null,
  status, activity: null, tokens: { input: 0, output: out, cacheRead: 0, cacheWrite: 0, reasoning: 0 },
  ownTokens: { input: 0, output: out, cacheRead: 0, cacheWrite: 0, reasoning: 0 }, subagents: [],
  costUsd, billingMode, priced, startedAtMs: null, updatedAtMs: 0,
  quietMs: 0, toolRunningMs: null, health: "ok", healthReason: null,
});

test("summarize counts by status, sums tokens and adds cost of unpriced sessions", () => {
  const r = summarize([
    s("busy", 10, 0.5),
    s("busy", 5, 1.25),
    s("waiting", 1, 0, false),
    s("idle", 4, 0.25),
  ]);
  expect(r).toEqual({
    active: 4,
    busy: 2,
    waiting: 1,
    idle: 1,
    tokens: 20,
    cost: 2,
    spend: 2,
    notional: 2,
    unpriced: 1,
    stalled: 0,
  });
});

test("a subscription adds nothing to spend but keeps its notional figure", () => {
  const r = summarize([
    s("busy", 10, 806.42, true, "subscription"),
    s("idle", 4, 0.25),
  ]);
  expect(r.spend).toBe(0.25);
  expect(r.notional).toBe(806.67);
  expect(r.cost).toBe(0.25);
});
