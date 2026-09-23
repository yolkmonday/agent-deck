import { afterEach, beforeEach, describe, expect, it } from "bun:test";
import { useLang } from "@/i18n";
import type { PerfAgg } from "@/lib/api";
import { chartSeries, filterPerf, formatTps, formatTtft, sortPerf } from "@/lib/perf";

afterEach(() => useLang.setState({ lang: "en" }));

const row = (key: string, tpsP50: number, ttftP50Ms: number | null, samples = 10, models = [key]): PerfAgg => ({
  key, label: key, agent: "opencode", models, samples, tpsP50, tpsP10: tpsP50 / 2, ttftP50Ms, precise: true, daily: [], children: [],
});

describe("id", () => {
  beforeEach(() => useLang.setState({ lang: "id" }));

  describe("formatTps", () => {
    it("formats with one decimal, id-ID", () => expect(formatTps(41.23, true)).toBe("41,2"));
    it("prefixes estimates with ~", () => expect(formatTps(41.23, false)).toBe("~41,2"));
    it("matches brief example", () => expect(formatTps(12.34, true)).toBe("12,3"));
  });

  describe("formatTtft", () => {
    it("dash when unknown", () => expect(formatTtft(null)).toBe("-"));
    it("ms under a second", () => expect(formatTtft(850)).toBe("850 ms"));
    it("seconds above", () => expect(formatTtft(1100)).toBe("1,1 dtk"));
    it("matches brief example", () => expect(formatTtft(1500)).toBe("1,5 dtk"));
  });
});

describe("en", () => {
  beforeEach(() => useLang.setState({ lang: "en" }));

  describe("formatTps", () => {
    it("formats with one decimal, en-US", () => expect(formatTps(12.34, true)).toBe("12.3"));
    it("prefixes estimates with ~", () => expect(formatTps(12.34, false)).toBe("~12.3"));
  });

  describe("formatTtft", () => {
    it("dash when unknown", () => expect(formatTtft(null)).toBe("-"));
    it("ms under a second", () => expect(formatTtft(850)).toBe("850 ms"));
    it("seconds above", () => expect(formatTtft(1500)).toBe("1.5 s"));
  });
});

describe("sortPerf", () => {
  it("sorts desc by tps", () => {
    const out = sortPerf([row("a", 10, 1), row("b", 40, 1)], "tpsP50", true);
    expect(out.map((r) => r.key)).toEqual(["b", "a"]);
  });
  it("puts null ttft last in both directions", () => {
    const rows = [row("a", 1, null), row("b", 1, 500), row("c", 1, 200)];
    expect(sortPerf(rows, "ttftP50Ms", false).map((r) => r.key)).toEqual(["c", "b", "a"]);
    expect(sortPerf(rows, "ttftP50Ms", true).map((r) => r.key)).toEqual(["b", "c", "a"]);
  });
});

describe("filterPerf", () => {
  it("matches label or models case-insensitively", () => {
    const rows = [row("deepseek-v4-1-flash", 1, null, 10, ["kn/deepseek-v4-1-flash", "Sumo/deepseek-v4.1-flash:netra"]), row("gpt-5.5", 1, null)];
    expect(filterPerf(rows, "SUMO").map((r) => r.key)).toEqual(["deepseek-v4-1-flash"]);
    expect(filterPerf(rows, "").length).toBe(2);
  });

  it("matches by label even when the key carries a fam:/agent: prefix", () => {
    const rows = [{ ...row("fam:deepseek-v4-1-flash", 1, null), label: "deepseek-v4-1-flash" }];
    expect(filterPerf(rows, "deepseek").map((r) => r.key)).toEqual(["fam:deepseek-v4-1-flash"]);
  });
});

describe("chartSeries", () => {
  it("merges daily points by day", () => {
    const a = { ...row("a", 1, null), daily: [{ day: "2026-09-21", tpsP50: 10 }, { day: "2026-09-22", tpsP50: 12 }] };
    const b = { ...row("b", 1, null), daily: [{ day: "2026-09-22", tpsP50: 30 }] };
    expect(chartSeries([a, b], ["a", "b"])).toEqual([
      { day: "2026-09-21", a: 10 },
      { day: "2026-09-22", a: 12, b: 30 },
    ]);
  });

  it("matches a selected key against a grouped row's children", () => {
    const child1 = { ...row("c1", 1, null), daily: [{ day: "2026-09-21", tpsP50: 5 }] };
    const child2 = { ...row("c2", 1, null), daily: [{ day: "2026-09-21", tpsP50: 8 }] };
    const family = { ...row("family", 1, null), children: [child1, child2] };
    expect(chartSeries([family], ["c1"])).toEqual([{ day: "2026-09-21", c1: 5 }]);
  });
});
