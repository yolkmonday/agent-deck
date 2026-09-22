import { useEffect } from "react";
import { Sidebar } from "@/components/Sidebar";
import { LivePage } from "@/pages/LivePage";
import { startLive } from "@/store/live";

const App = () => {
  useEffect(() => {
    const stop = startLive();
    return () => {
      void stop.then((fn) => fn());
    };
  }, []);
  return (
    <div className="flex h-full">
      <Sidebar />
      <LivePage />
    </div>
  );
};

export default App;
