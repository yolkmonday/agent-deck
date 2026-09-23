import { useState } from "react";
import { GroupModal } from "@/components/GroupModal";
import { KpiRow } from "@/components/KpiRow";
import { SessionCard } from "@/components/SessionCard";
import { useT } from "@/i18n";
import { groupCards } from "@/lib/grouping";
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
  const t = useT();
  const snapshot = useLive((s) => s.snapshot);
  const sessions = snapshot?.sessions ?? [];
  const nowMs = snapshot?.generatedAtMs ?? Date.now();
  const cards = groupCards(sessions);
  const [openKey, setOpenKey] = useState<string | null>(null);
  const open = cards.find((c) => c.key === openKey) ?? null;

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <header className="flex items-center justify-between px-7 py-4.5">
        <div className="flex flex-col gap-0.75">
          <h1 className="text-[22px] font-semibold">{t("livePage.title")}</h1>
          <span className="flex items-center gap-2 text-[12.5px] text-fg-2">
            <span className="size-1.75 rounded-full bg-ok" />
            {t(sessions.length === 1 ? "livePage.activeSessions.one" : "livePage.activeSessions.other", {
              n: sessions.length,
            })}
            {cards.length !== sessions.length && (
              <span className="text-fg-3">
                · {t(cards.length === 1 ? "livePage.projectCount.one" : "livePage.projectCount.other", { n: cards.length })}
              </span>
            )}
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
            <span className="text-sm font-semibold">{t("livePage.runningSessions")}</span>
            <span className="text-xs text-fg-3">{t("livePage.sortHint")}</span>
          </div>
          {sessions.length === 0 ? (
            <div className="rounded-[10px] border border-dashed border-border p-8 text-center text-sm text-fg-3">
              {t("livePage.empty")}
            </div>
          ) : (
            <div className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] items-stretch gap-4">
              {cards.map((c) => (
                <SessionCard
                  key={c.key}
                  session={c.primary}
                  nowMs={nowMs}
                  onOpenTerminal={onOpenTerminal}
                  highlighted={c.primary.id === highlightSessionId}
                  onHighlightDone={onHighlightDone}
                  siblingCount={c.children.length}
                  siblingWaiting={c.waitingChildren}
                  onOpenSiblings={() => setOpenKey(c.key)}
                />
              ))}
            </div>
          )}
        </main>
      </div>
      {open && (
        <GroupModal
          label={open.label}
          sessions={open.children}
          onClose={() => setOpenKey(null)}
          onOpenTerminal={onOpenTerminal}
        />
      )}
    </div>
  );
};
