import { useEffect, useRef, useState } from "react";
import { Icon } from "@iconify/react";
import { AgentIcon } from "@/components/BrandIcon";
import { ClaudeThinking } from "@/components/brainless/claude/claude-thinking";
import { RecoverMenu } from "@/components/RecoverMenu";
import { TranscriptModal } from "@/components/TranscriptModal";
import { formatUsd, formatNotional, NOTIONAL_HINT } from "@/lib/cost";
import { formatDuration, formatShort, formatTokens, totalTokens } from "@/lib/format";
import type { Session, SubAgent } from "@/lib/types";
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

const SubAgentRow = ({ sub }: { sub: SubAgent }) => (
  <div className="flex min-w-0 items-center gap-2 pt-1.5">
    <Icon icon="lucide:git-branch" width={12} height={12} className="shrink-0 text-fg-3" />
    <span className="shrink-0 text-[11.5px] font-semibold text-fg-2">{sub.agentType}</span>
    <span className="min-w-0 flex-1 truncate text-[11.5px] text-fg-3" title={sub.description}>
      {sub.description}
    </span>
    <span className="shrink-0 font-mono text-[11.5px] whitespace-nowrap text-fg-2">
      {formatTokens(totalTokens(sub.tokens))}
    </span>
    {sub.priced ? (
      <span className="shrink-0 font-mono text-[11.5px] whitespace-nowrap text-fg-2">
        {formatUsd(sub.costUsd)}
      </span>
    ) : (
      <span className="shrink-0 font-mono text-[11.5px] text-fg-3" title="Model ini belum ada di tabel harga.">
        -
      </span>
    )}
  </div>
);

export const SessionCard = ({
  session: s,
  nowMs,
  onOpenTerminal,
  highlighted = false,
  onHighlightDone,
  siblingCount = 0,
  siblingWaiting = 0,
  onOpenSiblings,
}: {
  session: Session;
  nowMs: number;
  onOpenTerminal: (sessionId?: string) => void;
  highlighted?: boolean;
  onHighlightDone?: () => void;
  siblingCount?: number;
  siblingWaiting?: number;
  onOpenSiblings?: () => void;
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
  const [subagentsOpen, setSubagentsOpen] = useState(false);
  const [confirmStart, setConfirmStart] = useState(false);
  const [transcriptOpen, setTranscriptOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const transcriptRef = useRef<HTMLButtonElement | null>(null);

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

  // Not `window.confirm`: Tauri v2 routes it to the dialog plugin, which needs a
  // permission we do not grant, and it would block the webview while the live
  // loop keeps running behind it. A two-step button says the same thing.
  const startNew = () => {
    if (!confirmStart) {
      setConfirmStart(true);
      setNote(`Mulai ${s.agent} baru di ${s.cwd}? Ini proses baru, bukan lanjutan sesi ini.`);
      return;
    }
    setConfirmStart(false);
    setBusy(true);
    setNote(null);
    void startSession(s.agent, s.cwd)
      .then(() => onOpenTerminal())
      .catch(() => setNote("Gagal memulai sesi. Cek notifikasi di halaman Terminal."))
      .finally(() => setBusy(false));
  };

  const cancelStart = () => {
    setConfirmStart(false);
    setNote(null);
  };
  return (
    <div
      ref={rootRef}
      className={`ad-rise flex h-full min-w-0 flex-col gap-3 rounded-[10px] border p-4 ${
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
      <button
        type="button"
        ref={transcriptRef}
        onClick={() => setTranscriptOpen(true)}
        className="flex min-w-0 cursor-pointer flex-col gap-0.75 text-left"
      >
        <span className="truncate text-base font-semibold" title={s.project}>
          {s.project}
        </span>
        <span className="truncate font-mono text-[11.5px] text-fg-3" title={s.cwd}>
          {s.cwd}
        </span>
        <span className="flex min-w-0 items-center gap-1 text-xs text-fg-2">
          <span className="truncate">
            {[s.model, s.branch].filter(Boolean).join(" · ") || "-"}
          </span>
          {s.branch && <Icon icon="lucide:git-branch" width={12} height={12} className="shrink-0 text-fg-3" />}
          {s.isWorktree && s.worktreeName && (
            <span className="flex min-w-0 items-center gap-1 rounded-full bg-bg px-1.5 py-0.5">
              <Icon icon="lucide:folder-git-2" width={12} height={12} className="shrink-0 text-fg-3" />
              <span className="truncate font-mono text-[11.5px] text-fg-3" title={s.worktreeName}>
                {s.worktreeName}
              </span>
            </span>
          )}
        </span>
      </button>
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
      {siblingCount > 0 && (
        <button
          type="button"
          onClick={onOpenSiblings}
          className={`ad-interactive ad-press flex cursor-pointer items-center gap-1.5 border-t border-border pt-1.5 text-left text-[11.5px] ${
            siblingWaiting > 0 ? "text-waiting" : "ad-interactive text-fg-3 hover:text-fg-2"
          }`}
        >
          <Icon icon="lucide:folder-git-2" width={12} height={12} />
          {siblingCount} worktree jalan
          {siblingWaiting > 0 && <span className="font-semibold">· {siblingWaiting} butuh jawaban</span>}
          <Icon icon="lucide:chevron-right" width={12} height={12} className="ml-auto" />
        </button>
      )}
      {/* Collapsed by default: a card whose height changed every time a subagent
          started or finished made the whole board jump around. One stable line. */}
      {s.subagents.length > 0 && (
        <div className="flex min-w-0 flex-col border-t border-border pt-1.5">
          <button
            type="button"
            onClick={() => setSubagentsOpen((v) => !v)}
            aria-expanded={subagentsOpen}
            className="ad-interactive ad-press flex cursor-pointer items-center gap-1.5 text-left text-[11.5px] text-fg-3 hover:text-fg-2"
          >
            <Icon
              icon="lucide:chevron-right"
              width={12}
              height={12}
              className={subagentsOpen ? "rotate-90" : ""}
            />
            {s.subagents.length} subagent jalan
          </button>
          {subagentsOpen && (
            <div className="mt-1 flex min-w-0 flex-col divide-y divide-border">
              {s.subagents.map((sub) => (
                <SubAgentRow key={sub.id} sub={sub} />
              ))}
            </div>
          )}
        </div>
      )}
      <div className="mt-auto flex flex-col gap-2">
        <div className="flex items-center gap-3 font-mono text-[11.5px] whitespace-nowrap">
          <span className="text-fg-2">{formatTokens(totalTokens(s.tokens))}</span>
          {!s.priced ? (
            <span className="text-fg-3" title="Model ini belum ada di tabel harga.">
              -
            </span>
          ) : s.billingMode === "subscription" ? (
            // A subscription's tokens cost nothing extra, so the figure is an
            // estimate at API rates and must never read like a bill.
            <span className="text-fg-3" title={NOTIONAL_HINT}>
              {formatNotional(s.costUsd)}
            </span>
          ) : (
            <span className="text-fg-2">{formatUsd(s.costUsd)}</span>
          )}
        </div>
        <div className="flex items-center justify-between gap-2">
          <span className="font-mono text-[11.5px] whitespace-nowrap text-fg-3">{since}</span>
          <RecoverMenu session={s} onOpenTerminal={onOpenTerminal} />
          {waiting ? (
            <button
              type="button"
              onClick={answerWaiting}
              className="ad-interactive ad-press shrink-0 cursor-pointer rounded-md bg-waiting px-3 py-1.25 text-[12px] font-semibold text-bg hover:opacity-90"
            >
              Jawab
            </button>
          ) : (
            <button
              type="button"
              disabled={busy}
              onClick={startNew}
              className={`ad-interactive ad-press shrink-0 rounded-md border px-3 py-1.25 text-[12px] font-semibold ${
                confirmStart ? "border-waiting text-waiting" : "ad-interactive border-border hover:border-fg-3 hover:text-fg"
              } ${busy ? "cursor-default text-fg-3" : "cursor-pointer"} ${
                !confirmStart && !busy ? "text-fg-2" : ""
              }`}
            >
              {confirmStart ? "Yakin?" : "Terminal"}
            </button>
          )}
        </div>
      </div>
      {note && (
        <span className="flex items-center gap-2 text-[11.5px] text-fg-3">
          <span className="min-w-0 flex-1">{note}</span>
          {confirmStart && (
            <button type="button" onClick={cancelStart} className="ad-interactive ad-press shrink-0 cursor-pointer underline hover:text-fg">
              Batal
            </button>
          )}
        </span>
      )}
      {transcriptOpen && (
        <TranscriptModal
          session={s}
          onClose={() => {
            setTranscriptOpen(false);
            // WKWebView does not focus a button on click, so the card hands
            // focus back to itself rather than relying on the modal's memory.
            transcriptRef.current?.focus();
          }}
        />
      )}
    </div>
  );
};
