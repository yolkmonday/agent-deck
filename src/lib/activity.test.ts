import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { useLang } from "@/i18n";
import { activityText } from "@/lib/activity";
import type { Session } from "@/lib/types";

const session = (activity: Session["activity"]): Session =>
  ({
    id: "s1",
    agent: "claude",
    pid: 1,
    project: "noor",
    cwd: "/dev/noor",
    group: "noor",
    groupRoot: "/dev/noor",
    isWorktree: false,
    worktreeName: null,
    task: null,
    model: null,
    branch: null,
    status: "busy",
    activity,
    tokens: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, reasoning: 0 },
    ownTokens: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, reasoning: 0 },
    subagents: [],
    costUsd: 0,
    billingMode: "payg",
    priced: false,
    startedAtMs: null,
    updatedAtMs: 0,
    quietMs: 0,
    toolRunningMs: null,
    health: "ok",
    healthReason: null,
  }) as Session;

afterEach(() => useLang.setState({ lang: "en" }));

describe("en", () => {
  beforeEach(() => useLang.setState({ lang: "en" }));

  test("no activity reads as idle", () => {
    expect(activityText(session(null))).toEqual({ label: "Idle", detail: null });
  });

  test("waiting keeps the question as its detail", () => {
    expect(activityText(session({ kind: "waiting", label: "waiting", detail: "pilih opsi" }))).toEqual({
      label: "Waiting for your answer",
      detail: "pilih opsi",
    });
  });

  test("thinking has no detail even when the collector sent one", () => {
    expect(activityText(session({ kind: "thinking", label: "thinking", detail: "x" }))).toEqual({
      label: "Thinking",
      detail: null,
    });
  });

  test("done has no detail", () => {
    expect(activityText(session({ kind: "done", label: "done", detail: "x" }))).toEqual({
      label: "Done",
      detail: null,
    });
  });
});

describe("id", () => {
  beforeEach(() => useLang.setState({ lang: "id" }));

  test("no activity reads as idle", () => {
    expect(activityText(session(null))).toEqual({ label: "Diam", detail: null });
  });

  test("waiting keeps the question as its detail", () => {
    expect(activityText(session({ kind: "waiting", label: "waiting", detail: "pilih opsi" }))).toEqual({
      label: "Menunggu jawaban kamu",
      detail: "pilih opsi",
    });
  });

  test("thinking has no detail even when the collector sent one", () => {
    expect(activityText(session({ kind: "thinking", label: "thinking", detail: "x" }))).toEqual({
      label: "Berpikir",
      detail: null,
    });
  });

  test("done has no detail", () => {
    expect(activityText(session({ kind: "done", label: "done", detail: "x" }))).toEqual({
      label: "Selesai",
      detail: null,
    });
  });
});

test("a tool passes its own label and detail through", () => {
  expect(activityText(session({ kind: "tool", label: "Bash", detail: "bun test" }))).toEqual({
    label: "Bash",
    detail: "bun test",
  });
});
