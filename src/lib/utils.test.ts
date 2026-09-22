import { describe, expect, test } from "bun:test";
import { cn } from "@/lib/utils";

describe("cn", () => {
  test("keeps_strings_and_drops_falsy", () => {
    expect(cn("a", "b")).toBe("a b");
    expect(cn("a", null, undefined, false, "")).toBe("a");
  });

  test("flattens_arrays_and_objects", () => {
    expect(cn("a", ["b", ["c"]])).toBe("a b c");
    expect(cn({ on: true, off: false, missing: undefined })).toBe("on");
    expect(cn(0, 1)).toBe("0 1");
  });
});
