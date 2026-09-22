import { useEffect, useMemo, useState } from "react";
import { Sidebar } from "@/components/Sidebar";
import { NavProvider, type PageKey } from "@/lib/nav";
import { LivePage } from "@/pages/LivePage";
import { ProviderEditPage } from "@/pages/ProviderEditPage";
import { ProviderOverviewPage } from "@/pages/ProviderOverviewPage";
import { ProjectPage } from "@/pages/ProjectPage";
import { SavingsPage } from "@/pages/SavingsPage";
import { TerminalPage } from "@/pages/TerminalPage";
import { TimelinePage } from "@/pages/TimelinePage";
import { TokenPage } from "@/pages/TokenPage";
import { startLive } from "@/store/live";
import { startTerminalEvents } from "@/store/terminal";

const App = () => {
  const [page, setPage] = useState<PageKey>("live");
  const [terminalTarget, setTerminalTarget] = useState<string | null>(null);
  const [providerTarget, setProviderTarget] = useState<string | null>(null);

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

  const goToTerminal = (sessionId?: string) => {
    setTerminalTarget(sessionId ?? null);
    setPage("terminal");
  };

  return (
    <NavProvider value={nav}>
      <div className="flex h-full">
        <Sidebar page={page} onSelect={setPage} />
        {page === "live" && <LivePage onOpenTerminal={goToTerminal} />}
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
    </NavProvider>
  );
};

export default App;
