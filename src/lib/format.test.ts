import { describe, expect, test } from "bun:test";
import { formatDuration, formatShort, formatTokens, totalTokens } from "@/lib/format";

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

test("totalTokens excludes reasoning", () => {
  expect(totalTokens({ input: 1, output: 2, cacheRead: 3, cacheWrite: 4, reasoning: 99 })).toBe(10);
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
