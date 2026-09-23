import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Icon } from "@iconify/react";
import { useEffect, useState } from "react";
import { AgentIcon } from "@/components/BrandIcon";
import { useT } from "@/i18n";
import type { Project, TermProfile } from "@/lib/api";
import { projectTouch, projectsList, termProfiles } from "@/lib/api";
import { useNavigate } from "@/lib/nav";
import { pickFolder } from "@/lib/pick";
import { useLive } from "@/store/live";
import { useTerminal } from "@/store/terminal";

const AGENT_TEXT = { claude: "text-claude", opencode: "text-opencode", codex: "text-codex" } as const;

const isAgent = (id: string): id is "claude" | "opencode" | "codex" => id in AGENT_TEXT;

const agentOf = (profileId: string): "claude" | "opencode" | "codex" =>
  isAgent(profileId) ? profileId : "opencode";

const ProjectIcon = ({ defaultProfile }: { defaultProfile: string | null }) => {
  const agent = agentOf(defaultProfile ?? "");
  return <AgentIcon agent={agent} size={14} className={AGENT_TEXT[agent]} />;
};

const displayCommand = (profile: TermProfile) =>
  `${profile.program}${profile.args.length > 0 ? ` ${profile.args.join(" ")}` : ""}`;

const byRecentUse = (a: Project, b: Project) => (b.lastUsedMs ?? 0) - (a.lastUsedMs ?? 0);

export const NewSessionDialog = ({ onClose }: { onClose: () => void }) => {
  const t = useT();
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
  const [picking, setPicking] = useState(false);
  const [pickError, setPickError] = useState<string | null>(null);

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

  const browse = async () => {
    if (picking) return;
    setPicking(true);
    setPickError(null);
    try {
      const typed = cwd.trim();
      const picked = await pickFolder({
        title: t("newSessionDialog.pickerTitle"),
        defaultPath: typed.startsWith("/") ? typed : undefined,
      });
      if (picked === null) return;
      setCwd(picked);
      setFree(true);
      setProjectId(null);
    } catch (e) {
      setPickError(e instanceof Error ? e.message : String(e));
    } finally {
      setPicking(false);
    }
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
          <span className="text-base font-semibold">{t("newSessionDialog.title")}</span>
          <span className="text-xs text-fg-2">{t("newSessionDialog.description")}</span>
        </div>

        <div className="flex flex-col gap-2">
          <span className="text-xs text-fg-3">{t("newSessionDialog.project")}</span>
          <div className="flex flex-col gap-1.5">
            <button
              type="button"
              onClick={pickFree}
              className={`ad-interactive ad-press flex cursor-pointer items-center gap-2.5 rounded-md border px-3 py-2.5 text-left text-[13px] hover:bg-surface-2 ${
                freeSelected ? "border-busy/50 bg-surface-2 font-semibold" : "border-border"
              }`}
            >
              <span className="size-2 rounded-full bg-idle" />
              <span className="flex-1">{t("newSessionDialog.freeFolder")}</span>
              <span className="font-mono text-[11.5px] text-fg-3">{t("newSessionDialog.typeItYourself")}</span>
            </button>

            {projects.isPending ? (
              <span className="text-xs text-fg-3">{t("newSessionDialog.loadingProjects")}</span>
            ) : saved.length === 0 ? (
              <div className="flex flex-col gap-1.5 rounded-md border border-dashed border-border px-3 py-2.5">
                <span className="text-xs text-fg-3">{t("newSessionDialog.noProjectsSaved")}</span>
                <button
                  type="button"
                  onClick={() => {
                    onClose();
                    nav.project();
                  }}
                  className="ad-interactive ad-press cursor-pointer self-start rounded-md border border-border px-2.5 py-1 text-[11.5px] font-medium text-fg-2 hover:text-fg"
                >
                  {t("newSessionDialog.openProjectPage")}
                </button>
              </div>
            ) : (
              saved.map((p) => {
                const active = !freeSelected && p.id === projectId;
                return (
                  <button
                    key={p.id}
                    type="button"
                    disabled={!p.exists}
                    onClick={() => pickProject(p)}
                    className={`ad-interactive ad-press flex items-center gap-2.5 rounded-md border px-3 py-2.5 text-left text-[13px] hover:bg-surface-2 ${
                      active ? "border-busy/50 bg-surface-2 font-semibold" : "border-border"
                    } ${p.exists ? "cursor-pointer" : "cursor-default opacity-45"}`}
                  >
                    <ProjectIcon defaultProfile={p.defaultProfile} />
                    <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                      <span className="truncate">{p.name}</span>
                      <span className="truncate font-mono text-[11.5px] font-normal text-fg-3">
                        {p.path}
                      </span>
                    </span>
                    {!p.exists && (
                      <span className="shrink-0 text-[11.5px] font-semibold text-err">
                        {t("newSessionDialog.folderMissing")}
                      </span>
                    )}
                  </button>
                );
              })
            )}
          </div>
        </div>

        <div className="flex flex-col gap-2">
          <span className="text-xs text-fg-3">{t("newSessionDialog.agentLabel")}</span>
          <div className="flex flex-col gap-1.5">
            {profiles.map((p) => (
              <button
                key={p.id}
                type="button"
                disabled={!p.available}
                onClick={() => setProfileId(p.id)}
                className={`ad-interactive ad-press flex items-center gap-2.5 rounded-md border px-3 py-2.5 text-left text-[13px] hover:bg-surface-2 ${
                  p.id === profileId ? "border-busy/50 bg-surface-2 font-semibold" : "border-border"
                } ${p.available ? "cursor-pointer" : "cursor-default opacity-45"}`}
              >
                <AgentIcon
                  agent={agentOf(p.id)}
                  size={14}
                  className={isAgent(p.id) ? AGENT_TEXT[p.id] : undefined}
                />
                <span className="flex-1">{p.label}</span>
                <span className="font-mono text-[11.5px] text-fg-3">
                  {p.available ? p.program : t("newSessionDialog.notFoundInPath")}
                </span>
              </button>
            ))}
            {profiles.length === 0 && !error && (
              <span className="text-xs text-fg-3">{t("newSessionDialog.loadingProfiles")}</span>
            )}
          </div>
        </div>

        <div className="flex flex-col gap-2">
          <span className="text-xs text-fg-3">{t("newSessionDialog.workingDirectory")}</span>
          <div className="flex items-center gap-2">
            <input
              value={cwd}
              onChange={(e) => {
                setCwd(e.target.value);
                setFree(true);
                setProjectId(null);
              }}
              list="terminal-cwd-options"
              placeholder={t("newSessionDialog.cwdPlaceholder")}
              className="min-w-0 flex-1 rounded-md border border-border bg-bg px-3 py-2 font-mono text-xs text-fg outline-none focus:border-busy/60"
            />
            <button
              type="button"
              disabled={picking}
              onClick={() => void browse()}
              className="ad-interactive ad-press flex shrink-0 cursor-pointer items-center gap-1.5 rounded-md border border-border px-2.5 py-2 text-[11.5px] font-medium text-fg-2 hover:text-fg disabled:cursor-default disabled:opacity-45"
            >
              <Icon icon="lucide:folder-open" width={13} height={13} />
              {picking ? t("newSessionDialog.opening") : t("newSessionDialog.chooseFolder")}
            </button>
          </div>
          <datalist id="terminal-cwd-options">
            {cwdOptions.map((c) => (
              <option key={c} value={c} />
            ))}
          </datalist>
          {pickError !== null && (
            <span className="font-mono text-[11.5px] text-err">
              {t("newSessionDialog.pickFailed", { error: pickError })}
            </span>
          )}
        </div>

        <div className="flex flex-col gap-2 rounded-md border border-border bg-bg px-3 py-2.5">
          <span className="text-[11.5px] font-semibold text-fg-3">{t("newSessionDialog.commandToRun")}</span>
          <span className="break-all font-mono text-xs leading-[1.4]">
            {selected ? displayCommand(selected) : "-"}
          </span>
          <div className="flex gap-2">
            <span className="w-14 shrink-0 text-[11.5px] text-fg-3">{t("newSessionDialog.folder")}</span>
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
            className="ad-interactive ad-press cursor-pointer rounded-md border border-border px-3.5 py-2 text-[13px] text-fg-2 hover:border-fg-3 hover:text-fg"
          >
            {t("common.cancel")}
          </button>
          <button
            type="button"
            disabled={!canStart}
            onClick={() => void submit()}
            className={`ad-interactive ad-press rounded-md px-3.5 py-2 text-[13px] font-semibold ${
              canStart ? "ad-interactive cursor-pointer bg-busy text-bg hover:opacity-90" : "cursor-default bg-surface-2 text-fg-3"
            }`}
          >
            {busy ? t("newSessionDialog.running") : t("newSessionDialog.run")}
          </button>
        </div>
      </div>
    </div>
  );
};
