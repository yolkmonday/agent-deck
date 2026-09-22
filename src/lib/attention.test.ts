import { describe, expect, test } from "bun:test";
import type { TermSession } from "@/lib/api";
import {
  healthLabel,
  newlyWaiting,
  resolveTarget,
  unhealthySessions,
  waitingLabel,
  waitingSessions,
} from "@/lib/attention";
import type { Session } from "@/lib/types";

const session = (over: Partial<Session> = {}): Session => ({
  id: "s1",
  agent: "claude",
  pid: 1,
  project: "noor",
  cwd: "/dev/noor",
  group: "noor",
  groupRoot: "/dev/noor",
  isWorktree: false,
  worktreeName: null,
  model: "opus",
  branch: "main",
  status: "waiting",
  activity: null,
  tokens: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, reasoning: 0 },
  ownTokens: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, reasoning: 0 },
  subagents: [],
  costUsd: 0,
  billingMode: "payg",
  priced: true,
  startedAtMs: 0,
  updatedAtMs: 0,
  quietMs: 0,
  toolRunningMs: null,
  health: "ok",
  healthReason: null,
  ...over,
});

const term = (over: Partial<TermSession> = {}): TermSession => ({
  id: "t1",
  profileId: "claude",
  label: "claude",
  cwd: "/dev/noor",
  command: "claude",
  startedAtMs: 0,
  alive: true,
  ...over,
});

describe("waitingSessions", () => {
  test("keeps only waiting sessions, oldest waiting first", () => {
    const busy = session({ id: "busy", status: "busy", updatedAtMs: 1 });
    const newer = session({ id: "newer", updatedAtMs: 200 });
    const older = session({ id: "older", updatedAtMs: 100 });
    const result = waitingSessions([busy, newer, older]);
    expect(result.map((s) => s.id)).toEqual(["older", "newer"]);
  });
});

describe("newlyWaiting", () => {
  test("returns nothing on the first snapshot", () => {
    expect(newlyWaiting(null, [session()])).toEqual([]);
  });

  test("returns only sessions that flipped to waiting", () => {
    const prev = [session({ id: "a", status: "waiting" }), session({ id: "b", status: "busy" })];
    const next = [session({ id: "a", status: "waiting" }), session({ id: "b", status: "waiting" })];
    expect(newlyWaiting(prev, next).map((s) => s.id)).toEqual(["b"]);
  });

  test("includes a session that did not exist before", () => {
    const result = newlyWaiting([session({ id: "a" })], [session({ id: "a" }), session({ id: "fresh" })]);
    expect(result.map((s) => s.id)).toEqual(["fresh"]);
  });
});

describe("resolveTarget", () => {
  test("picks the live terminal that shares the cwd", () => {
    expect(resolveTarget(session(), [term({ id: "t9" })])).toEqual({ kind: "terminal", termId: "t9" });
  });

  test("ignores terminals that are no longer alive", () => {
    expect(resolveTarget(session(), [term({ id: "t9", alive: false })])).toEqual({
      kind: "live",
      sessionId: "s1",
    });
  });

  test("prefers the most recently started match", () => {
    const older = term({ id: "old", startedAtMs: 10 });
    const newer = term({ id: "new", startedAtMs: 20 });
    expect(resolveTarget(session(), [older, newer])).toEqual({ kind: "terminal", termId: "new" });
  });

  test("falls back to the live card for a foreign cwd", () => {
    expect(resolveTarget(session(), [term({ id: "t9", cwd: "/elsewhere" })])).toEqual({
      kind: "live",
      sessionId: "s1",
    });
  });
});

describe("waitingLabel", () => {
  test("formats project and duration", () => {
    const s = session({ project: "noor", updatedAtMs: 0 });
    expect(waitingLabel(s, 8 * 60_000)).toBe("noor butuh jawaban · 8 mnt");
  });
});

describe("unhealthySessions", () => {
  test("puts stalled before slow", () => {
    const slow = session({ id: "slow", health: "slow", healthReason: "tool \"Bash\" berjalan 12 mnt" });
    const stalled = session({ id: "stalled", health: "stalled", healthReason: "diam 9 mnt tanpa tool berjalan" });
    expect(unhealthySessions([slow, stalled]).map((s) => s.id)).toEqual(["stalled", "slow"]);
  });

  test("sorts by quiet time within a group", () => {
    const recent = session({ id: "recent", health: "stalled", quietMs: 6 * 60_000 });
    const older = session({ id: "older", health: "stalled", quietMs: 30 * 60_000 });
    expect(unhealthySessions([recent, older]).map((s) => s.id)).toEqual(["older", "recent"]);
  });

  test("excludes ok sessions", () => {
    const ok = session({ id: "ok", health: "ok" });
    const stalled = session({ id: "stalled", health: "stalled" });
    expect(unhealthySessions([ok, stalled]).map((s) => s.id)).toEqual(["stalled"]);
  });
});

describe("healthLabel", () => {
  test("formats stalled and slow", () => {
    const stalled = session({
      project: "noor",
      health: "stalled",
      healthReason: "diam 9 mnt tanpa tool berjalan",
    });
    expect(healthLabel(stalled)).toBe("noor macet · diam 9 mnt tanpa tool berjalan");

    const slow = session({
      project: "kirimi",
      health: "slow",
      healthReason: 'tool "Bash" berjalan 12 mnt',
    });
    expect(healthLabel(slow)).toBe('kirimi lambat · tool "Bash" berjalan 12 mnt');
  });
});
