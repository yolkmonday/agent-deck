import { describe, expect, test } from "bun:test";
import type { TimelineSpan } from "@/lib/api";
import { layoutSpan, ticksFor, windowFor } from "@/lib/timeline";

const span = (startMs: number, endMs: number | null): TimelineSpan => ({
  id: `s:${startMs}`,
  tool: "Bash",
  detail: null,
  startMs,
  endMs,
  status: endMs === null ? "running" : "ok",
  tokens: null,
});

describe("windowFor", () => {
  test("returns a window whose width equals the requested minutes and whose toMs is nowMs", () => {
    const now = 1_700_000_000_000;
    expect(windowFor(30, now)).toEqual({ fromMs: now - 30 * 60_000, toMs: now });
  });
});

describe("layoutSpan", () => {
  const from = 0;
  const to = 1000;

  test("places a span covering the middle half of the window at leftPct 25 and widthPct 50", () => {
    expect(layoutSpan(span(250, 750), from, to, to)).toEqual({ leftPct: 25, widthPct: 50 });
  });

  test("clamps a span that starts before the window to leftPct 0 and reduces its width accordingly", () => {
    expect(layoutSpan(span(-500, 500), from, to, to)).toEqual({ leftPct: 0, widthPct: 50 });
  });

  test("treats a null endMs as nowMs", () => {
    expect(layoutSpan(span(500, null), from, to, 750)).toEqual({ leftPct: 50, widthPct: 25 });
  });

  test("gives a zero-length span at least 0.5 width", () => {
    expect(layoutSpan(span(500, 500), from, to, to)).toEqual({ leftPct: 50, widthPct: 0.5 });
  });
});

describe("ticksFor", () => {
  test("returns the requested count, ascending, first tick at fromMs", () => {
    const ticks = ticksFor(0, 1000, 4);
    expect(ticks.length).toBe(4);
    expect(ticks.map((t) => t.ms)).toEqual([0, 333.3333333333333, 666.6666666666666, 1000]);
    expect(new Set(ticks.map((t) => t.label)).size).toBeGreaterThan(0);
  });
});
