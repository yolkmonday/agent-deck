import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { useLang } from "@/i18n";
import { formatDuration, formatShort, formatTokens, totalTokens } from "@/lib/format";

afterEach(() => useLang.setState({ lang: "en" }));

describe("id", () => {
  beforeEach(() => useLang.setState({ lang: "id" }));

  describe("formatTokens", () => {
    test("millions use jt with comma decimal", () => {
      expect(formatTokens(18_400_000)).toBe("18,4 jt");
      expect(formatTokens(1_000_000)).toBe("1,0 jt");
    });
    test("thousands use rb", () => {
      expect(formatTokens(448_700)).toBe("448,7 rb");
    });
    test("small numbers stay plain", () => {
      expect(formatTokens(950)).toBe("950");
    });
  });

  describe("formatDuration", () => {
    test("minutes and hours", () => {
      expect(formatDuration(30_000)).toBe("<1 mnt");
      expect(formatDuration(26 * 60_000)).toBe("26 mnt");
      expect(formatDuration(64 * 60_000)).toBe("1 j 4 mnt");
    });
  });

  describe("formatShort", () => {
    test("shows seconds below a minute", () => {
      expect(formatShort(12_300)).toBe("12 dtk");
      expect(formatShort(400)).toBe("0 dtk");
      expect(formatShort(-5_000)).toBe("0 dtk");
    });
    test("delegates to formatDuration above a minute", () => {
      expect(formatShort(90_000)).toBe(formatDuration(90_000));
    });
  });
});

describe("en", () => {
  beforeEach(() => useLang.setState({ lang: "en" }));
  test("tokens", () => {
    expect(formatTokens(18_400_000)).toBe("18.4M");
    expect(formatTokens(448_700)).toBe("448.7K");
    expect(formatTokens(950)).toBe("950");
  });
  test("durations", () => {
    expect(formatDuration(30_000)).toBe("<1 min");
    expect(formatDuration(26 * 60_000)).toBe("26 min");
    expect(formatDuration(64 * 60_000)).toBe("1 h 4 min");
    expect(formatShort(12_300)).toBe("12 s");
  });
  test("switching language changes output immediately", () => {
    expect(formatTokens(1_000)).toBe("1.0K");
    useLang.setState({ lang: "id" });
    expect(formatTokens(1_000)).toBe("1,0 rb");
  });
});

test("totalTokens excludes reasoning", () => {
  expect(totalTokens({ input: 1, output: 2, cacheRead: 3, cacheWrite: 4, reasoning: 99 })).toBe(10);
});
