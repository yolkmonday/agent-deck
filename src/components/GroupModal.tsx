import { useEffect } from "react";
import { Icon } from "@iconify/react";
import { AgentIcon } from "@/components/BrandIcon";
import { formatNotional, formatUsd, NOTIONAL_HINT } from "@/lib/cost";
import { formatTokens, totalTokens } from "@/lib/format";
import type { Session } from "@/lib/types";

const statusLabel = { busy: "Sibuk", waiting: "Menunggu", idle: "Diam" } as const;
const statusPill = {
  busy: "bg-busy/15 text-busy",
  waiting: "bg-waiting/15 text-waiting",
  idle: "bg-idle/15 text-idle",
} as const;
const statusIcon = {
  busy: "lucide:loader-circle",
  waiting: "lucide:message-circle-question",
  idle: "lucide:circle-pause",
} as const;
const healthPill = { stalled: "bg-err/15 text-err", slow: "bg-waiting/15 text-waiting" } as const;
const healthIcon = { stalled: "lucide:octagon-alert", slow: "lucide:hourglass" } as const;
const healthText = { stalled: "Macet", slow: "Lambat" } as const;
const agentText = { claude: "text-claude", opencode: "text-opencode", codex: "text-codex" } as const;

const Row = ({ s, onOpenTerminal }: { s: Session; onOpenTerminal: (id?: string) => void }) => (
  <div className="flex min-w-0 items-center gap-3 border-b border-border py-3 last:border-0">
    <AgentIcon agent={s.agent} size={14} className={`shrink-0 ${agentText[s.agent]}`} />
    <div className="flex min-w-0 flex-1 flex-col gap-0.5">
      <span className="flex min-w-0 items-center gap-1.5">
        <span className="truncate text-[13px] font-semibold" title={s.project}>
          {s.project}
        </span>
        {s.isWorktree && s.worktreeName && (
          <span className="flex min-w-0 items-center gap-1 rounded-full bg-bg px-1.5 py-0.5">
            <Icon icon="lucide:folder-git-2" width={11} height={11} className="shrink-0 text-fg-3" />
            <span className="truncate font-mono text-[11px] text-fg-3">{s.worktreeName}</span>
          </span>
        )}
      </span>
      <span className="truncate text-[11.5px] text-fg-3" title={s.cwd}>
        {[s.model, s.branch].filter(Boolean).join(" · ") || s.cwd}
      </span>
    </div>
    {s.health === "ok" ? (
      <span
        className={`flex shrink-0 items-center gap-1.5 rounded-full px-2.25 py-0.75 text-[11.5px] font-semibold ${statusPill[s.status]}`}
      >
        <Icon icon={statusIcon[s.status]} width={12} height={12} />
        {statusLabel[s.status]}
      </span>
    ) : (
      <span
        className={`flex shrink-0 items-center gap-1.5 rounded-full px-2.25 py-0.75 text-[11.5px] font-semibold ${healthPill[s.health]}`}
      >
        <Icon icon={healthIcon[s.health]} width={12} height={12} />
        {healthText[s.health]}
      </span>
    )}
    <span className="shrink-0 font-mono text-[11.5px] whitespace-nowrap text-fg-2">
      {formatTokens(totalTokens(s.tokens))}
    </span>
    {!s.priced ? (
      <span className="shrink-0 font-mono text-[11.5px] text-fg-3" title="Model ini belum ada di tabel harga.">
        -
      </span>
    ) : s.billingMode === "subscription" ? (
      <span className="shrink-0 font-mono text-[11.5px] whitespace-nowrap text-fg-3" title={NOTIONAL_HINT}>
        {formatNotional(s.costUsd)}
      </span>
    ) : (
      <span className="shrink-0 font-mono text-[11.5px] whitespace-nowrap text-fg-2">{formatUsd(s.costUsd)}</span>
    )}
    <button
      type="button"
      onClick={() => onOpenTerminal()}
      className="ad-interactive ad-press shrink-0 cursor-pointer rounded-md border border-border px-2.5 py-1 text-[11.5px] font-semibold text-fg-2 hover:border-fg-3 hover:text-fg"
    >
      Terminal
    </button>
  </div>
);

export const GroupModal = ({
  label,
  sessions,
  onClose,
  onOpenTerminal,
}: {
  label: string;
  sessions: Session[];
  onClose: () => void;
  onOpenTerminal: (sessionId?: string) => void;
}) => {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      className="ad-fade fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-6"
      role="dialog"
      aria-modal="true"
      aria-label={`Sesi lain di ${label}`}
      onClick={onClose}
    >
      <div
        className="ad-rise flex max-h-[80vh] w-full max-w-[760px] flex-col rounded-xl border border-border bg-surface"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between border-b border-border px-5 py-3.5">
          <span className="flex items-center gap-2 text-sm font-semibold">
            <Icon icon="lucide:folder-git-2" width={14} height={14} className="text-fg-3" />
            {label}
            <span className="font-normal text-fg-3">· {sessions.length} sesi lain</span>
          </span>
          <button
            type="button"
            onClick={onClose}
            aria-label="Tutup"
            className="ad-interactive ad-press cursor-pointer text-fg-3 hover:text-fg"
          >
            <Icon icon="lucide:x" width={16} height={16} />
          </button>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-1">
          {sessions.map((s) => (
            <Row key={s.id} s={s} onOpenTerminal={onOpenTerminal} />
          ))}
        </div>
      </div>
    </div>
  );
};
