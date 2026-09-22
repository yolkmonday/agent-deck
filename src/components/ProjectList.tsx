import { Pencil, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import type { Project } from "@/lib/api";
import { PALETTE } from "@/components/ProjectForm";
import { formatDuration } from "@/lib/format";

const swatch = (color: string | null) =>
  PALETTE.find((p) => p.value.toUpperCase() === color?.toUpperCase())?.token ?? null;

const lastUsed = (ms: number | null) =>
  ms === null ? "belum pernah" : `${formatDuration(Date.now() - ms)} lalu`;

export const ProjectList = ({
  projects,
  editingId,
  busy,
  onEdit,
  onDelete,
  onMove,
}: {
  projects: Project[];
  editingId: string | null;
  busy: boolean;
  onEdit: (project: Project) => void;
  onDelete: (id: string) => void;
  onMove: (project: Project, direction: -1 | 1) => void;
}) => {
  const [pendingDelete, setPendingDelete] = useState<string | null>(null);

  useEffect(() => {
    if (pendingDelete !== null && !projects.some((p) => p.id === pendingDelete)) setPendingDelete(null);
  }, [projects, pendingDelete]);

  if (projects.length === 0) {
    return (
      <div className="rounded-[10px] border border-dashed border-border p-8 text-center text-sm text-fg-3">
        Belum ada project. Tambahkan satu, atau pakai saran di bawah.
      </div>
    );
  }

  return (
    <div className="flex flex-col">
      {projects.map((p, i) => (
        <div
          key={p.id}
          className={`flex items-center gap-3 border-b border-border/60 px-3 py-3 last:border-0 ${
            p.id === editingId ? "bg-surface-2" : ""
          }`}
        >
          <span className={`size-2.5 shrink-0 rounded-full ${swatch(p.color) ?? "bg-idle"}`} />

          <div className="flex min-w-0 flex-1 flex-col gap-1">
            <div className="flex items-center gap-2">
              <span className="truncate text-[13px] font-medium text-fg">{p.name}</span>
              {!p.exists && (
                <span className="shrink-0 rounded border border-err/40 bg-err/10 px-1.5 py-0.5 text-[10.5px] font-semibold text-err">
                  Folder hilang
                </span>
              )}
            </div>
            <span className="truncate font-mono text-[11.5px] text-fg-3">{p.path}</span>
            <span className="text-[11.5px] text-fg-3">
              {p.defaultProfile ?? "tanpa profil default"} · {lastUsed(p.lastUsedMs)}
            </span>
          </div>

          <div className="flex shrink-0 items-center gap-1.5">
            <button
              type="button"
              disabled={i === 0 || busy}
              onClick={() => onMove(p, -1)}
              className="ad-interactive ad-press cursor-pointer rounded-md border border-border px-2 py-1 text-[11.5px] text-fg-2 hover:border-fg-3 hover:text-fg disabled:cursor-default disabled:opacity-35"
            >
              Naik
            </button>
            <button
              type="button"
              disabled={i === projects.length - 1 || busy}
              onClick={() => onMove(p, 1)}
              className="ad-interactive ad-press cursor-pointer rounded-md border border-border px-2 py-1 text-[11.5px] text-fg-2 hover:border-fg-3 hover:text-fg disabled:cursor-default disabled:opacity-35"
            >
              Turun
            </button>
            <button
              type="button"
              onClick={() => onEdit(p)}
              aria-label="Ubah"
              className="ad-interactive ad-press cursor-pointer rounded-md p-1.5 text-fg-3 hover:bg-surface-2 hover:text-fg"
            >
              <Pencil size={14} />
            </button>
            {pendingDelete === p.id ? (
              <div className="flex items-center gap-1.5">
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => {
                    onDelete(p.id);
                    setPendingDelete(null);
                  }}
                  className="ad-interactive ad-press cursor-pointer rounded-md bg-err px-2.5 py-1 text-[11.5px] font-semibold text-bg hover:opacity-90 disabled:cursor-default disabled:opacity-45"
                >
                  Yakin?
                </button>
                <button
                  type="button"
                  onClick={() => setPendingDelete(null)}
                  className="ad-interactive ad-press cursor-pointer rounded-md border border-border px-2.5 py-1 text-[11.5px] text-fg-2 hover:border-fg-3 hover:text-fg"
                >
                  Batal
                </button>
              </div>
            ) : (
              <button
                type="button"
                onClick={() => setPendingDelete(p.id)}
                aria-label="Hapus"
                className="ad-interactive ad-press cursor-pointer rounded-md p-1.5 text-fg-3 hover:bg-err/10 hover:text-err"
              >
                <Trash2 size={14} />
              </button>
            )}
          </div>
        </div>
      ))}
      <span className="pt-2 text-[11.5px] text-fg-3">Folder aslinya tidak ikut terhapus.</span>
    </div>
  );
};
