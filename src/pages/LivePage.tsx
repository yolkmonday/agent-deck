import { ActivityFeed } from "@/components/ActivityFeed";
import { KpiRow } from "@/components/KpiRow";
import { SessionCard } from "@/components/SessionCard";
import { useLive } from "@/store/live";

export const LivePage = ({
  onOpenTerminal,
  highlightSessionId = null,
  onHighlightDone,
}: {
  onOpenTerminal: (sessionId?: string) => void;
  highlightSessionId?: string | null;
  onHighlightDone?: () => void;
}) => {
  const snapshot = useLive((s) => s.snapshot);
  const events = useLive((s) => s.events);
  const sessions = snapshot?.sessions ?? [];
  const nowMs = snapshot?.generatedAtMs ?? Date.now();
  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <header className="flex items-center justify-between px-7 py-4.5">
        <div className="flex flex-col gap-0.75">
          <h1 className="text-[22px] font-semibold">Live</h1>
          <span className="flex items-center gap-2 text-[12.5px] text-fg-2">
            <span className="size-1.75 rounded-full bg-ok" />
            {sessions.length} sesi aktif
          </span>
        </div>
      </header>
      <div className="flex min-h-0 flex-1">
        <main className="flex min-w-0 flex-1 flex-col gap-6.5 overflow-y-auto px-7 pb-7">
          <KpiRow sessions={sessions} />
          {snapshot?.warnings.map((w) => (
            <div key={w} className="rounded-md border border-err/40 bg-err/10 px-3 py-2 text-xs text-err">
              {w}
            </div>
          ))}
          <div className="flex items-center justify-between">
            <span className="text-sm font-semibold">Sesi berjalan</span>
            <span className="text-xs text-fg-3">Urut: butuh input dulu, lalu paling baru aktif</span>
          </div>
          {sessions.length === 0 ? (
            <div className="rounded-[10px] border border-dashed border-border p-8 text-center text-sm text-fg-3">
              Belum ada agent yang berjalan. Buka claude atau opencode di terminal.
            </div>
          ) : (
            <div className="grid grid-cols-3 gap-4">
              {sessions.map((s) => (
                <SessionCard
                  key={s.id}
                  session={s}
                  nowMs={nowMs}
                  onOpenTerminal={onOpenTerminal}
                  highlighted={s.id === highlightSessionId}
                  onHighlightDone={onHighlightDone}
                />
              ))}
            </div>
          )}
        </main>
        <ActivityFeed events={events} />
      </div>
    </div>
  );
};
