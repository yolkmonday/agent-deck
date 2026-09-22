import { Plus } from "lucide-react";
import { useState } from "react";
import { NewSessionDialog } from "@/components/NewSessionDialog";
import { TerminalTabs } from "@/components/TerminalTabs";
import { TerminalView } from "@/components/TerminalView";
import { stopSession, useTerminal } from "@/store/terminal";

const ConfirmDialog = ({ label, onCancel, onConfirm }: { label: string; onCancel: () => void; onConfirm: () => void }) => (
  <div className="fixed inset-0 z-50 flex items-center justify-center bg-bg/70 px-6">
    <div className="flex w-full max-w-sm flex-col gap-4 rounded-[12px] border border-border bg-surface p-5">
      <span className="text-base font-semibold">Hentikan sesi ini?</span>
      <span className="font-mono text-xs text-fg-2">{label}</span>
      <div className="flex justify-end gap-2">
        <button
          type="button"
          onClick={onCancel}
          className="cursor-pointer rounded-md border border-border px-3.5 py-2 text-[13px] text-fg-2"
        >
          Batal
        </button>
        <button
          type="button"
          onClick={onConfirm}
          className="cursor-pointer rounded-md bg-err px-3.5 py-2 text-[13px] font-semibold text-bg"
        >
          Hentikan
        </button>
      </div>
    </div>
  </div>
);

export const TerminalPage = ({ initialSessionId }: { initialSessionId?: string | null }) => {
  const sessions = useTerminal((s) => s.sessions);
  const activeId = useTerminal((s) => s.activeId);
  const error = useTerminal((s) => s.error);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [pendingKill, setPendingKill] = useState<string | null>(null);

  const target = initialSessionId && sessions.some((s) => s.id === initialSessionId) ? initialSessionId : activeId;
  const active = sessions.find((s) => s.id === target) ?? null;
  const killTarget = sessions.find((s) => s.id === pendingKill) ?? null;

  const confirmKill = () => {
    if (!pendingKill) return;
    void stopSession(pendingKill).catch(() => undefined);
    setPendingKill(null);
  };

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <header className="flex items-center justify-between px-7 py-4.5">
        <div className="flex flex-col gap-0.75">
          <h1 className="text-[22px] font-semibold">Terminal</h1>
          <span className="text-[12.5px] text-fg-2">Sesi di sini mati kalau app ditutup.</span>
        </div>
        <button
          type="button"
          onClick={() => setDialogOpen(true)}
          className="flex cursor-pointer items-center gap-1.5 rounded-md bg-busy px-3.5 py-2 text-[13px] font-semibold text-bg"
        >
          <Plus size={15} />
          Sesi baru
        </button>
      </header>

      {sessions.length > 0 && <TerminalTabs onClose={setPendingKill} />}

      {error && (
        <div className="mx-7 mb-3 rounded-md border border-err/40 bg-err/10 px-3 py-2 font-mono text-xs text-err">
          {error}
        </div>
      )}

      {active === null ? (
        <div className="flex flex-1 items-center justify-center px-7 pb-7">
          <div className="rounded-[10px] border border-dashed border-border p-8 text-center text-sm text-fg-3">
            Belum ada sesi. Klik "Sesi baru" untuk mulai.
          </div>
        </div>
      ) : (
        <div className="flex min-h-0 flex-1 flex-col gap-0 pb-5">
          <div className="flex items-center gap-3 px-7 py-2">
            <span className="truncate font-mono text-[11.5px] text-fg-3">{active.command}</span>
            <span className="truncate font-mono text-[11.5px] text-fg-3">{active.cwd}</span>
            {!active.alive && <span className="text-[11.5px] font-semibold text-idle">sudah berakhir</span>}
          </div>
          <div className="min-h-0 flex-1 px-5">
            {sessions.map((s) => (
              <div key={s.id} className={s.id === active.id ? "h-full" : "hidden"}>
                <TerminalView session={s} />
              </div>
            ))}
          </div>
        </div>
      )}

      {dialogOpen && <NewSessionDialog onClose={() => setDialogOpen(false)} />}
      {killTarget && (
        <ConfirmDialog label={killTarget.command} onCancel={() => setPendingKill(null)} onConfirm={confirmKill} />
      )}
    </div>
  );
};
