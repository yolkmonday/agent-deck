import { RecoverMenu } from "@/components/RecoverMenu";
import { useT } from "@/i18n";
import type { Orphan, Session } from "@/lib/types";
import { healthLabel, orphanLabel, waitingLabel } from "@/lib/attention";

export const AttentionBar = ({
  sessions,
  unhealthy,
  orphans,
  nowMs,
  onJump,
  onOpenTerminal,
}: {
  sessions: Session[];
  unhealthy: Session[];
  orphans: Orphan[];
  nowMs: number;
  onJump: () => void;
  onOpenTerminal: (termId: string) => void;
}) => {
  const t = useT();
  const first = sessions[0];
  const bad = unhealthy[0];
  if (!first && !bad && orphans.length === 0) return null;

  // Waiting stays the loudest thing on screen; a stuck agent comes second.
  const tone = first ? "waiting" : "err";
  const text = first
    ? waitingLabel(first, nowMs)
    : bad
      ? healthLabel(bad)
      : orphanLabel(orphans[0]);
  const extra = (first ? sessions.length - 1 : 0) + (first ? 0 : unhealthy.length - 1);
  const jumpable = Boolean(first || bad);
  return (
    <div
      className={`flex flex-col border-b ${
        tone === "waiting" ? "border-waiting/40 bg-waiting/10" : "border-err/40 bg-err/10"
      }`}
    >
      <div className="flex items-center gap-3 px-7 py-2.5">
        <span className={`size-2 shrink-0 rounded-full ${tone === "waiting" ? "bg-waiting" : "bg-err"}`} />
        <span className={`truncate text-[13px] font-semibold ${tone === "waiting" ? "text-waiting" : "text-err"}`}>
          {text}
        </span>
        {extra > 0 && (
          <span className="shrink-0 text-[12.5px] text-fg-2">{t("attentionBar.andMore", { n: extra })}</span>
        )}
        {jumpable && (
          <button
            type="button"
            onClick={onJump}
            className={`ad-interactive ad-press ml-auto shrink-0 cursor-pointer rounded-md px-3 py-1.25 text-[12px] font-semibold text-bg hover:opacity-90 ${
              tone === "waiting" ? "bg-waiting" : "bg-err"
            }`}
          >
            {t("attentionBar.open")}
          </button>
        )}
        {!first && bad && bad.health === "stalled" && (
          <RecoverMenu session={bad} onOpenTerminal={onOpenTerminal} />
        )}
      </div>
      {orphans.map((o) => (
        <div key={`${o.pid}-${o.cwd}`} className="flex items-center gap-3 border-t border-err/25 px-7 py-1.5">
          <span className="size-2 shrink-0 rounded-full bg-err/50" />
          <span className="truncate text-[12.5px] text-err">{orphanLabel(o)}</span>
        </div>
      ))}
    </div>
  );
};
