import { describe, expect, test } from "bun:test";
import { buildTheme } from "@/lib/terminal-theme";

describe("buildTheme", () => {
  test("reads every color from a CSS var, keyed by the expected var name", () => {
    const seen: Record<string, string> = {};
    const theme = buildTheme((name, fallback) => {
      seen[name] = fallback;
      return `value(${name})`;
    });

    expect(theme.background).toBe("value(--color-bg)");
    expect(theme.foreground).toBe("value(--color-fg)");
    expect(theme.cursor).toBe("value(--color-busy)");
    expect(theme.cursorAccent).toBe("value(--color-bg)");
    expect(theme.selectionBackground).toBe("value(--color-surface-2)");
  });

  test("falls back to a fixed dark palette when a var is unset", () => {
    const theme = buildTheme((_name, fallback) => fallback);
    expect(theme.background).toBe("#0b0d10");
    expect(theme.foreground).toBe("#e7e9ec");
    expect(theme.red).toBe("#f0616d");
    expect(theme.green).toBe("#3dd68c");
    expect(theme.blue).toBe("#4c9aff");
  });

  test("bright variants stay legible against the standard ones (not identical shades)", () => {
    const theme = buildTheme((_name, fallback) => fallback);
    expect(theme.magenta).not.toBe(theme.brightMagenta);
  });

  test("is pure: same getter input always yields the same theme", () => {
    const getVar = (_name: string, fallback: string) => fallback;
    expect(buildTheme(getVar)).toEqual(buildTheme(getVar));
  });
});
