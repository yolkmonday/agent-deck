import { useEffect, useMemo, useRef, useState } from "react";
import { AttentionBar } from "@/components/AttentionBar";
import { Sidebar } from "@/components/Sidebar";
import {
  type JumpTarget,
  newlyStalled,
  newlyWaiting,
  resolveTarget,
  unhealthySessions,
  waitingSessions,
} from "@/lib/attention";
import { DEFAULT_SETTINGS, settingsGet, settingsSet, windowFocused, type Settings } from "@/lib/api";
import { NavProvider, type PageKey } from "@/lib/nav";
import { notifyStalled, notifyWaiting } from "@/lib/notify";
import type { Orphan, Session } from "@/lib/types";
import { LivePage } from "@/pages/LivePage";
import { ProviderEditPage } from "@/pages/ProviderEditPage";
import { ProviderOverviewPage } from "@/pages/ProviderOverviewPage";
import { ProjectPage } from "@/pages/ProjectPage";
import { SavingsPage } from "@/pages/SavingsPage";
import { TerminalPage } from "@/pages/TerminalPage";
import { TimelinePage } from "@/pages/TimelinePage";
import { TokenPage } from "@/pages/TokenPage";
import { useLive, startLive } from "@/store/live";
import { focusTerminal, startTerminalEvents, useTerminal } from "@/store/terminal";

// Stable reference: a fresh [] in the selector makes zustand see a new snapshot
// on every render and loop forever while the first live snapshot is still null.
const NO_SESSIONS: Session[] = [];
const NO_ORPHANS: Orphan[] = [];

const App = () => {
  const [page, setPage] = useState<PageKey>("live");
  const [terminalTarget, setTerminalTarget] = useState<string | null>(null);
  const [highlightSessionId, setHighlightSessionId] = useState<string | null>(null);
  const [providerTarget, setProviderTarget] = useState<string | null>(null);
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const prevSessions = useRef<Session[] | null>(null);
  const notified = useRef(new Set<string>());
  const notifiedStalled = useRef(new Set<string>());

  useEffect(() => {
    const stop = startLive();
    return () => {
      void stop.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    const stop = startTerminalEvents();
    return () => {
      void stop.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    settingsGet()
      .then(setSettings)
      .catch(() => undefined);
  }, []);

  const saveSettings = (next: Settings) => {
    setSettings(next);
    void settingsSet(next).catch(() => undefined);
  };

  const goToTarget = (target: JumpTarget) => {
    if (target.kind === "terminal") {
      setTerminalTarget(target.termId);
      setHighlightSessionId(null);
      setPage("terminal");
      focusTerminal(target.termId);
      return;
    }
    setHighlightSessionId(target.sessionId);
    setTerminalTarget(null);
    setPage("live");
  };

  const goToTerminal = (sessionId?: string) => {
    setTerminalTarget(sessionId ?? null);
    setHighlightSessionId(null);
    setPage("terminal");
  };

  const sessions = useLive((s) => s.snapshot?.sessions ?? NO_SESSIONS);
  const terms = useTerminal((s) => s.sessions);
  const termsRef = useRef(terms);
  termsRef.current = terms;
  const settingsRef = useRef(settings);
  settingsRef.current = settings;

  useEffect(() => {
    const fresh = newlyWaiting(prevSessions.current, sessions);
    const stalled = newlyStalled(prevSessions.current, sessions);
    prevSessions.current = sessions;

    const mode = settingsRef.current.attentionMode;
    if (mode === "off") return;

    if (mode === "auto") {
      const target = fresh.find((s) => !notified.current.has(s.id)) ?? null;
      if (target) {
        notified.current.add(target.id);
        goToTarget(resolveTarget(target, termsRef.current));
        return;
      }
      // There is nowhere useful to jump to for a stalled agent, so it only notifies.
    }

    for (const s of fresh) {
      if (notified.current.has(s.id)) continue;
      notified.current.add(s.id);
      void windowFocused()
        .catch(() => true)
        .then((focused) => (focused ? undefined : notifyWaiting(s).then(() => undefined)));
    }

    for (const s of stalled) {
      if (notifiedStalled.current.has(s.id)) continue;
      notifiedStalled.current.add(s.id);
      void windowFocused()
        .catch(() => true)
        .then((focused) => (focused ? undefined : notifyStalled(s).then(() => undefined)));
    }
  }, [sessions]);

  const nav = useMemo(
    () => ({
      provider: (id: string) => {
        setProviderTarget(id === "" ? null : id);
        setPage("provider");
      },
      project: () => {
        setPage("project");
      },
    }),
    [],
  );

  const waiting = waitingSessions(sessions);
  const unhealthy = unhealthySessions(sessions);
  const orphans = useLive((s) => s.snapshot?.orphans ?? NO_ORPHANS);
  const nowMs = useLive((s) => s.snapshot?.generatedAtMs ?? Date.now());
  const firstJump = () => {
    if (waiting.length > 0) {
      goToTarget(resolveTarget(waiting[0], termsRef.current));
      return;
    }
    if (unhealthy.length > 0) goToTarget(resolveTarget(unhealthy[0], termsRef.current));
  };

  return (
    <NavProvider value={nav}>
      <div className="flex h-full flex-col">
        <AttentionBar
          sessions={waiting}
          unhealthy={unhealthy}
          orphans={orphans}
          nowMs={nowMs}
          settings={settings}
          onJump={firstJump}
          onMode={(mode) => saveSettings({ ...settings, attentionMode: mode })}
          onNotifySound={(notifySound) => saveSettings({ ...settings, notifySound })}
          onMinutes={(value) => saveSettings({ ...settings, ...value })}
        />
        <div className="flex min-h-0 flex-1">
          <Sidebar page={page} onSelect={setPage} onJumpWaiting={firstJump} waitingCount={waiting.length} />
          {page === "live" && (
            <LivePage
              onOpenTerminal={goToTerminal}
              highlightSessionId={highlightSessionId}
              onHighlightDone={() => setHighlightSessionId(null)}
            />
          )}
          {page === "token" && <TokenPage />}
          {page === "timeline" && <TimelinePage />}
          {page === "savings" && <SavingsPage />}
          {page === "terminal" && <TerminalPage initialSessionId={terminalTarget} />}
          {page === "provider" &&
            (providerTarget === null ? (
              <ProviderOverviewPage onNew={() => nav.provider("__new__")} />
            ) : (
              <ProviderEditPage providerId={providerTarget === "__new__" ? null : providerTarget} />
            ))}
          {page === "project" && <ProjectPage />}
        </div>
      </div>
    </NavProvider>
  );
};

export default App;
