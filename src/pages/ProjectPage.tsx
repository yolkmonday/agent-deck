import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus } from "lucide-react";
import { useEffect, useState } from "react";
import { ProjectForm } from "@/components/ProjectForm";
import { ProjectList } from "@/components/ProjectList";
import { useT } from "@/i18n";
import type { Project, ProjectInput } from "@/lib/api";
import {
  projectCreate,
  projectDelete,
  projectSuggestions,
  projectUpdate,
  projectsList,
  termProfiles,
} from "@/lib/api";

const PROJECTS = ["projects"] as const;

export const ProjectPage = () => {
  const t = useT();
  const qc = useQueryClient();
  const projects = useQuery({ queryKey: PROJECTS, queryFn: projectsList });
  const profiles = useQuery({ queryKey: ["term-profiles"], queryFn: termProfiles });
  const suggestions = useQuery({ queryKey: ["project-suggestions"], queryFn: projectSuggestions });

  const [editing, setEditing] = useState<Project | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setError(null);
  }, [editing]);

  const refresh = async () => {
    await qc.invalidateQueries({ queryKey: PROJECTS });
    await qc.invalidateQueries({ queryKey: ["project-suggestions"] });
  };

  const create = useMutation({
    mutationFn: (input: ProjectInput) => projectCreate(input),
    onSuccess: async () => {
      setEditing(null);
      setError(null);
      await refresh();
    },
    onError: (e) => setError(String(e)),
  });

  const update = useMutation({
    mutationFn: ({ id, input }: { id: string; input: ProjectInput }) => projectUpdate(id, input),
    onSuccess: async () => {
      setEditing(null);
      setError(null);
      await refresh();
    },
    onError: (e) => setError(String(e)),
  });

  const remove = useMutation({
    mutationFn: (id: string) => projectDelete(id),
    onSuccess: async () => {
      setError(null);
      await refresh();
    },
    onError: (e) => setError(String(e)),
  });

  const list = projects.data ?? [];
  const busy = create.isPending || update.isPending || remove.isPending;
  const nextSortOrder = list.reduce((max, p) => Math.max(max, p.sortOrder), 0) + 1;

  // A prefilled suggestion has an empty id: it is a draft, not an existing row.
  const isEdit = editing !== null && editing.id !== "";

  const submit = (input: ProjectInput) => {
    if (!isEdit) create.mutate(input);
    else update.mutate({ id: editing.id, input });
  };

  const move = (project: Project, direction: -1 | 1) => {
    const index = list.findIndex((p) => p.id === project.id);
    const neighbour = list[index + direction];
    if (neighbour === undefined) return;
    void Promise.all([
      projectUpdate(project.id, {
        name: project.name,
        path: project.path,
        defaultProfile: project.defaultProfile,
        color: project.color,
        sortOrder: neighbour.sortOrder,
      }),
      projectUpdate(neighbour.id, {
        name: neighbour.name,
        path: neighbour.path,
        defaultProfile: neighbour.defaultProfile,
        color: neighbour.color,
        sortOrder: project.sortOrder,
      }),
    ])
      .then(() => refresh())
      .catch((e: unknown) => setError(String(e)));
  };

  const prefill = (name: string, path: string) => {
    setEditing({
      id: "",
      name,
      path,
      defaultProfile: null,
      color: null,
      sortOrder: nextSortOrder,
      lastUsedMs: null,
      exists: true,
    });
    setError(null);
  };

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <header className="flex items-center justify-between px-7 py-4.5">
        <div className="flex flex-col gap-0.75">
          <h1 className="text-[22px] font-semibold">{t("projectPage.title")}</h1>
          <span className="text-[12.5px] text-fg-2">{t("projectPage.subtitle")}</span>
        </div>
        <button
          type="button"
          onClick={() => setEditing(null)}
          className="ad-interactive ad-press flex cursor-pointer items-center gap-1.5 rounded-md border border-border px-3.5 py-2 text-[13px] font-medium text-fg-2 hover:text-fg"
        >
          <Plus size={15} />
          {t("projectPage.newProject")}
        </button>
      </header>

      <main className="flex min-h-0 flex-1 flex-col gap-4 overflow-hidden px-7 pb-7">
        {error !== null && (
          <div className="rounded-md border border-err/40 bg-err/10 px-3 py-2 font-mono text-xs text-err">
            {error}
          </div>
        )}

        <div className="flex min-h-0 flex-1 gap-5">
          <ProjectForm
            editing={editing}
            nextSortOrder={nextSortOrder}
            profiles={profiles.data ?? []}
            busy={busy}
            error={error}
            onSubmit={submit}
            onCancel={() => setEditing(null)}
          />

          <section className="flex min-h-0 min-w-0 flex-1 flex-col gap-3 overflow-y-auto">
            <div className="flex items-center justify-between">
              <span className="text-sm font-semibold">{t("projectPage.listTitle")}</span>
              <span className="text-xs text-fg-3">
                {t(list.length === 1 ? "projectPage.count.one" : "projectPage.count.other", { n: list.length })}
              </span>
            </div>

            {projects.isError && (
              <div className="rounded-md border border-err/40 bg-err/10 px-3 py-2 font-mono text-xs text-err">
                {String(projects.error)}
              </div>
            )}

            {projects.isPending ? (
              <span className="text-xs text-fg-3">{t("projectPage.loading")}</span>
            ) : (
              <div className="rounded-[10px] border border-border bg-surface px-2 py-1">
                <ProjectList
                  projects={list}
                  editingId={editing?.id ?? null}
                  busy={busy}
                  onEdit={(p) => setEditing(p)}
                  onDelete={(id) => remove.mutate(id)}
                  onMove={move}
                />
              </div>
            )}

            <div className="flex flex-col gap-3 border-t border-border pt-4">
              <span className="text-sm font-semibold">{t("projectPage.suggestionsTitle")}</span>
              {suggestions.isError && (
                <div className="rounded-md border border-err/40 bg-err/10 px-3 py-2 font-mono text-xs text-err">
                  {String(suggestions.error)}
                </div>
              )}
              {suggestions.isPending ? (
                <span className="text-xs text-fg-3">{t("projectPage.loadingSuggestions")}</span>
              ) : (suggestions.data ?? []).length === 0 ? (
                <span className="text-xs text-fg-3">{t("projectPage.noSuggestions")}</span>
              ) : (
                <div className="flex flex-col">
                  {(suggestions.data ?? []).map((s) => (
                    <div
                      key={`${s.source}:${s.path}`}
                      className="flex items-center gap-3 border-b border-border/60 py-2.5 last:border-0"
                    >
                      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                        <span className="truncate text-[12.5px] font-medium text-fg">{s.name}</span>
                        <span className="truncate font-mono text-[11.5px] text-fg-3">{s.path}</span>
                      </div>
                      <span className="shrink-0 text-[11.5px] text-fg-3">
                        {s.source === "live"
                          ? t("projectPage.suggestionLive")
                          : t(
                              s.messages === 1
                                ? "projectPage.suggestionMessages.one"
                                : "projectPage.suggestionMessages.other",
                              { n: s.messages },
                            )}
                      </span>
                      <button
                        type="button"
                        onClick={() => prefill(s.name, s.path)}
                        className="ad-interactive ad-press shrink-0 cursor-pointer rounded-md border border-border px-2.5 py-1 text-[11.5px] font-medium text-fg-2 hover:text-fg"
                      >
                        {t("common.add")}
                      </button>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </section>
        </div>
      </main>
    </div>
  );
};
