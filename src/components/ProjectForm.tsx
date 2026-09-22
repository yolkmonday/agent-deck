import { useEffect, useState } from "react";
import type { Project, ProjectInput, TermProfile } from "@/lib/api";

export const PALETTE = [
  { value: "#4C9AFF", token: "bg-busy" },
  { value: "#F5A524", token: "bg-waiting" },
  { value: "#3DD68C", token: "bg-ok" },
  { value: "#F0616D", token: "bg-err" },
  { value: "#E8825C", token: "bg-claude" },
  { value: "#34D0E0", token: "bg-opencode" },
] as const;

const field =
  "rounded-md border border-border bg-bg px-3 py-2 text-[13px] text-fg outline-none focus:border-busy";

const blank = (sortOrder: number): ProjectInput => ({
  name: "",
  path: "",
  defaultProfile: null,
  color: null,
  sortOrder,
});

export const ProjectForm = ({
  editing,
  nextSortOrder,
  profiles,
  busy,
  error,
  onSubmit,
  onCancel,
}: {
  editing: Project | null;
  nextSortOrder: number;
  profiles: TermProfile[];
  busy: boolean;
  error: string | null;
  onSubmit: (input: ProjectInput) => void;
  onCancel: () => void;
}) => {
  const [draft, setDraft] = useState<ProjectInput>(blank(nextSortOrder));

  useEffect(() => {
    setDraft(
      editing === null
        ? blank(nextSortOrder)
        : {
            name: editing.name,
            path: editing.path,
            defaultProfile: editing.defaultProfile,
            color: editing.color,
            sortOrder: editing.sortOrder,
          },
    );
  }, [editing, nextSortOrder]);

  const patch = (next: Partial<ProjectInput>) => setDraft((p) => ({ ...p, ...next }));

  const nameInvalid = error === "nama tidak boleh kosong" || error === "nama terlalu panjang";
  const pathInvalid = error === "path harus absolut" || error === "folder tidak ditemukan" || error === "project dengan folder ini sudah ada";
  const profileInvalid = error === "profil tidak dikenal";

  const canSave = draft.name.trim() !== "" && draft.path.trim() !== "" && !busy;

  return (
    <section className="flex w-90 shrink-0 flex-col gap-4">
      <div className="flex items-center justify-between">
        <span className="text-sm font-semibold">{editing === null ? "Project baru" : "Ubah project"}</span>
        {editing !== null && (
          <button
            type="button"
            onClick={onCancel}
            className="cursor-pointer rounded-md px-2 py-1 text-xs text-fg-3 hover:text-fg"
          >
            Batal
          </button>
        )}
      </div>

      <label className="flex flex-col gap-1.5">
        <span className="text-xs text-fg-3">Nama</span>
        <input
          value={draft.name}
          onChange={(e) => patch({ name: e.target.value })}
          placeholder="kirimi"
          className={`${field} ${nameInvalid ? "border-err" : ""}`}
        />
        {nameInvalid && <span className="text-[11.5px] text-err">{error}</span>}
      </label>

      <label className="flex flex-col gap-1.5">
        <span className="text-xs text-fg-3">Folder</span>
        <input
          value={draft.path}
          onChange={(e) => patch({ path: e.target.value })}
          placeholder="~/Dev/kirimi"
          className={`${field} font-mono ${pathInvalid ? "border-err" : ""}`}
        />
        <span className="text-[11.5px] text-fg-3">Boleh pakai ~. Folder harus ada saat disimpan.</span>
        {pathInvalid && <span className="text-[11.5px] text-err">{error}</span>}
      </label>

      <label className="flex flex-col gap-1.5">
        <span className="text-xs text-fg-3">Profil default</span>
        <select
          value={draft.defaultProfile ?? ""}
          onChange={(e) => patch({ defaultProfile: e.target.value === "" ? null : e.target.value })}
          className={`${field} cursor-pointer`}
        >
          <option value="">Tidak ada</option>
          {profiles.map((p) => (
            <option key={p.id} value={p.id}>
              {p.label}
            </option>
          ))}
        </select>
        <span className="text-[11.5px] text-fg-3">Dipakai saat mulai sesi dari project ini.</span>
        {profileInvalid && <span className="text-[11.5px] text-err">{error}</span>}
      </label>

      <div className="flex flex-col gap-1.5">
        <span className="text-xs text-fg-3">Warna</span>
        <div className="flex flex-wrap items-center gap-2">
          {PALETTE.map((p) => (
            <button
              key={p.value}
              type="button"
              aria-label={p.value}
              onClick={() => patch({ color: draft.color === p.value ? null : p.value })}
              className={`size-6 cursor-pointer rounded-full ${p.token} ${
                draft.color === p.value ? "ring-2 ring-fg ring-offset-2 ring-offset-surface" : ""
              }`}
            />
          ))}
          <button
            type="button"
            onClick={() => patch({ color: null })}
            className={`cursor-pointer rounded-md border px-2.5 py-1 text-[11.5px] ${
              draft.color === null ? "border-fg text-fg" : "border-border text-fg-3"
            }`}
          >
            tanpa warna
          </button>
        </div>
      </div>

      <div className="flex items-center gap-2 border-t border-border pt-4">
        <button
          type="button"
          disabled={!canSave}
          onClick={() => onSubmit({ ...draft, name: draft.name.trim(), path: draft.path.trim() })}
          className="cursor-pointer rounded-md bg-busy px-4 py-1.75 text-xs font-semibold text-bg disabled:cursor-default disabled:opacity-45"
        >
          {busy ? "Menyimpan…" : editing === null ? "Tambah" : "Simpan"}
        </button>
        <span className="text-[11.5px] text-fg-3">Perubahan hanya di Agent Deck, bukan di disk.</span>
      </div>
    </section>
  );
};
