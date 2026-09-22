import { Activity, BarChart3, Cpu, History, PiggyBank, SquareTerminal, Radar } from "lucide-react";
import { useLive } from "@/store/live";

const items = [
  { label: "Live", icon: Activity, active: true },
  { label: "Token & Biaya", icon: BarChart3 },
  { label: "Timeline", icon: History },
  { label: "Hemat Token", icon: PiggyBank },
  { label: "Terminal", icon: SquareTerminal },
  { label: "Model & Provider", icon: Cpu },
];

export const Sidebar = () => {
  const waiting = useLive((s) => s.snapshot?.sessions.filter((x) => x.status === "waiting").length ?? 0);
  return (
    <aside className="flex w-58 shrink-0 flex-col border-r border-border">
      <div className="flex items-center gap-2.5 px-5 py-5.5">
        <Radar size={20} className="text-busy" />
        <span className="text-[15px] font-semibold">Agent Deck</span>
      </div>
      <nav className="flex flex-col gap-0.5 px-3 py-1">
        {items.map(({ label, icon: Icon, active }) => (
          <div
            key={label}
            className={`flex items-center gap-2.5 rounded-lg px-3 py-2.25 text-[13.5px] ${
              active ? "bg-surface-2 font-semibold text-fg" : "font-medium text-fg-3"
            }`}
          >
            <Icon size={16} />
            <span className="flex-1">{label}</span>
            {active && waiting > 0 && (
              <span className="rounded-full bg-waiting px-1.75 py-0.5 text-[11px] font-semibold text-bg">
                {waiting} tunggu
              </span>
            )}
          </div>
        ))}
      </nav>
    </aside>
  );
};
