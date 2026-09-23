import { describe, expect, test } from "bun:test";
import { computeGridSize } from "@/lib/terminal-instances";

describe("computeGridSize", () => {
  test("divides available space by char metrics and floors to whole cells", () => {
    expect(computeGridSize(800, 400, 8, 16)).toEqual({ cols: 100, rows: 25 });
    expect(computeGridSize(805, 415, 8, 16)).toEqual({ cols: 100, rows: 25 });
  });

  test("never returns fewer than the sane minimum grid", () => {
    expect(computeGridSize(1, 1, 8, 16)).toEqual({ cols: 20, rows: 4 });
    expect(computeGridSize(-100, -100, 8, 16)).toEqual({ cols: 20, rows: 4 });
  });

  test("does not divide by zero when char metrics are degenerate", () => {
    expect(computeGridSize(800, 400, 0, 0)).toEqual({ cols: 800, rows: 400 });
  });
});
