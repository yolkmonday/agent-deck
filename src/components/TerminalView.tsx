import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import { useEffect, useRef } from "react";
import type { TermSession } from "@/lib/api";
import { termResize, termScrollback, termWrite } from "@/lib/api";
import { registerWriter } from "@/store/terminal";

const cssVar = (name: string, fallback: string) => {
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return value.length > 0 ? value : fallback;
};

const buildTheme = () => ({
  background: cssVar("--color-bg", "#0b0d10"),
  foreground: cssVar("--color-fg", "#e7e9ec"),
  cursor: cssVar("--color-busy", "#4c9aff"),
  cursorAccent: cssVar("--color-bg", "#0b0d10"),
  selectionBackground: cssVar("--color-surface-2", "#1a1e25"),
  black: cssVar("--color-surface", "#12151a"),
  red: cssVar("--color-err", "#f0616d"),
  green: cssVar("--color-ok", "#3dd68c"),
  yellow: cssVar("--color-waiting", "#f5a524"),
  blue: cssVar("--color-busy", "#4c9aff"),
  magenta: cssVar("--color-codex", "#a78bfa"),
  cyan: cssVar("--color-opencode", "#34d0e0"),
  white: cssVar("--color-fg", "#e7e9ec"),
  brightBlack: cssVar("--color-fg-3", "#69717d"),
  brightRed: cssVar("--color-err", "#f0616d"),
  brightGreen: cssVar("--color-ok", "#3dd68c"),
  brightYellow: cssVar("--color-waiting", "#f5a524"),
  brightBlue: cssVar("--color-busy", "#4c9aff"),
  brightMagenta: cssVar("--color-claude", "#e8825c"),
  brightCyan: cssVar("--color-opencode", "#34d0e0"),
  brightWhite: cssVar("--color-fg", "#e7e9ec"),
});

export const TerminalView = ({ session }: { session: TermSession }) => {
  const hostRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const term = new Terminal({
      fontFamily: "JetBrains Mono Variable, ui-monospace, monospace",
      fontSize: 12.5,
      lineHeight: 1.3,
      cursorBlink: true,
      scrollback: 5000,
      theme: buildTheme(),
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host);
    fit.fit();

    let disposed = false;
    let fitFrame = 0;
    let inputFrame = 0;
    let lastCols = 0;
    let lastRows = 0;
    let pending = "";

    const applyFit = () => {
      if (disposed || !host.isConnected) return;
      try {
        fit.fit();
      } catch {
        return;
      }
      if (term.cols !== lastCols || term.rows !== lastRows) {
        lastCols = term.cols;
        lastRows = term.rows;
        void termResize(session.id, term.cols, term.rows).catch(() => undefined);
      }
    };

    const flush = () => {
      if (disposed || pending.length === 0) return;
      const data = pending;
      pending = "";
      void termWrite(session.id, data).catch(() => undefined);
    };

    const unsubscribe = registerWriter(session.id, (chunk) => term.write(chunk));
    const onData = term.onData((data) => {
      pending += data;
      if (inputFrame === 0) {
        inputFrame = requestAnimationFrame(() => {
          inputFrame = 0;
          flush();
        });
      }
    });
    const observer = new ResizeObserver(() => {
      if (fitFrame !== 0) cancelAnimationFrame(fitFrame);
      fitFrame = requestAnimationFrame(() => {
        fitFrame = 0;
        applyFit();
      });
    });
    observer.observe(host);

    void termScrollback(session.id)
      .then((text) => {
        if (disposed || text.length === 0) return;
        term.write(text.replace(/\n/g, "\r\n"));
        applyFit();
      })
      .catch(() => undefined);

    term.focus();

    return () => {
      disposed = true;
      flush();
      if (fitFrame !== 0) cancelAnimationFrame(fitFrame);
      if (inputFrame !== 0) cancelAnimationFrame(inputFrame);
      observer.disconnect();
      unsubscribe();
      onData.dispose();
      term.dispose();
    };
  }, [session.id]);

  return <div ref={hostRef} className="h-full w-full overflow-hidden bg-bg px-2 py-1.5" />;
};
