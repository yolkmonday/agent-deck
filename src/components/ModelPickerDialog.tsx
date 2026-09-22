import { Search, X } from "lucide-react";
import { useState } from "react";
import type { OcModel } from "@/lib/api";
import { filterModels, DEFAULT_LIMITS_NOTE, toggleAll, toggleOne } from "@/lib/models";

interface ModelPickerDialogProps {
  fetched: string[];
  existing: OcModel[];
  onCancel: () => void;
  onAdd: (ids: string[]) => void;
}

export const ModelPickerDialog = ({ fetched, existing, onCancel, onAdd }: ModelPickerDialogProps) => {
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<string[]>([]);

  const known = new Set(existing.map((m) => m.id));
  const visible = fetched.filter((id) => {
    const needle = search.trim().toLowerCase();
    return needle === "" || id.toLowerCase().includes(needle);
  });
  const addable = visible.filter((id) => !known.has(id));
  const allVisibleSelected = addable.length > 0 && addable.every((id) => selected.includes(id));

  const rows = filterModels(
    fetched.map((id) => ({ id, name: null, contextLimit: null, outputLimit: null }) as OcModel),
    search,
  );

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-bg/70 px-6" onClick={onCancel}>
      <div
        className="flex max-h-[80vh] w-full max-w-xl flex-col gap-4 rounded-xl border border-border bg-surface p-5"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start justify-between gap-3">
          <div className="flex flex-col gap-1">
            <span className="text-base font-semibold">Pilih model</span>
            <span className="text-xs text-fg-2">
              {fetched.length} model dikembalikan endpoint.
            </span>
          </div>
          <button
            type="button"
            onClick={onCancel}
            className="ad-interactive ad-press cursor-pointer rounded-md p-1 text-fg-3 hover:bg-surface-2 hover:text-fg"
          >
            <X size={16} />
          </button>
        </div>

        <div className="flex items-center gap-2 rounded-md border border-border bg-bg px-2.5 py-1.75">
          <Search size={14} className="shrink-0 text-fg-3" />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Cari model"
            className="min-w-0 flex-1 bg-transparent font-mono text-[12px] text-fg outline-none"
          />
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto rounded-[10px] border border-border bg-bg px-3 py-2">
          {rows.length === 0 ? (
            <div className="py-10 text-center text-sm text-fg-3">Tidak ada model yang cocok.</div>
          ) : (
            <div className="flex flex-col">
              <label className="ad-interactive flex cursor-pointer items-center gap-2.5 border-b border-border pb-2.5 text-[11.5px] text-fg-3 hover:bg-surface-2">
                <input
                  type="checkbox"
                  checked={allVisibleSelected}
                  disabled={addable.length === 0}
                  onChange={() => setSelected((p) => toggleAll(p, addable))}
                  className="size-3.5 cursor-pointer accent-busy disabled:cursor-default"
                />
                Pilih semua yang terlihat
              </label>
              {rows.map(({ id }) => {
                const already = known.has(id);
                const checked = already || selected.includes(id);
                return (
                  <label
                    key={id}
                    className={`ad-interactive flex items-center gap-2.5 border-b border-border/60 py-2.5 last:border-0 hover:bg-surface-2 ${
                      already ? "cursor-default" : "cursor-pointer"
                    }`}
                  >
                    <input
                      type="checkbox"
                      checked={checked}
                      disabled={already}
                      onChange={() => setSelected((p) => toggleOne(p, id))}
                      className="size-3.5 cursor-pointer accent-busy disabled:cursor-default disabled:opacity-45"
                    />
                    <span className="min-w-0 flex-1 truncate font-mono text-[12px] text-fg">{id}</span>
                    {already && <span className="shrink-0 text-[11px] text-fg-3">sudah ada</span>}
                  </label>
                );
              })}
            </div>
          )}
        </div>

        <div className="flex flex-col gap-2">
          <div className="flex items-center justify-between gap-3">
            <span className="text-xs text-fg-2">
              {selected.length} dipilih dari {fetched.length}
            </span>
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={onCancel}
                className="ad-interactive ad-press cursor-pointer rounded-md border border-border px-3 py-1.75 text-xs text-fg-2 hover:border-fg-3 hover:text-fg"
              >
                Batal
              </button>
              <button
                type="button"
                disabled={selected.length === 0}
                onClick={() => onAdd(selected)}
                className="ad-interactive ad-press cursor-pointer rounded-md bg-busy px-3 py-1.75 text-xs font-semibold text-bg hover:opacity-90 disabled:cursor-default disabled:opacity-45"
              >
                Tambah {selected.length} model
              </button>
            </div>
          </div>
          <span className="text-[11px] text-fg-3">
            Limit diisi otomatis dengan perkiraan dan bisa diubah setelah ditambahkan. {DEFAULT_LIMITS_NOTE}
          </span>
        </div>
      </div>
    </div>
  );
};
