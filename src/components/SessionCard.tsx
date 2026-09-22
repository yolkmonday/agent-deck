import { useState } from "react";
import { formatDuration, formatTokens, totalTokens } from "@/lib/format";
import type { Session } from "@/lib/types";
import { useTerminal } from "@/store/terminal";

const agentColor = { claude: "bg-claude", opencode: "bg-opencode", codex: "bg-codex" } as const;
const agentName = { claude: "Claude", opencode: "opencode", codex: "Codex" } as const;
const statusLabel = { busy: "Sibuk", waiting: "Menunggu", idle: "Diam" } as const;
const statusPill = {
  busy: "bg-busy/15 text-busy",
  waiting: "bg-waiting/15 text-waiting",
  idle: "bg-idle/15 text-idle",
} as const;
const dot = { busy: "bg-busy", waiting: "bg-waiting", idle: "bg-idle" } as const;

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
}: {
  session: Session;
  nowMs: number;
  onOpenTerminal: (sessionId?: string) => void;
}) => {
  const waiting = s.status === "waiting";
  const act = activityText(s);
  const since = s.startedAtMs ? formatDuration(nowMs - s.startedAtMs) : "-";
  const terminalSessions = useTerminal((st) => st.sessions);
  const startSession = useTerminal((st) => st.start);
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

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
    <div className={`flex flex-col gap-3 rounded-[10px] border p-4 ${waiting ? "border-waiting/40 bg-waiting/6" : "border-border bg-surface"}`}>
      <div className="flex items-center justify-between">
        <span className="flex items-center gap-1.5 text-xs font-semibold text-fg-2">
          <span className={`size-2 rounded-full ${agentColor[s.agent]}`} />
          {agentName[s.agent]}
        </span>
        <span className={`flex items-center gap-1.5 rounded-full px-2.25 py-0.75 text-[11.5px] font-semibold ${statusPill[s.status]}`}>
          <span className={`size-1.5 rounded-full ${dot[s.status]}`} />
          {statusLabel[s.status]}
        </span>
      </div>
      <div className="flex flex-col gap-0.75">
        <span className="text-base font-semibold">{s.project}</span>
        <span className="truncate font-mono text-[11.5px] text-fg-3">{s.cwd}</span>
        <span className="text-xs text-fg-2">{[s.model, s.branch].filter(Boolean).join(" · ") || "-"}</span>
      </div>
      <div className={`flex flex-col gap-1 rounded-md px-3 py-2.5 ${waiting ? "bg-waiting/10" : "bg-bg"}`}>
        <span className={`text-[11.5px] font-semibold ${waiting ? "text-waiting" : "text-fg-3"}`}>{act.label}</span>
        {act.detail && <span className="break-words font-mono text-xs leading-[1.4]">{act.detail}</span>}
      </div>
      <div className="flex items-center gap-3 font-mono text-[11.5px]">
        <span className="text-fg-2">{formatTokens(totalTokens(s.tokens))}</span>
        <span className="text-fg-3">{since}</span>
        {waiting ? (
          <button
            type="button"
            onClick={answerWaiting}
            className="ml-auto cursor-pointer rounded-md bg-waiting px-3 py-1.25 text-[12px] font-semibold text-bg"
          >
            Jawab
          </button>
        ) : (
          <button
            type="button"
            disabled={busy}
            onClick={startNew}
            className={`ml-auto rounded-md border border-border px-3 py-1.25 text-[12px] font-semibold ${
              busy ? "cursor-default text-fg-3" : "cursor-pointer text-fg-2"
            }`}
          >
            Terminal
          </button>
        )}
      </div>
      {note && <span className="text-[11.5px] text-fg-3">{note}</span>}
    </div>
  );
};
