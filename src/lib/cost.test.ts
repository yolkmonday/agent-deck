import { afterEach, describe, expect, test } from "bun:test";
import { useLang } from "@/i18n";
import { t } from "@/i18n";
import type { DailyRow } from "@/lib/api";
import { formatNotional, formatPct, formatUsd, stackByDate } from "@/lib/cost";

afterEach(() => {
  useLang.setState({ lang: "en" });
});

const usage = (input: number, output: number, cacheRead = 0, cacheWrite = 0, reasoning = 0) => ({
  input,
  output,
  cacheRead,
  cacheWrite,
  reasoning,
});

describe("formatUsd", () => {
  test("EN uses a period decimal separator", () => {
    useLang.setState({ lang: "en" });
    expect(formatUsd(12.6)).toBe("$12.60");
    expect(formatUsd(1234.5)).toBe("$1234.50");
  });
  test("EN zero renders as zero", () => {
    useLang.setState({ lang: "en" });
    expect(formatUsd(0)).toBe("$0.00");
  });
  test("EN tiny positive values render as less than one cent", () => {
    useLang.setState({ lang: "en" });
    expect(formatUsd(0.004)).toBe("<$0.01");
  });
  test("ID two decimals with comma separator and dollar prefix", () => {
    useLang.setState({ lang: "id" });
    expect(formatUsd(12.6)).toBe("$12,60");
    expect(formatUsd(1234.5)).toBe("$1234,50");
  });
  test("ID zero renders as zero", () => {
    useLang.setState({ lang: "id" });
    expect(formatUsd(0)).toBe("$0,00");
  });
  test("ID tiny positive values render as less than one cent", () => {
    useLang.setState({ lang: "id" });
    expect(formatUsd(0.004)).toBe("<$0,01");
  });
});

describe("formatPct", () => {
  test("rounds to a whole number", () => {
    expect(formatPct(87.4)).toBe("87%");
    expect(formatPct(0)).toBe("0%");
  });
});

describe("formatNotional", () => {
  test("EN always carries the approximation marker", () => {
    useLang.setState({ lang: "en" });
    expect(formatNotional(806.42)).toBe("≈ $806.42");
    expect(formatNotional(0)).toBe("≈ $0.00");
    expect(formatNotional(0.004)).toBe("≈ <$0.01");
  });
  test("ID always carries the approximation marker", () => {
    useLang.setState({ lang: "id" });
    expect(formatNotional(806.42)).toBe("≈ $806,42");
    expect(formatNotional(0)).toBe("≈ $0,00");
    expect(formatNotional(0.004)).toBe("≈ <$0,01");
  });
  test("the tooltip says the figure does not add to the bill", () => {
    expect(t("cost.notionalHint")).toBe("Estimate if billed per token. Doesn't add to the bill.");
  });
});

describe("stackByDate", () => {
  test("merges agents on the same date, fills missing agents with zero and sorts ascending", () => {
    const rows: DailyRow[] = [
      { date: "2026-09-22", agent: "claude", tokens: usage(100, 1), costUsd: 1 },
      { date: "2026-09-21", agent: "codex", tokens: usage(7, 3), costUsd: 2 },
      { date: "2026-09-22", agent: "claude", tokens: usage(1, 1), costUsd: 0 },
      { date: "2026-09-21", agent: "opencode", tokens: usage(10, 0), costUsd: 0 },
    ];
    expect(stackByDate(rows)).toEqual([
      { date: "2026-09-21", claude: 0, opencode: 10, codex: 10, total: 20 },
      { date: "2026-09-22", claude: 103, opencode: 0, codex: 0, total: 103 },
    ]);
  });

  test("empty input yields no buckets", () => {
    expect(stackByDate([])).toEqual([]);
  });
});
