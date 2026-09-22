import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, Check, History, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { KeyField } from "@/components/KeyField";
import { ModelList } from "@/components/ModelList";
import { ProviderIcon } from "@/components/BrandIcon";
import type { HeaderStyle, OcModel, OcProviderInput } from "@/lib/api";
import { configBackups, configRestore, modelsOverview, secretMigrateInline } from "@/lib/api";
import { providerDelete, providerSave } from "@/lib/api";
import { useNavigate } from "@/lib/nav";

const field =
  "rounded-md border border-border bg-bg px-3 py-2 text-[13px] text-fg outline-none focus:border-busy";

const ADAPTER = "@ai-sdk/openai-compatible";

const SubHeader = ({ text }: { text: string }) => (
  <span className="text-[11.5px] text-fg-3">{text}</span>
);

const BackupMenu = ({ onClose }: { onClose: () => void }) => {
  const qc = useQueryClient();
  const query = useQuery({ queryKey: ["config-backups"], queryFn: configBackups });
  const [pending, setPending] = useState<string | null>(null);

  const restore = useMutation({
    mutationFn: (path: string) => configRestore(path),
    onSuccess: () => {
      void qc.invalidateQueries();
      onClose();
    },
  });

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-bg/70 px-6" onClick={onClose}>
      <div
        className="flex max-h-[80vh] w-full max-w-xl flex-col gap-4 rounded-xl border border-border bg-surface p-5"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex flex-col gap-1">
          <span className="text-base font-semibold">Pulihkan config</span>
          <span className="text-xs text-fg-2">
            Setiap penyimpanan membuat backup. Memulihkan akan menimpa config saat ini.
          </span>
        </div>

        {query.isError && (
          <div className="rounded-md border border-err/40 bg-err/10 px-3 py-2 font-mono text-xs text-err">
            {String(query.error)}
          </div>
        )}

        <div className="min-h-0 flex-1 overflow-y-auto">
          {query.isPending ? (
            <span className="text-xs text-fg-3">Memuat backup…</span>
          ) : (query.data ?? []).length === 0 ? (
            <span className="text-xs text-fg-3">Belum ada backup.</span>
          ) : (
            <div className="flex flex-col">
              {(query.data ?? []).map((b) => (
                <div
                  key={b.path}
                  className="flex items-center gap-3 border-b border-border/60 py-2.5 last:border-0"
                >
                  <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                    <span className="truncate font-mono text-[11.5px] text-fg">{b.path}</span>
                    <span className="text-[11px] text-fg-3">{new Date(b.atMs).toLocaleString("id-ID")}</span>
                  </div>
                  {pending === b.path ? (
                    <div className="flex items-center gap-2">
                      <button
                        type="button"
                        onClick={() => restore.mutate(b.path)}
                        disabled={restore.isPending}
                        className="cursor-pointer rounded-md bg-err px-3 py-1.5 text-[11.5px] font-semibold text-bg disabled:cursor-default disabled:opacity-45"
                      >
                        {restore.isPending ? "Memulihkan…" : "Yakin?"}
                      </button>
                      <button
                        type="button"
                        onClick={() => setPending(null)}
                        className="cursor-pointer rounded-md border border-border px-3 py-1.5 text-[11.5px] text-fg-2"
                      >
                        Batal
                      </button>
                    </div>
                  ) : (
                    <button
                      type="button"
                      onClick={() => setPending(b.path)}
                      className="cursor-pointer rounded-md border border-border px-3 py-1.5 text-[11.5px] text-fg-2 hover:text-fg"
                    >
                      Pulihkan
                    </button>
                  )}
                </div>
              ))}
            </div>
          )}
          {restore.isError && (
            <div className="mt-2 rounded-md border border-err/40 bg-err/10 px-3 py-2 font-mono text-xs text-err">
              {String(restore.error)}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

export const ProviderEditPage = ({ providerId }: { providerId: string | null }) => {
  const qc = useQueryClient();
  const nav = useNavigate();
  const overview = useQuery({ queryKey: ["models-overview"], queryFn: modelsOverview });

  const existing = overview.data?.opencode.find((p) => p.id === providerId) ?? null;
  const creating = providerId === null;

  const [draft, setDraft] = useState<OcProviderInput | null>(null);
  const [backups, setBackups] = useState(false);
  const [savedBackup, setSavedBackup] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Refill the form from the loaded provider whenever the target changes.
  useEffect(() => {
    setDraft(null);
  }, [providerId]);

  useEffect(() => {
    if (!creating && existing === null) return;
    if (draft !== null) return;
    setDraft(
      existing === null
        ? {
            id: "",
            name: "",
            npm: ADAPTER,
            baseUrl: "",
            headerStyle: "bearer",
            customHeaderName: null,
            enabled: true,
            models: [],
          }
        : {
            id: existing.id,
            name: existing.name,
            npm: existing.npm,
            baseUrl: existing.baseUrl,
            headerStyle: existing.headerStyle,
            customHeaderName: existing.customHeaderName,
            enabled: existing.enabled,
            models: existing.models.map((m) => ({ ...m })),
          },
    );
  }, [existing, creating, draft]);

  const save = useMutation({
    mutationFn: (input: OcProviderInput) => providerSave(input),
    onSuccess: async (provider) => {
      setError(null);
      const list = await configBackups().catch(() => []);
      setSavedBackup(list[0]?.path ?? null);
      void qc.invalidateQueries({ queryKey: ["models-overview"] });
      void qc.invalidateQueries({ queryKey: ["config-backups"] });
      if (creating) nav.provider(provider.id);
    },
    onError: (e) => setError(String(e)),
  });

  const remove = useMutation({
    mutationFn: (id: string) => providerDelete(id),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["models-overview"] });
      nav.provider("");
    },
    onError: (e) => setError(String(e)),
  });

  const migrate = useMutation({
    mutationFn: (id: string) => secretMigrateInline(id),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["models-overview"] });
      void qc.invalidateQueries({ queryKey: ["config-backups"] });
    },
    onError: (e) => setError(String(e)),
  });

  const back = (
    <button
      type="button"
      onClick={() => nav.provider("")}
      className="flex cursor-pointer items-center gap-1.5 rounded-md border border-border px-3 py-1.75 text-xs font-medium text-fg-2 hover:text-fg"
    >
      <ArrowLeft size={14} />
      Kembali
    </button>
  );

  if (overview.isPending || draft === null) {
    return (
      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex items-center justify-between px-7 py-4.5">
          <h1 className="text-[22px] font-semibold">Kelola Provider</h1>
          {back}
        </header>
        <main className="flex flex-1 items-center justify-center text-sm text-fg-3">
          {overview.isError ? String(overview.error) : "Memuat provider…"}
        </main>
      </div>
    );
  }

  const idValid = /^[A-Za-z0-9_-]{1,64}$/.test(draft.id);
  const limitsIncomplete = draft.models.some((m) => m.contextLimit === null || m.outputLimit === null);
  const canSave = draft.name.trim() !== "" && idValid && draft.baseUrl.trim() !== "" && !limitsIncomplete;

  const patch = (next: Partial<OcProviderInput>) => setDraft((p) => (p === null ? p : { ...p, ...next }));

  const setModels = (models: OcModel[]) => patch({ models });

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <header className="flex items-center justify-between px-7 py-4.5">
        <div className="flex items-center gap-3">
          <ProviderIcon providerId={creating ? draft.id : (providerId ?? draft.id)} size={28} />
          <div className="flex flex-col gap-0.75">
            <h1 className="text-[22px] font-semibold">{creating ? "Provider baru" : draft.name || draft.id}</h1>
            <span className="text-[12.5px] text-fg-2">
              Ditulis ke config global opencode lewat file sementara dan backup.
            </span>
          </div>
        </div>
        <div className="flex items-center gap-2">
          {!creating && (
            <button
              type="button"
              onClick={() => setBackups(true)}
              className="flex cursor-pointer items-center gap-1.5 rounded-md border border-border px-3 py-1.75 text-xs font-medium text-fg-2 hover:text-fg"
            >
              <History size={14} />
              Pulihkan
            </button>
          )}
          {back}
        </div>
      </header>

      <main className="flex min-h-0 flex-1 flex-col gap-4 overflow-hidden px-7 pb-7">
        {error !== null && (
          <div className="rounded-md border border-err/40 bg-err/10 px-3 py-2 font-mono text-xs text-err">
            {error}
          </div>
        )}

        {savedBackup !== null && (
          <div className="flex flex-col gap-1 rounded-md border border-ok/40 bg-ok/10 px-3 py-2.5">
            <span className="flex items-center gap-1.5 text-[12.5px] font-medium text-ok">
              <Check size={14} />
              Tersimpan. Backup dibuat:
            </span>
            <span className="break-all font-mono text-[11.5px] text-fg-2">{savedBackup}</span>
            <span className="text-[12px] text-fg-2">Sesi opencode yang sedang jalan perlu direstart.</span>
          </div>
        )}

        <div className="flex min-h-0 flex-1 gap-5">
          <section className="flex w-90 shrink-0 flex-col gap-4 overflow-y-auto">
            <div className="flex flex-col gap-1.5">
              <span className="text-xs text-fg-3">Tipe provider</span>
              <select disabled defaultValue={ADAPTER} className={`${field} disabled:opacity-55`}>
                <option value={ADAPTER}>openai-compatible</option>
              </select>
              <SubHeader text="Adapter openai-compatible dipakai untuk gateway apa pun." />
            </div>

            <label className="flex flex-col gap-1.5">
              <span className="text-xs text-fg-3">Nama</span>
              <input
                value={draft.name}
                onChange={(e) => patch({ name: e.target.value })}
                placeholder="aki gateway"
                className={field}
              />
            </label>

            <label className="flex flex-col gap-1.5">
              <span className="text-xs text-fg-3">ID provider</span>
              <input
                value={draft.id}
                readOnly={!creating}
                onChange={(e) => patch({ id: e.target.value })}
                placeholder="aki"
                className={`${field} font-mono disabled:opacity-55 ${!creating || idValid ? "" : "border-err"}`}
              />
              <SubHeader
                text={creating ? "Huruf, angka, minus dan garis bawah." : "ID tidak bisa diubah setelah dibuat."}
              />
            </label>

            <label className="flex flex-col gap-1.5">
              <span className="text-xs text-fg-3">Adapter npm</span>
              <input
                value={draft.npm}
                onChange={(e) => patch({ npm: e.target.value })}
                className={`${field} font-mono`}
              />
            </label>

            <label className="flex flex-col gap-1.5">
              <span className="text-xs text-fg-3">Base URL</span>
              <input
                value={draft.baseUrl}
                onChange={(e) => patch({ baseUrl: e.target.value })}
                placeholder="https://gateway.example.com/v1"
                className={`${field} font-mono`}
              />
            </label>

            <div className="flex items-center justify-between rounded-md border border-border px-3 py-2.5">
              <div className="flex flex-col gap-0.5">
                <span className="text-[13px] text-fg">Aktif</span>
                <SubHeader text="Nonaktif berarti ID masuk ke disabled_providers." />
              </div>
              <button
                type="button"
                onClick={() => patch({ enabled: !draft.enabled })}
                className={`relative h-5.5 w-10 shrink-0 cursor-pointer rounded-full ${draft.enabled ? "bg-busy" : "bg-surface-2"}`}
              >
                <span
                  className={`absolute top-0.75 size-4 rounded-full bg-fg transition-all ${
                    draft.enabled ? "left-5.25" : "left-0.75"
                  }`}
                />
              </button>
            </div>

            <div className="flex flex-col gap-2">
              <span className="text-xs text-fg-3">Gaya header</span>
              <div className="flex rounded-md border border-border p-0.5">
                {(["bearer", "custom"] as HeaderStyle[]).map((style) => (
                  <button
                    key={style}
                    type="button"
                    onClick={() => patch({ headerStyle: style, customHeaderName: style === "custom" ? draft.customHeaderName : null })}
                    className={`flex-1 cursor-pointer rounded px-3 py-1.5 text-[12.5px] ${
                      draft.headerStyle === style ? "bg-surface-2 font-semibold text-fg" : "text-fg-3"
                    }`}
                  >
                    {style === "bearer" ? "Authorization: Bearer" : "Header custom"}
                  </button>
                ))}
              </div>
              {draft.headerStyle === "custom" ? (
                <label className="flex flex-col gap-1.5">
                  <span className="text-xs text-fg-3">Nama header</span>
                  <input
                    value={draft.customHeaderName ?? ""}
                    onChange={(e) => patch({ customHeaderName: e.target.value === "" ? null : e.target.value })}
                    placeholder="x-api-key"
                    className={`${field} font-mono`}
                  />
                  <SubHeader text="Gateway aki menolak Bearer dan menerima x-api-key." />
                </label>
              ) : (
                <SubHeader text="Gateway aki menolak Bearer dan menerima x-api-key." />
              )}
            </div>

            {creating ? (
              <div className="rounded-md border border-dashed border-border px-3 py-2.5 text-[11.5px] text-fg-3">
                Key bisa diisi setelah provider disimpan.
              </div>
            ) : existing !== null ? (
              <KeyField providerId={existing.id} keyMasked={existing.keyMasked} keyInline={existing.keyInline} />
            ) : null}

            {existing?.keyInline === true && (
              <button
                type="button"
                disabled={migrate.isPending}
                onClick={() => migrate.mutate(existing.id)}
                className="cursor-pointer rounded-md border border-waiting/50 px-3 py-1.75 text-xs font-medium text-waiting disabled:cursor-default disabled:opacity-45"
              >
                {migrate.isPending ? "Memindahkan…" : "Pindahkan key ke file 0600"}
              </button>
            )}

            <div className="flex items-center gap-2 border-t border-border pt-4">
              <button
                type="button"
                disabled={!canSave || save.isPending}
                onClick={() => save.mutate(draft)}
                className="cursor-pointer rounded-md bg-busy px-4 py-1.75 text-xs font-semibold text-bg disabled:cursor-default disabled:opacity-45"
              >
                {save.isPending ? "Menyimpan…" : "Simpan"}
              </button>
              {!creating && (
                <button
                  type="button"
                  disabled={remove.isPending}
                  onClick={() => {
                    if (confirm(`Hapus provider ${draft.id}? File key tidak ikut dihapus.`)) remove.mutate(draft.id);
                  }}
                  className="flex cursor-pointer items-center gap-1.5 rounded-md border border-border px-3 py-1.75 text-xs font-medium text-fg-2 hover:text-err disabled:cursor-default disabled:opacity-45"
                >
                  <Trash2 size={14} />
                  Hapus provider
                </button>
              )}
            </div>

            <SubHeader text="opencode perlu restart setelah config berubah." />
          </section>

          <section className="flex min-h-0 min-w-0 flex-1 flex-col gap-3">
            <div className="flex items-center justify-between">
              <span className="text-sm font-semibold">Model</span>
              <span className="text-xs text-fg-3">{draft.models.length} model</span>
            </div>
            <ModelList
              providerId={existing?.id ?? draft.id}
              providerReady={existing !== null}
              models={draft.models}
              onChange={setModels}
            />
          </section>
        </div>
      </main>

      {backups && <BackupMenu onClose={() => setBackups(false)} />}
    </div>
  );
};
