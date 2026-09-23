import { describe, expect, test } from "bun:test";
import { groupCards, groupSessions } from "@/lib/grouping";
import type { Session } from "@/lib/types";

const session = (over: Partial<Session> & { id: string }): Session => ({
  agent: "claude",
  pid: null,
  project: over.id,
  cwd: `/w/${over.id}`,
  group: "p",
  groupRoot: "/w/p",
  isWorktree: false,
  billingMode: "payg",
  worktreeName: null,
  task: null,
  model: null,
  branch: null,
  status: "busy",
  activity: null,
  tokens: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, reasoning: 0 },
  ownTokens: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, reasoning: 0 },
  subagents: [],
  costUsd: 0,
  priced: false,
  startedAtMs: null,
  updatedAtMs: 0,
  quietMs: 0,
  toolRunningMs: null,
  health: "ok",
  healthReason: null,
  ...over,
});

describe("groupSessions", () => {
  test("groups_by_group_root_not_by_name", () => {
    const a = session({ id: "a", project: "noor", group: "noor", groupRoot: "/w/noor", updatedAtMs: 20 });
    const b = session({
      id: "b",
      project: "noor-oc-feat-1",
      group: "noor",
      groupRoot: "/w/noor",
      isWorktree: true,
      worktreeName: "noor-oc-feat-1",
      updatedAtMs: 10,
    });
    const groups = groupSessions([a, b]);
    expect(groups.length).toBe(1);
    expect(groups[0].key).toBe("/w/noor");
    expect(groups[0].label).toBe("noor");
    expect(groups[0].sessions.map((s) => s.id)).toEqual(["a", "b"]);
  });

  test("main_checkout_sorts_before_its_worktrees", () => {
    const wt = session({ id: "wt", groupRoot: "/w/noor", isWorktree: true, updatedAtMs: 900 });
    const main = session({ id: "main", groupRoot: "/w/noor", updatedAtMs: 100 });
    expect(groupSessions([wt, main])[0].sessions.map((s) => s.id)).toEqual(["main", "wt"]);
  });

  test("a_group_with_a_waiting_session_comes_first", () => {
    const calm = session({ id: "calm", groupRoot: "/w/calm", updatedAtMs: 9_000 });
    const waiting = session({ id: "wait", groupRoot: "/w/wait", status: "waiting", updatedAtMs: 1 });
    expect(groupSessions([calm, waiting]).map((g) => g.key)).toEqual(["/w/wait", "/w/calm"]);
  });

  test("groups_without_waiting_sort_by_newest_activity", () => {
    const old = session({ id: "old", groupRoot: "/w/old", updatedAtMs: 10 });
    const fresh = session({ id: "fresh", groupRoot: "/w/fresh", updatedAtMs: 500 });
    expect(groupSessions([old, fresh]).map((g) => g.key)).toEqual(["/w/fresh", "/w/old"]);
  });

  test("a_single_session_still_yields_one_group", () => {
    const groups = groupSessions([session({ id: "solo", groupRoot: "/w/solo" })]);
    expect(groups.length).toBe(1);
    expect(groups[0].sessions.map((s) => s.id)).toEqual(["solo"]);
  });
});

describe("groupCards", () => {
  test("the main checkout leads and its worktrees become children", () => {
    const cards = groupCards([
      session({ id: "wt", isWorktree: true, worktreeName: "feat-1" }),
      session({ id: "main" }),
    ]);
    expect(cards).toHaveLength(1);
    expect(cards[0].primary.id).toBe("main");
    expect(cards[0].children.map((c) => c.id)).toEqual(["wt"]);
  });

  test("a lone session has no children", () => {
    const cards = groupCards([session({ id: "solo" })]);
    expect(cards[0].children).toEqual([]);
  });

  test("a waiting child is counted so it cannot hide", () => {
    const cards = groupCards([
      session({ id: "main" }),
      session({ id: "wt", isWorktree: true, status: "waiting" }),
    ]);
    expect(cards[0].waitingChildren).toBe(1);
  });
});
