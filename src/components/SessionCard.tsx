import { useEffect, useRef, useState } from "react";
import { Icon } from "@iconify/react";
import { AgentIcon } from "@/components/BrandIcon";
import { ClaudeThinking } from "@/components/brainless/claude/claude-thinking";
import { formatUsd } from "@/lib/cost";
import { formatDuration, formatShort, formatTokens, totalTokens } from "@/lib/format";
import type { Session } from "@/lib/types";
import { useTerminal } from "@/store/terminal";

const agentText = { claude: "text-claude", opencode: "text-opencode", codex: "text-codex" } as const;
const agentName = { claude: "Claude", opencode: "opencode", codex: "Codex" } as const;
const statusLabel = { busy: "Sibuk", waiting: "Menunggu", idle: "Diam" } as const;
const statusPill = {
  busy: "bg-busy/15 text-busy",
  waiting: "bg-waiting/15 text-waiting",
  idle: "bg-idle/15 text-idle",
} as const;
// Bundled offline in src/lib/icons.ts, so these resolve with no network call.
const statusIcon = {
  busy: "lucide:loader-circle",
  waiting: "lucide:message-circle-question",
  idle: "lucide:circle-pause",
} as const;
const healthIcon = { stalled: "lucide:octagon-alert", slow: "lucide:hourglass" } as const;

// A stalled session borrows the error colours and a slow one the waiting colours,
// so the board never shows three different greens for three very different states.
const healthPill = { stalled: "bg-err/15 text-err", slow: "bg-waiting/15 text-waiting" } as const;
const healthText = { stalled: "Macet", slow: "Lambat" } as const;

const activityText = (s: Session): { label: string; detail: string | null } => {
  const a = s.activity;
  if (!a) return { label: "Diam", detail: null };
  if (a.kind === "waiting") return { label: "Menunggu jawaban kamu", detail: a.detail };
  if (a.kind === "thinking") return { label: "Berpikir", detail: null };
  if (a.kind === "done") return { label: "Selesai", detail: null };
  return { label: a.label, detail: a.detail };
};

export const SessionCard = ({
  session: s,
  nowMs,
  onOpenTerminal,
  highlighted = false,
  onHighlightDone,
}: {
  session: Session;
  nowMs: number;
  onOpenTerminal: (sessionId?: string) => void;
  highlighted?: boolean;
  onHighlightDone?: () => void;
}) => {
  const waiting = s.status === "waiting";
  const stalled = s.health === "stalled";
  const slow = s.health === "slow";
  const act = activityText(s);
  const thinking = s.activity?.kind === "thinking" && s.health === "ok";
  const since = s.startedAtMs ? formatDuration(nowMs - s.startedAtMs) : "-";
  const terminalSessions = useTerminal((st) => st.sessions);
  const startSession = useTerminal((st) => st.start);
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!highlighted) return;
    rootRef.current?.scrollIntoView({ block: "center", behavior: "smooth" });
    const timer = setTimeout(() => onHighlightDone?.(), 1600);
    return () => clearTimeout(timer);
  }, [highlighted, onHighlightDone]);

  const owned = terminalSessions.find((t) => t.cwd === s.cwd);
  const openOwned = () => onOpenTerminal(owned?.id);

  const answerWaiting = () => {
    if (owned) {
      openOwned();
      return;
    }
    setNote("Sesi ini jalan di terminal lain. Buka di sana.");
  };

  const startNew = () => {
    const confirm = window.confirm(`Mulai ${s.agent} baru di ${s.cwd}? Ini proses baru, bukan lanjutan sesi ini.`);
    if (!confirm) return;
    setBusy(true);
    setNote(null);
    void startSession(s.agent, s.cwd)
      .then(() => onOpenTerminal())
      .catch(() => setNote("Gagal memulai sesi. Cek notifikasi di halaman Terminal."))
      .finally(() => setBusy(false));
  };
  return (
    <div
      ref={rootRef}
      className={`flex h-full min-w-0 flex-col gap-3 rounded-[10px] border p-4 ${
        stalled
          ? "border-err/40 bg-err/6"
          : waiting || slow
            ? "border-waiting/40 bg-waiting/6"
            : "border-border bg-surface"
      } ${highlighted ? "ring-2 ring-waiting/70" : ""}`}
    >
      <div className="flex items-center justify-between">
        <span className="flex items-center gap-1.5 text-xs font-semibold text-fg-2">
          <AgentIcon agent={s.agent} size={14} className={agentText[s.agent]} />
          {agentName[s.agent]}
        </span>
        {s.health === "ok" ? (
          <span className={`flex items-center gap-1.5 rounded-full px-2.25 py-0.75 text-[11.5px] font-semibold ${statusPill[s.status]}`}>
            <Icon
              icon={statusIcon[s.status]}
              width={12}
              height={12}
              className={s.status === "busy" ? "animate-spin [animation-duration:2s] motion-reduce:animate-none" : ""}
            />
            {statusLabel[s.status]}
          </span>
        ) : (
          <span className={`flex items-center gap-1.5 rounded-full px-2.25 py-0.75 text-[11.5px] font-semibold ${healthPill[s.health]}`}>
            <Icon icon={healthIcon[s.health]} width={12} height={12} />
            {healthText[s.health]}
          </span>
        )}
      </div>
      <div className="flex min-w-0 flex-col gap-0.75">
        <span className="truncate text-base font-semibold" title={s.project}>
          {s.project}
        </span>
        <span className="truncate font-mono text-[11.5px] text-fg-3" title={s.cwd}>
          {s.cwd}
        </span>
        <span className="truncate text-xs text-fg-2">
          {[s.model, s.branch].filter(Boolean).join(" · ") || "-"}
        </span>
      </div>
      <div className={`flex min-w-0 flex-col gap-1 rounded-md px-3 py-2.5 ${waiting ? "bg-waiting/10" : "bg-bg"}`}>
        {thinking ? (
          // Single verb, no token estimate, no interrupt hint: see the P14 plan for why.
          <ClaudeThinking verbs={["Berpikir"]} showTokens={false} hint={null} elapsedMs={s.quietMs} />
        ) : (
          <span className={`truncate text-[11.5px] font-semibold ${waiting ? "text-waiting" : "text-fg-3"}`}>
            {act.label}
            {s.toolRunningMs !== null && (
              <span className="font-normal text-fg-3"> · {formatShort(s.toolRunningMs)}</span>
            )}
          </span>
        )}
        {act.detail && (
          <span className="line-clamp-2 break-all font-mono text-xs leading-[1.4]" title={act.detail}>
            {act.detail}
          </span>
        )}
        {s.health !== "ok" && s.healthReason && (
          <span className={`truncate text-[11.5px] ${stalled ? "text-err" : "text-waiting"}`} title={s.healthReason}>
            {s.healthReason}
          </span>
        )}
      </div>
      <div className="mt-auto flex flex-col gap-2">
        <div className="flex items-center gap-3 font-mono text-[11.5px] whitespace-nowrap">
          <span className="text-fg-2">{formatTokens(totalTokens(s.tokens))}</span>
          {s.priced ? (
            <span className="text-fg-2">{formatUsd(s.costUsd)}</span>
          ) : (
            <span className="text-fg-3" title="Model ini belum ada di tabel harga.">
              -
            </span>
          )}
        </div>
        <div className="flex items-center justify-between gap-2">
          <span className="font-mono text-[11.5px] whitespace-nowrap text-fg-3">{since}</span>
          {waiting ? (
            <button
              type="button"
              onClick={answerWaiting}
              className="shrink-0 cursor-pointer rounded-md bg-waiting px-3 py-1.25 text-[12px] font-semibold text-bg"
            >
              Jawab
            </button>
          ) : (
            <button
              type="button"
              disabled={busy}
              onClick={startNew}
              className={`shrink-0 rounded-md border border-border px-3 py-1.25 text-[12px] font-semibold ${
                busy ? "cursor-default text-fg-3" : "cursor-pointer text-fg-2"
              }`}
            >
              Terminal
            </button>
          )}
        </div>
      </div>
      {note && <span className="text-[11.5px] text-fg-3">{note}</span>}
    </div>
  );
};
