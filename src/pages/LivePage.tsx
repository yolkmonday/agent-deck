import { Icon } from "@iconify/react";
import { KpiRow } from "@/components/KpiRow";
import { SessionCard } from "@/components/SessionCard";
import { groupSessions } from "@/lib/grouping";
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
  const sessions = snapshot?.sessions ?? [];
  const nowMs = snapshot?.generatedAtMs ?? Date.now();
  const groups = groupSessions(sessions);
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
      <div className="flex min-h-0 min-w-0 flex-1 overflow-hidden">
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
            <div className="flex flex-col gap-6.5">
              {groups.map((g) => (
                <div key={g.key} className="flex flex-col gap-3">
                  {(g.sessions.length > 1 || g.sessions.some((s) => s.isWorktree)) && (
                    <span className="flex items-center gap-1.5 text-[12.5px] font-semibold text-fg-2">
                      {g.sessions.some((s) => s.isWorktree) && (
                        <Icon icon="lucide:folder-git-2" width={12} height={12} className="text-fg-3" />
                      )}
                      {g.label}
                      <span className="font-normal text-fg-3">· {g.sessions.length} sesi</span>
                    </span>
                  )}
                  <div className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] items-stretch gap-4">
                    {g.sessions.map((s) => (
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
                </div>
              ))}
            </div>
          )}
        </main>
      </div>
    </div>
  );
};
