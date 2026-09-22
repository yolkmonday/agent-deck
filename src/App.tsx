import { useEffect, useState } from "react";
import { Sidebar } from "@/components/Sidebar";
import type { PageKey } from "@/lib/nav";
import { LivePage } from "@/pages/LivePage";
import { TokenPage } from "@/pages/TokenPage";
import { startLive } from "@/store/live";

const App = () => {
  const [page, setPage] = useState<PageKey>("live");
  useEffect(() => {
    const stop = startLive();
    return () => {
      void stop.then((fn) => fn());
    };
  }, []);
  return (
    <div className="flex h-full">
      <Sidebar page={page} onSelect={setPage} />
      {page === "token" ? <TokenPage /> : <LivePage />}
    </div>
  );
};

export default App;
