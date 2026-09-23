import { Channel } from "@tauri-apps/api/core";
import { FitAddon } from "@xterm/addon-fit";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal } from "@xterm/xterm";
import { termAttach, termResize, termWrite } from "@/lib/api";
import { buildTheme } from "@/lib/terminal-theme";

export const TERMINAL_FONT_FAMILY =
  '"JetBrains Mono Variable", "SF Mono", Menlo, ui-monospace, monospace';
export const TERMINAL_FONT_SIZE = 13;
export const TERMINAL_LINE_HEIGHT = 1.2;

const MIN_COLS = 20;
const MIN_ROWS = 4;

/**
 * One xterm `Terminal` per PTY session, created once and kept alive for as
 * long as the session exists — independent of whether `TerminalView` (the
 * React component) is currently mounted. This is what lets switching tabs,
 * or leaving and coming back to the Terminal page, show the exact same
 * terminal instead of a freshly recreated one replaying scrollback text.
 */
interface TerminalInstance {
  term: Terminal;
  fit: FitAddon;
  element: HTMLDivElement;
  lastCols: number;
  lastRows: number;
}

const instances = new Map<string, TerminalInstance>();

const loadWebgl = (term: Terminal): void => {
  try {
    const webgl = new WebglAddon();
    webgl.onContextLoss(() => webgl.dispose());
    term.loadAddon(webgl);
  } catch {
    // WebGL unavailable (e.g. software rendering) — DOM renderer stays active.
  }
};

/** Returns the session's terminal instance, creating and attaching it on first use. */
export const getOrCreate = (sessionId: string): TerminalInstance => {
  const existing = instances.get(sessionId);
  if (existing) return existing;

  const element = document.createElement("div");
  element.className = "h-full w-full";

  const term = new Terminal({
    fontFamily: TERMINAL_FONT_FAMILY,
    fontSize: TERMINAL_FONT_SIZE,
    lineHeight: TERMINAL_LINE_HEIGHT,
    cursorStyle: "bar",
    cursorBlink: true,
    scrollback: 10000,
    minimumContrastRatio: 4.5,
    allowProposedApi: true,
    scrollSensitivity: 1,
    fastScrollSensitivity: 5,
    macOptionIsMeta: false,
    theme: buildTheme(),
  });

  const fit = new FitAddon();
  term.loadAddon(fit);
  term.loadAddon(new Unicode11Addon());
  term.unicode.activeVersion = "11";

  term.open(element);
  loadWebgl(term);

  term.onData((data) => {
    void termWrite(sessionId, data).catch(() => undefined);
  });

  const channel = new Channel<ArrayBuffer>();
  channel.onmessage = (data) => {
    term.write(new Uint8Array(data));
  };
  void termAttach(sessionId, channel).catch(() => undefined);

  const instance: TerminalInstance = {
    term,
    fit,
    element,
    lastCols: term.cols,
    lastRows: term.rows,
  };
  instances.set(sessionId, instance);
  return instance;
};

/** Re-fits the terminal to its current host size and resizes the PTY if the grid changed. */
export const fitInstance = (sessionId: string): void => {
  const instance = instances.get(sessionId);
  if (!instance || !instance.element.isConnected) return;
  try {
    instance.fit.fit();
  } catch {
    return;
  }
  const { cols, rows } = instance.term;
  if (cols !== instance.lastCols || rows !== instance.lastRows) {
    instance.lastCols = cols;
    instance.lastRows = rows;
    void termResize(sessionId, cols, rows).catch(() => undefined);
  }
};

/**
 * Forces a full repaint. A DOM/WebGL write that happened while the tab was
 * hidden (`display: none`) is not guaranteed to be reflected once it becomes
 * visible again — this is the standard xterm.js fix.
 */
export const refreshInstance = (sessionId: string): void => {
  const instance = instances.get(sessionId);
  if (!instance) return;
  instance.term.refresh(0, instance.term.rows - 1);
};

export const focusInstance = (sessionId: string): boolean => {
  const instance = instances.get(sessionId);
  if (!instance) return false;
  instance.term.focus();
  return true;
};

/** Disposes a session's terminal. Call once the session is actually gone, not on tab switch. */
export const dispose = (sessionId: string): void => {
  const instance = instances.get(sessionId);
  if (!instance) return;
  instances.delete(sessionId);
  instance.term.dispose();
};

/** Pure grid-size math, kept separate from DOM measurement so it is unit-testable. */
export const computeGridSize = (
  availableWidth: number,
  availableHeight: number,
  charWidth: number,
  charHeight: number,
): { cols: number; rows: number } => ({
  cols: Math.max(Math.floor(availableWidth / Math.max(charWidth, 1)), MIN_COLS),
  rows: Math.max(Math.floor(availableHeight / Math.max(charHeight, 1)), MIN_ROWS),
});

// Rough chrome estimate for the space a terminal host occupies before it has
// ever been laid out (a brand-new session's PTY must open before its
// TerminalView can mount — see Sidebar's `w-58` and TerminalPage's header /
// tab bar / cwd row / padding). The real ResizeObserver-driven fit in
// TerminalView corrects this within the first frame once the host is
// actually measurable; this only keeps the PTY from opening at a hardcoded
// 80x24 in the meantime.
const SIDEBAR_WIDTH = 232;
const PAGE_CHROME_WIDTH = 60;
const PAGE_CHROME_HEIGHT = 170;

/** Estimates the initial PTY grid size before any terminal host exists yet. */
export const measureInitialSize = (): { cols: number; rows: number } => {
  let charWidth = TERMINAL_FONT_SIZE * 0.6;
  if (typeof document !== "undefined") {
    const ctx = document.createElement("canvas").getContext("2d");
    if (ctx) {
      ctx.font = `${TERMINAL_FONT_SIZE}px ${TERMINAL_FONT_FAMILY}`;
      const measured = ctx.measureText("W").width;
      if (measured > 0) charWidth = measured;
    }
  }
  const charHeight = TERMINAL_FONT_SIZE * TERMINAL_LINE_HEIGHT;
  const availableWidth =
    typeof window !== "undefined" ? window.innerWidth - SIDEBAR_WIDTH - PAGE_CHROME_WIDTH : 800;
  const availableHeight =
    typeof window !== "undefined" ? window.innerHeight - PAGE_CHROME_HEIGHT : 400;
  return computeGridSize(availableWidth, availableHeight, charWidth, charHeight);
};
