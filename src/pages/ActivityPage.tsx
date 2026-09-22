import { useState } from "react";
import { AgentIcon } from "@/components/BrandIcon";
import type { Agent } from "@/lib/types";
import { useLive } from "@/store/live";

const dot = { waiting: "bg-waiting", busy: "bg-busy", ok: "bg-ok", idle: "bg-idle" } as const;
const agentText = { claude: "text-claude", opencode: "text-opencode", codex: "text-codex" } as const;

const clock = (ms: number) =>
  new Date(ms).toLocaleTimeString("id-ID", { hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false });

const FILTERS: { key: Agent | "all"; label: string }[] = [
  { key: "all", label: "Semua" },
  { key: "claude", label: "Claude" },
  { key: "opencode", label: "opencode" },
  { key: "codex", label: "Codex" },
];

export const ActivityPage = () => {
  const events = useLive((s) => s.events);
  const [filter, setFilter] = useState<Agent | "all">("all");
  const rows = filter === "all" ? events : events.filter((e) => e.agent === filter);

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <header className="flex items-center justify-between px-7 py-4.5">
        <div className="flex flex-col gap-0.75">
          <h1 className="text-[22px] font-semibold">Aktivitas</h1>
          <span className="flex items-center gap-2 text-[12.5px] text-fg-2">
            <span className="size-1.75 rounded-full bg-ok" />
            {rows.length} kejadian sejak app dibuka
          </span>
        </div>
        <div className="flex items-center gap-0.5 rounded-lg border border-border bg-surface p-0.75">
          {FILTERS.map((f) => (
            <button
              key={f.key}
              type="button"
              onClick={() => setFilter(f.key)}
              className={`ad-interactive ad-press cursor-pointer rounded-md px-3 py-1.5 text-[12.5px] hover:text-fg ${
                filter === f.key ? "bg-surface-2 font-semibold text-fg" : "font-medium text-fg-2"
              }`}
            >
              {f.label}
            </button>
          ))}
        </div>
      </header>

      <main className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-7 pb-7">
        {rows.length === 0 ? (
          <div className="rounded-[10px] border border-dashed border-border p-8 text-center text-sm text-fg-3">
            Belum ada kejadian. Aktivitas agent akan muncul di sini selama app terbuka.
          </div>
        ) : (
          rows.map((e) => (
            <div key={e.id} className="flex min-w-0 items-start gap-3 border-b border-border pb-3">
              <span className="w-[68px] shrink-0 font-mono text-[11.5px] whitespace-nowrap text-fg-3">
                {clock(e.timeMs)}
              </span>
              <span className={`mt-1 size-1.75 shrink-0 rounded-full ${dot[e.color]}`} />
              <AgentIcon agent={e.agent} size={14} className={`mt-0.5 shrink-0 ${agentText[e.agent]}`} />
              <span className="w-[160px] shrink-0 truncate text-[12.5px] font-semibold" title={e.project}>
                {e.project}
              </span>
              <span className="min-w-0 flex-1 break-words font-mono text-xs leading-[1.5] text-fg-2">{e.text}</span>
            </div>
          ))
        )}
      </main>
    </div>
  );
};
