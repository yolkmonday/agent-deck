import { expect, test } from "bun:test";
import { diffEvents } from "@/lib/events";
import type { Session } from "@/lib/types";

const base: Session = {
  id: "a", agent: "claude", pid: 1, project: "noor", cwd: "/noor", model: null, branch: null,
  status: "busy", activity: null,
  tokens: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, reasoning: 0 },
  ownTokens: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, reasoning: 0 },
  subagents: [],
  costUsd: 0, billingMode: "payg", priced: true,
  startedAtMs: null, updatedAtMs: 0,
  quietMs: 0, toolRunningMs: null, health: "ok", healthReason: null,
};

test("first snapshot produces no events", () => {
  expect(diffEvents(null, [base], 1000)).toEqual([]);
});

test("new session emits a started event", () => {
  const ev = diffEvents([], [base], 1000);
  expect(ev).toHaveLength(1);
  expect(ev[0].text).toBe("sesi dimulai");
});

test("busy to waiting emits a needs-answer event", () => {
  const ev = diffEvents([base], [{ ...base, status: "waiting" }], 2000);
  expect(ev[0]).toMatchObject({ project: "noor", color: "waiting", text: "butuh jawaban" });
});

test("new session keeps its own status colour", () => {
  const waitingSession = { ...base, id: "w", status: "waiting" as const };
  expect(diffEvents([], [waitingSession], 1000)[0]).toMatchObject({
    color: "waiting",
    text: "sesi dimulai",
  });
  expect(diffEvents([], [base], 1000)[0].color).toBe("busy");
});

test("new tool activity emits a tool event, unchanged activity emits nothing", () => {
  const withTool = { ...base, activity: { kind: "tool" as const, label: "Bash", detail: "bun test" } };
  expect(diffEvents([base], [withTool], 3000)[0].text).toBe("Bash bun test");
  expect(diffEvents([withTool], [withTool], 4000)).toEqual([]);
});
