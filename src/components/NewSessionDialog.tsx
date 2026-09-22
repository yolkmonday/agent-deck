import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import type { Project, TermProfile } from "@/lib/api";
import { projectTouch, projectsList, termProfiles } from "@/lib/api";
import { useNavigate } from "@/lib/nav";
import { useLive } from "@/store/live";
import { useTerminal } from "@/store/terminal";

const AGENT_DOT = { claude: "bg-claude", opencode: "bg-opencode", codex: "bg-codex" } as const;

const displayCommand = (profile: TermProfile) =>
  `${profile.program}${profile.args.length > 0 ? ` ${profile.args.join(" ")}` : ""}`;

const byRecentUse = (a: Project, b: Project) => (b.lastUsedMs ?? 0) - (a.lastUsedMs ?? 0);

export const NewSessionDialog = ({ onClose }: { onClose: () => void }) => {
  const nav = useNavigate();
  const qc = useQueryClient();
  const sessions = useLive((s) => s.snapshot?.sessions ?? []);
  const start = useTerminal((s) => s.start);
  const projects = useQuery({ queryKey: ["projects"], queryFn: projectsList });
  const [profiles, setProfiles] = useState<TermProfile[]>([]);
  const [profileId, setProfileId] = useState<string | null>(null);
  const [projectId, setProjectId] = useState<string | null>(null);
  const [cwd, setCwd] = useState("");
  const [free, setFree] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void termProfiles()
      .then((list) => {
        setProfiles(list);
        setProfileId(list.find((p) => p.available)?.id ?? list[0]?.id ?? null);
      })
      .catch((e) => setError(String(e)));
  }, []);

  useEffect(() => {
    if (cwd.length === 0) setCwd(sessions[0]?.cwd ?? "");
  }, [sessions, cwd]);

  const saved = [...(projects.data ?? [])].sort(byRecentUse);
  const chosen = saved.find((p) => p.id === projectId) ?? null;

  const freeSelected = free || projectId === null;
  const cwdOptions = [...new Set(sessions.map((s) => s.cwd))];
  const selected = profiles.find((p) => p.id === profileId) ?? null;
  const canStart = selected !== null && selected.available && cwd.trim().length > 0 && !busy;

  const pickFree = () => {
    setFree(true);
    setProjectId(null);
  };

  const pickProject = (p: Project) => {
    setFree(false);
    setProjectId(p.id);
    setCwd(p.path);
    if (p.defaultProfile !== null) setProfileId(p.defaultProfile);
  };

  const submit = async () => {
    if (!canStart || !selected) return;
    setBusy(true);
    setError(null);
    try {
      await start(selected.id, cwd.trim());
      if (chosen !== null) {
        await projectTouch(chosen.id).catch(() => undefined);
        void qc.invalidateQueries({ queryKey: ["projects"] });
      }
      onClose();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-bg/70 px-6">
      <div className="flex max-h-[85vh] w-full max-w-lg flex-col gap-4 overflow-y-auto rounded-[12px] border border-border bg-surface p-5">
        <div className="flex flex-col gap-0.75">
          <span className="text-base font-semibold">Sesi baru</span>
          <span className="text-xs text-fg-2">
            Perintah dijalankan langsung, tanpa shell dan tanpa alias dari terminalmu.
          </span>
        </div>

        <div className="flex flex-col gap-2">
          <span className="text-xs text-fg-3">Project</span>
          <div className="flex flex-col gap-1.5">
            <button
              type="button"
              onClick={pickFree}
              className={`flex items-center gap-2.5 rounded-md border px-3 py-2.5 text-left text-[13px] ${
                freeSelected ? "border-busy/50 bg-surface-2 font-semibold" : "border-border"
              } cursor-pointer`}
            >
              <span className="size-2 rounded-full bg-idle" />
              <span className="flex-1">Folder bebas</span>
              <span className="font-mono text-[11.5px] text-fg-3">ketik sendiri</span>
            </button>

            {projects.isPending ? (
              <span className="text-xs text-fg-3">Memuat project…</span>
            ) : saved.length === 0 ? (
              <div className="flex flex-col gap-1.5 rounded-md border border-dashed border-border px-3 py-2.5">
                <span className="text-xs text-fg-3">Belum ada project tersimpan.</span>
                <button
                  type="button"
                  onClick={() => {
                    onClose();
                    nav.project();
                  }}
                  className="cursor-pointer self-start rounded-md border border-border px-2.5 py-1 text-[11.5px] font-medium text-fg-2 hover:text-fg"
                >
                  Buka halaman Project
                </button>
              </div>
            ) : (
              saved.map((p) => {
                const active = !freeSelected && p.id === projectId;
                const agent = AGENT_DOT[p.defaultProfile as keyof typeof AGENT_DOT] ?? "bg-idle";
                return (
                  <button
                    key={p.id}
                    type="button"
                    disabled={!p.exists}
                    onClick={() => pickProject(p)}
                    className={`flex items-center gap-2.5 rounded-md border px-3 py-2.5 text-left text-[13px] ${
                      active ? "border-busy/50 bg-surface-2 font-semibold" : "border-border"
                    } ${p.exists ? "cursor-pointer" : "cursor-default opacity-45"}`}
                  >
                    <span className={`size-2 rounded-full ${agent}`} />
                    <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                      <span className="truncate">{p.name}</span>
                      <span className="truncate font-mono text-[11.5px] font-normal text-fg-3">
                        {p.path}
                      </span>
                    </span>
                    {!p.exists && (
                      <span className="shrink-0 text-[11.5px] font-semibold text-err">Folder hilang</span>
                    )}
                  </button>
                );
              })
            )}
          </div>
        </div>

        <div className="flex flex-col gap-2">
          <span className="text-xs text-fg-3">Agent</span>
          <div className="flex flex-col gap-1.5">
            {profiles.map((p) => (
              <button
                key={p.id}
                type="button"
                disabled={!p.available}
                onClick={() => setProfileId(p.id)}
                className={`flex items-center gap-2.5 rounded-md border px-3 py-2.5 text-left text-[13px] ${
                  p.id === profileId ? "border-busy/50 bg-surface-2 font-semibold" : "border-border"
                } ${p.available ? "cursor-pointer" : "cursor-default opacity-45"}`}
              >
                <span className={`size-2 rounded-full ${AGENT_DOT[p.id as keyof typeof AGENT_DOT] ?? "bg-idle"}`} />
                <span className="flex-1">{p.label}</span>
                <span className="font-mono text-[11.5px] text-fg-3">
                  {p.available ? p.program : "Tidak ditemukan di PATH"}
                </span>
              </button>
            ))}
            {profiles.length === 0 && !error && <span className="text-xs text-fg-3">Memuat profil…</span>}
          </div>
        </div>

        <div className="flex flex-col gap-2">
          <span className="text-xs text-fg-3">Direktori kerja</span>
          <input
            value={cwd}
            onChange={(e) => {
              setCwd(e.target.value);
              setFree(true);
              setProjectId(null);
            }}
            list="terminal-cwd-options"
            placeholder="/Users/kamu/Dev/proyek"
            className="rounded-md border border-border bg-bg px-3 py-2 font-mono text-xs text-fg outline-none focus:border-busy/60"
          />
          <datalist id="terminal-cwd-options">
            {cwdOptions.map((c) => (
              <option key={c} value={c} />
            ))}
          </datalist>
        </div>

        <div className="flex flex-col gap-2 rounded-md border border-border bg-bg px-3 py-2.5">
          <span className="text-[11.5px] font-semibold text-fg-3">Perintah yang akan dijalankan</span>
          <span className="break-all font-mono text-xs leading-[1.4]">
            {selected ? displayCommand(selected) : "-"}
          </span>
          <div className="flex gap-2">
            <span className="w-14 shrink-0 text-[11.5px] text-fg-3">Folder</span>
            <span className="break-all font-mono text-xs text-fg-2">{cwd.trim() || "-"}</span>
          </div>
        </div>

        {error && (
          <div className="rounded-md border border-err/40 bg-err/10 px-3 py-2 font-mono text-[11.5px] text-err">
            {error}
          </div>
        )}

        <div className="flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="cursor-pointer rounded-md border border-border px-3.5 py-2 text-[13px] text-fg-2"
          >
            Batal
          </button>
          <button
            type="button"
            disabled={!canStart}
            onClick={() => void submit()}
            className={`rounded-md px-3.5 py-2 text-[13px] font-semibold ${
              canStart ? "cursor-pointer bg-busy text-bg" : "cursor-default bg-surface-2 text-fg-3"
            }`}
          >
            {busy ? "Menjalankan…" : "Jalankan"}
          </button>
        </div>
      </div>
    </div>
  );
};
