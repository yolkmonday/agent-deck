import { Icon } from "@iconify/react";
import { useEffect, useRef, useState } from "react";
import { recoverKill, recoverNudge, recoverRestart, type RecoverAction, type RecoverResult } from "@/lib/api";
import type { Session } from "@/lib/types";
import { useTerminal } from "@/store/terminal";

const NUDGE_HINT = "hanya untuk sesi yang dimulai dari sini";
const RESULT_MS = 6000;

const actionClass = (disabled: boolean) =>
  `rounded-md px-2 py-1.5 text-left text-[12.5px] ${
    disabled ? "cursor-default text-fg-3" : "cursor-pointer text-fg-2 hover:bg-bg"
  }`;

/** The three recovery actions for one unhealthy session. Every destructive one
 *  asks first, per action: there is no "don't ask again" and no bulk kill. */
export const RecoverMenu = ({
  session,
  onOpenTerminal,
}: {
  session: Session;
  onOpenTerminal: (termId: string) => void;
}) => {
  const terminals = useTerminal((st) => st.sessions);
  const [open, setOpen] = useState(false);
  const [confirm, setConfirm] = useState<RecoverAction | null>(null);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; message: string } | null>(null);
  const ref = useRef<HTMLDivElement | null>(null);

  // Only a terminal this dashboard started has a stdin we can type into.
  const owned = terminals.find((t) => t.alive && t.cwd === session.cwd) ?? null;
  const pid = session.pid;

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  useEffect(() => {
    if (!result) return;
    const timer = setTimeout(() => setResult(null), RESULT_MS);
    return () => clearTimeout(timer);
  }, [result]);

  if (session.health === "ok") return null;

  const request = (action: RecoverAction): Promise<RecoverResult> | null => {
    if (action === "nudge") return owned ? recoverNudge(owned.id) : null;
    if (pid === null) return null;
    return action === "kill"
      ? recoverKill(pid, session.cwd)
      : recoverRestart(pid, session.cwd, session.agent);
  };

  const send = async (action: RecoverAction) => {
    const pending = request(action);
    if (pending === null) return;
    setConfirm(null);
    setBusy(true);
    try {
      const r = await pending;
      setResult({ ok: r.ok, message: r.message });
      if (!r.ok) return;
      setOpen(false);
      if (action === "restart" && r.newTermId) {
        // The new session only exists in the backend registry until this lands.
        await useTerminal.getState().refresh();
        onOpenTerminal(r.newTermId);
      }
    } catch (e) {
      setResult({ ok: false, message: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const confirmText =
    confirm === "kill"
      ? `Hentikan proses ${session.project} (pid ${pid})? Pekerjaan yang belum tersimpan bisa hilang.`
      : confirm === "restart"
        ? `Hentikan ${session.project} lalu mulai sesi baru di folder yang sama? Percakapan lama akan hilang.`
        : "";

  const disabled = busy || pid === null;

  return (
    <div ref={ref} className="relative flex shrink-0 flex-col items-end gap-1">
      <button
        type="button"
        onClick={() => {
          setConfirm(null);
          setOpen((v) => !v);
        }}
        aria-expanded={open}
        className={`flex cursor-pointer items-center gap-1 rounded-md border px-2.5 py-1.25 text-[12px] font-semibold ${
          open ? "border-err text-err" : "border-border text-fg-2"
        }`}
      >
        <Icon icon="lucide:life-buoy" width={13} height={13} />
        Tindakan
      </button>
      {open && (
        <div className="absolute top-full right-0 z-30 mt-1 flex w-72 flex-col gap-1.5 rounded-[10px] border border-border bg-surface p-2.5 shadow-lg">
          {confirm === null ? (
            <>
              <button
                type="button"
                disabled={owned === null || busy}
                onClick={() => void send("nudge")}
                className={actionClass(owned === null || busy)}
              >
                Kirim Enter
              </button>
              {owned === null && <span className="px-2 text-[11px] leading-[1.4] text-fg-3">{NUDGE_HINT}</span>}
              <button
                type="button"
                disabled={disabled}
                onClick={() => setConfirm("restart")}
                className={actionClass(disabled)}
              >
                Restart
              </button>
              <button
                type="button"
                disabled={disabled}
                onClick={() => setConfirm("kill")}
                className={`rounded-md px-2 py-1.5 text-left text-[12.5px] ${
                  disabled ? "cursor-default text-fg-3" : "cursor-pointer text-err hover:bg-err/10"
                }`}
              >
                Hentikan
              </button>
            </>
          ) : (
            <>
              <span
                className={`px-2 text-[11.5px] leading-[1.45] ${confirm === "kill" ? "text-err" : "text-fg-2"}`}
              >
                {confirmText}
              </span>
              <div className="flex items-center justify-end gap-2">
                <button
                  type="button"
                  onClick={() => setConfirm(null)}
                  className="cursor-pointer rounded-md border border-border px-2.5 py-1 text-[12px] text-fg-2"
                >
                  Batal
                </button>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => void send(confirm)}
                  className={`cursor-pointer rounded-md px-2.5 py-1 text-[12px] font-semibold text-bg ${
                    confirm === "kill" ? "bg-err" : "bg-waiting"
                  }`}
                >
                  Yakin?
                </button>
              </div>
            </>
          )}
        </div>
      )}
      {result && (
        <span
          className={`max-w-[18rem] text-right text-[11px] leading-[1.4] ${
            result.ok ? "text-ok" : "text-err"
          }`}
        >
          {result.message}
        </span>
      )}
    </div>
  );
};
