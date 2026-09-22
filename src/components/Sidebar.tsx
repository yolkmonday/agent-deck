import { Radar } from "lucide-react";
import { NAV_ITEMS, type PageKey } from "@/lib/nav";

export const Sidebar = ({
  page,
  onSelect,
  onJumpWaiting,
  waitingCount,
}: {
  page: PageKey;
  onSelect: (page: PageKey) => void;
  onJumpWaiting: () => void;
  waitingCount: number;
}) => {
  return (
    <aside className="flex w-58 shrink-0 flex-col border-r border-border">
      <div className="flex items-center gap-2.5 px-5 py-5.5">
        <Radar size={20} className="text-busy" />
        <span className="text-[15px] font-semibold">Agent Deck</span>
      </div>
      <nav className="flex flex-col gap-0.5 px-3 py-1">
        {NAV_ITEMS.map(({ key, label, icon: Icon, enabled }) => {
          const active = key === page;
          return (
            <button
              key={key}
              type="button"
              disabled={!enabled}
              onClick={() => onSelect(key)}
              className={`flex items-center gap-2.5 rounded-lg px-3 py-2.25 text-left text-[13.5px] ${
                active ? "bg-surface-2 font-semibold text-fg" : "font-medium text-fg-3"
              } ${enabled ? "cursor-pointer" : "cursor-default opacity-45"}`}
            >
              <Icon size={16} />
              <span className="flex-1">{label}</span>
              {key === "live" && waitingCount > 0 && (
                <span
                  role="button"
                  tabIndex={0}
                  title="Buka sesi yang menunggu"
                  onClick={(e) => {
                    e.stopPropagation();
                    onJumpWaiting();
                  }}
                  onKeyDown={(e) => {
                    if (e.key !== "Enter" && e.key !== " ") return;
                    e.stopPropagation();
                    onJumpWaiting();
                  }}
                  className="cursor-pointer rounded-full bg-waiting px-1.75 py-0.5 text-[11px] font-semibold text-bg"
                >
                  {waitingCount} tunggu
                </span>
              )}
            </button>
          );
        })}
      </nav>
    </aside>
  );
};
