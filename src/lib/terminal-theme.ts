import type { ITheme } from "@xterm/xterm";

/** Reads a CSS custom property off `:root`, or `fallback` when it is unset/empty. */
export type CssVarGetter = (name: string, fallback: string) => string;

export const cssVar: CssVarGetter = (name, fallback) => {
  if (typeof document === "undefined") return fallback;
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return value.length > 0 ? value : fallback;
};

/**
 * Builds the xterm theme from the app's own design tokens, so the terminal's
 * background/foreground/cursor/accents always match the rest of the UI. The
 * app is dark-only today (no theme toggle, no light-mode CSS variables — see
 * src/index.css), so there is no light/dark branch here: if a light mode is
 * added later, its CSS variables will flow through automatically since these
 * are read live, not baked in.
 *
 * `getVar` is injectable so this stays a pure, unit-testable function; the
 * default reads the real CSS custom properties.
 */
export const buildTheme = (getVar: CssVarGetter = cssVar): ITheme => ({
  background: getVar("--color-bg", "#0b0d10"),
  foreground: getVar("--color-fg", "#e7e9ec"),
  cursor: getVar("--color-busy", "#4c9aff"),
  cursorAccent: getVar("--color-bg", "#0b0d10"),
  selectionBackground: getVar("--color-surface-2", "#1a1e25"),
  black: getVar("--color-surface", "#12151a"),
  red: getVar("--color-err", "#f0616d"),
  green: getVar("--color-ok", "#3dd68c"),
  yellow: getVar("--color-waiting", "#f5a524"),
  blue: getVar("--color-busy", "#4c9aff"),
  magenta: getVar("--color-codex", "#a78bfa"),
  cyan: getVar("--color-opencode", "#34d0e0"),
  white: getVar("--color-fg", "#e7e9ec"),
  brightBlack: getVar("--color-fg-3", "#69717d"),
  brightRed: getVar("--color-err", "#f0616d"),
  brightGreen: getVar("--color-ok", "#3dd68c"),
  brightYellow: getVar("--color-waiting", "#f5a524"),
  brightBlue: getVar("--color-busy", "#4c9aff"),
  brightMagenta: getVar("--color-claude", "#e8825c"),
  brightCyan: getVar("--color-opencode", "#34d0e0"),
  brightWhite: getVar("--color-fg", "#e7e9ec"),
});
