import { useEffect, useState } from "react";
import { Sidebar } from "@/components/Sidebar";
import type { PageKey } from "@/lib/nav";
import { LivePage } from "@/pages/LivePage";
import { SavingsPage } from "@/pages/SavingsPage";
import { TimelinePage } from "@/pages/TimelinePage";
import { TokenPage } from "@/pages/TokenPage";
import { startLive } from "@/store/live";

const PAGES: Record<PageKey, () => React.ReactElement> = {
  live: LivePage,
  token: TokenPage,
  timeline: TimelinePage,
  savings: SavingsPage,
  terminal: LivePage,
  provider: LivePage,
};

const App = () => {
  const [page, setPage] = useState<PageKey>("live");
  useEffect(() => {
    const stop = startLive();
    return () => {
      void stop.then((fn) => fn());
    };
  }, []);
  const Page = PAGES[page];
  return (
    <div className="flex h-full">
      <Sidebar page={page} onSelect={setPage} />
      <Page />
    </div>
  );
};

export default App;
