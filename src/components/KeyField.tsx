import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Eye, EyeOff, KeyRound, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { secretClear, secretReveal, secretSet } from "@/lib/api";

const REVEAL_MS = 15_000;

interface KeyFieldProps {
  providerId: string;
  keyMasked: string | null;
  keyInline: boolean;
}

export const KeyField = ({ providerId, keyMasked, keyInline }: KeyFieldProps) => {
  const qc = useQueryClient();
  const [revealed, setRevealed] = useState<string | null>(null);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const remask = () => {
    setRevealed(null);
    if (timer.current !== null) {
      clearTimeout(timer.current);
      timer.current = null;
    }
  };

  useEffect(() => remask, [providerId]);

  const reveal = useMutation({
    mutationFn: () => secretReveal(providerId),
    onSuccess: (key) => {
      setError(null);
      setRevealed(key);
      if (timer.current !== null) clearTimeout(timer.current);
      timer.current = setTimeout(() => setRevealed(null), REVEAL_MS);
    },
    onError: (e) => setError(String(e)),
  });

  const save = useMutation({
    mutationFn: (key: string) => secretSet(providerId, key),
    onSuccess: () => {
      setError(null);
      setEditing(false);
      setDraft("");
      void qc.invalidateQueries({ queryKey: ["models-overview"] });
    },
    onError: (e) => setError(String(e)),
  });

  const clear = useMutation({
    mutationFn: () => secretClear(providerId),
    onSuccess: () => {
      setError(null);
      remask();
      void qc.invalidateQueries({ queryKey: ["models-overview"] });
    },
    onError: (e) => setError(String(e)),
  });

  const shown = revealed ?? keyMasked ?? "belum ada key";

  return (
    <div className="flex flex-col gap-2">
      <span className="flex items-center gap-1.5 text-xs text-fg-3">
        <KeyRound size={13} />
        API key
      </span>

      <div className="flex items-center gap-2">
        <input
          readOnly={!editing}
          value={editing ? draft : shown}
          onBlur={remask}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && editing && draft.trim() !== "") save.mutate(draft.trim());
            if (e.key === "Escape") {
              setEditing(false);
              setDraft("");
              remask();
            }
          }}
          placeholder={editing ? "Tempel key baru" : undefined}
          className={`min-w-0 flex-1 rounded-md border bg-bg px-3 py-2 font-mono text-xs text-fg outline-none ${
            editing ? "border-busy" : "border-border"
          }`}
        />

        {!editing && keyMasked !== null && (
          <button
            type="button"
            title="Tampilkan 15 detik"
            onClick={() => (revealed === null ? reveal.mutate() : remask())}
            className="ad-interactive ad-press cursor-pointer rounded-md border border-border p-2 text-fg-3 hover:bg-surface-2 hover:text-fg"
          >
            {revealed === null ? <Eye size={15} /> : <EyeOff size={15} />}
          </button>
        )}

        {editing ? (
          <>
            <button
              type="button"
              disabled={draft.trim() === "" || save.isPending}
              onClick={() => save.mutate(draft.trim())}
              className="ad-interactive ad-press cursor-pointer rounded-md bg-busy px-3 py-2 text-xs font-semibold text-bg hover:opacity-90 disabled:cursor-default disabled:opacity-45"
            >
              {save.isPending ? "Menyimpan…" : "Simpan"}
            </button>
            <button
              type="button"
              onClick={() => {
                setEditing(false);
                setDraft("");
              }}
              className="ad-interactive ad-press cursor-pointer rounded-md border border-border px-3 py-2 text-xs text-fg-2 hover:border-fg-3 hover:text-fg"
            >
              Batal
            </button>
          </>
        ) : (
          <>
            <button
              type="button"
              onClick={() => {
                remask();
                setEditing(true);
              }}
              className="ad-interactive ad-press cursor-pointer rounded-md border border-border px-3 py-2 text-xs text-fg-2 hover:border-fg-3 hover:text-fg"
            >
              {keyMasked === null ? "Isi key" : "Ganti"}
            </button>
            {keyMasked !== null && (
              <button
                type="button"
                title="Hapus key"
                disabled={clear.isPending}
                onClick={() => clear.mutate()}
                className="ad-interactive ad-press cursor-pointer rounded-md border border-border p-2 text-fg-3 hover:bg-err/10 hover:text-err disabled:cursor-default disabled:opacity-45"
              >
                <Trash2 size={15} />
              </button>
            )}
          </>
        )}
      </div>

      {keyInline && (
        <span className="text-[11.5px] text-waiting">
          Key ini masih plaintext di config. Pakai tombol di halaman sebelah untuk memindahkannya.
        </span>
      )}

      {revealed !== null && (
        <span className="text-[11.5px] text-fg-3">Key disembunyikan lagi dalam 15 detik.</span>
      )}

      {error !== null && (
        <div className="rounded-md border border-err/40 bg-err/10 px-3 py-2 font-mono text-[11.5px] text-err">
          {error}
        </div>
      )}
    </div>
  );
};
