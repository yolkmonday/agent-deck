import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, Check, History, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { KeyField } from "@/components/KeyField";
import { ModelList } from "@/components/ModelList";
import { ProviderIcon } from "@/components/BrandIcon";
import { locale, useT } from "@/i18n";
import type { HeaderStyle, OcModel, OcProviderInput } from "@/lib/api";
import { configBackups, configRestore, modelsOverview, secretMigrateInline } from "@/lib/api";
import { providerDelete, providerSave } from "@/lib/api";
import { incompleteModels } from "@/lib/models";
import { useNavigate } from "@/lib/nav";
import { isDraftDirty } from "@/lib/provider-draft";

const field =
  "rounded-md border border-border bg-bg px-3 py-2 text-[13px] text-fg outline-none focus:border-busy";

const ADAPTER = "@ai-sdk/openai-compatible";

const SubHeader = ({ text }: { text: string }) => (
  <span className="text-[11.5px] text-fg-3">{text}</span>
);

const BackupMenu = ({ onClose }: { onClose: () => void }) => {
  const t = useT();
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
          <span className="text-base font-semibold">{t("providerEditPage.restoreConfigTitle")}</span>
          <span className="text-xs text-fg-2">{t("providerEditPage.restoreConfigSubtitle")}</span>
        </div>

        {query.isError && (
          <div className="rounded-md border border-err/40 bg-err/10 px-3 py-2 font-mono text-xs text-err">
            {String(query.error)}
          </div>
        )}

        <div className="min-h-0 flex-1 overflow-y-auto">
          {query.isPending ? (
            <span className="text-xs text-fg-3">{t("providerEditPage.loadingBackups")}</span>
          ) : (query.data ?? []).length === 0 ? (
            <span className="text-xs text-fg-3">{t("providerEditPage.noBackups")}</span>
          ) : (
            <div className="flex flex-col">
              {(query.data ?? []).map((b) => (
                <div
                  key={b.path}
                  className="flex items-center gap-3 border-b border-border/60 py-2.5 last:border-0"
                >
                  <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                    <span className="truncate font-mono text-[11.5px] text-fg">{b.path}</span>
                    <span className="text-[11px] text-fg-3">{new Date(b.atMs).toLocaleString(locale())}</span>
                  </div>
                  {pending === b.path ? (
                    <div className="flex items-center gap-2">
                      <button
                        type="button"
                        onClick={() => restore.mutate(b.path)}
                        disabled={restore.isPending}
                        className="ad-interactive ad-press cursor-pointer rounded-md bg-err px-3 py-1.5 text-[11.5px] font-semibold text-bg hover:opacity-90 disabled:cursor-default disabled:opacity-45"
                      >
                        {restore.isPending ? t("providerEditPage.restoring") : t("providerEditPage.confirmQuestion")}
                      </button>
                      <button
                        type="button"
                        onClick={() => setPending(null)}
                        className="ad-interactive ad-press cursor-pointer rounded-md border border-border px-3 py-1.5 text-[11.5px] text-fg-2 hover:border-fg-3 hover:text-fg"
                      >
                        {t("common.cancel")}
                      </button>
                    </div>
                  ) : (
                    <button
                      type="button"
                      onClick={() => setPending(b.path)}
                      className="ad-interactive ad-press cursor-pointer rounded-md border border-border px-3 py-1.5 text-[11.5px] text-fg-2 hover:border-fg-3 hover:text-fg"
                    >
                      {t("providerEditPage.restore")}
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
  const t = useT();
  const qc = useQueryClient();
  const nav = useNavigate();
  const overview = useQuery({ queryKey: ["models-overview"], queryFn: modelsOverview });

  const existing = overview.data?.opencode.find((p) => p.id === providerId) ?? null;
  const creating = providerId === null;

  const [draft, setDraft] = useState<OcProviderInput | null>(null);
  const [baseline, setBaseline] = useState<OcProviderInput | null>(null);
  const [backups, setBackups] = useState(false);
  const [savedBackup, setSavedBackup] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [leaveConfirm, setLeaveConfirm] = useState(false);

  // Refill the form from the loaded provider whenever the target changes.
  useEffect(() => {
    setDraft(null);
    setBaseline(null);
    setLeaveConfirm(false);
  }, [providerId]);

  useEffect(() => {
    if (!creating && existing === null) return;
    if (draft !== null) return;
    const initial: OcProviderInput =
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
          };
    setDraft(initial);
    setBaseline(initial);
  }, [existing, creating, draft]);

  const save = useMutation({
    mutationFn: (input: OcProviderInput) => providerSave(input),
    onSuccess: async (provider, input) => {
      setError(null);
      setBaseline(input);
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
              className="ad-interactive ad-press flex cursor-pointer items-center gap-1.5 rounded-md border border-border px-3 py-1.75 text-xs font-medium text-fg-2 hover:text-fg"
    >
      <ArrowLeft size={14} />
      {t("providerEditPage.back")}
    </button>
  );

  if (overview.isPending || draft === null) {
    return (
      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex items-center justify-between px-7 py-4.5">
          <h1 className="text-[22px] font-semibold">{t("providerEditPage.manageProviderTitle")}</h1>
          {back}
        </header>
        <main className="flex flex-1 items-center justify-center text-sm text-fg-3">
          {overview.isError ? String(overview.error) : t("providerEditPage.loadingProviders")}
        </main>
      </div>
    );
  }

  const idValid = /^[A-Za-z0-9_-]{1,64}$/.test(draft.id);
  const incomplete = incompleteModels(draft.models);
  const limitsIncomplete = incomplete.length > 0;
  const canSave = draft.name.trim() !== "" && idValid && draft.baseUrl.trim() !== "" && !limitsIncomplete;
  const dirty = baseline !== null && isDraftDirty(draft, baseline);

  // Any edit invalidates the last save's success banner and error state, so
  // they never linger over changes that were never written to config.
  const patch = (next: Partial<OcProviderInput>) => {
    setSavedBackup(null);
    setError(null);
    setDraft((p) => (p === null ? p : { ...p, ...next }));
  };

  const setModels = (models: OcModel[]) => patch({ models });

  const requestBack = () => {
    if (dirty && !leaveConfirm) {
      setLeaveConfirm(true);
      return;
    }
    setLeaveConfirm(false);
    nav.provider("");
  };

  const backButton = (
    <div className="flex items-center gap-2">
      {leaveConfirm && <span className="text-[11.5px] text-waiting">{t("providerEditPage.unsavedChanges")}</span>}
      <button
        type="button"
        onClick={requestBack}
        className={`ad-interactive ad-press flex cursor-pointer items-center gap-1.5 rounded-md border px-3 py-1.75 text-xs font-medium hover:text-fg ${
          leaveConfirm ? "border-waiting text-waiting" : "border-border text-fg-2"
        }`}
      >
        <ArrowLeft size={14} />
        {leaveConfirm ? t("providerEditPage.discardAndBack") : t("providerEditPage.back")}
      </button>
      {leaveConfirm && (
        <button
          type="button"
          onClick={() => setLeaveConfirm(false)}
          className="ad-interactive ad-press cursor-pointer rounded-md border border-border px-3 py-1.75 text-xs font-medium text-fg-2 hover:text-fg"
        >
          {t("common.cancel")}
        </button>
      )}
    </div>
  );

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <header className="flex items-center justify-between px-7 py-4.5">
        <div className="flex items-center gap-3">
          <ProviderIcon providerId={creating ? draft.id : (providerId ?? draft.id)} size={28} />
          <div className="flex flex-col gap-0.75">
            <h1 className="text-[22px] font-semibold">
              {creating ? t("providerEditPage.newProviderTitle") : draft.name || draft.id}
            </h1>
            <span className="text-[12.5px] text-fg-2">{t("providerEditPage.writtenToConfigSubtitle")}</span>
          </div>
        </div>
        <div className="flex items-center gap-2">
          {!creating && (
            <button
              type="button"
              onClick={() => setBackups(true)}
      className="ad-interactive ad-press flex cursor-pointer items-center gap-1.5 rounded-md border border-border px-3 py-1.75 text-xs font-medium text-fg-2 hover:text-fg"
            >
              <History size={14} />
              {t("providerEditPage.restore")}
            </button>
          )}
          {backButton}
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
              {t("providerEditPage.savedBanner")}
            </span>
            <span className="break-all font-mono text-[11.5px] text-fg-2">{savedBackup}</span>
            <span className="text-[12px] text-fg-2">{t("providerEditPage.restartHint")}</span>
          </div>
        )}

        <div className="flex min-h-0 flex-1 gap-5">
          <section className="flex w-90 shrink-0 flex-col gap-4 overflow-y-auto">
            <div className="flex flex-col gap-1.5">
              <span className="text-xs text-fg-3">{t("providerEditPage.providerTypeLabel")}</span>
              <select disabled defaultValue={ADAPTER} className={`${field} disabled:opacity-55`}>
                <option value={ADAPTER}>openai-compatible</option>
              </select>
              <SubHeader text={t("providerEditPage.adapterHint")} />
            </div>

            <label className="flex flex-col gap-1.5">
              <span className="text-xs text-fg-3">{t("providerEditPage.nameLabel")}</span>
              <input
                value={draft.name}
                onChange={(e) => patch({ name: e.target.value })}
                placeholder="aki gateway"
                className={field}
              />
            </label>

            <label className="flex flex-col gap-1.5">
              <span className="text-xs text-fg-3">{t("providerEditPage.idLabel")}</span>
              <input
                value={draft.id}
                readOnly={!creating}
                onChange={(e) => patch({ id: e.target.value })}
                placeholder="aki"
                className={`${field} font-mono disabled:opacity-55 ${!creating || idValid ? "" : "border-err"}`}
              />
              <SubHeader
                text={
                  creating ? t("providerEditPage.idHintCreate") : t("providerEditPage.idHintLocked")
                }
              />
            </label>

            <label className="flex flex-col gap-1.5">
              <span className="text-xs text-fg-3">{t("providerEditPage.npmAdapterLabel")}</span>
              <input
                value={draft.npm}
                onChange={(e) => patch({ npm: e.target.value })}
                className={`${field} font-mono`}
              />
            </label>

            <label className="flex flex-col gap-1.5">
              <span className="text-xs text-fg-3">{t("providerEditPage.baseUrlLabel")}</span>
              <input
                value={draft.baseUrl}
                onChange={(e) => patch({ baseUrl: e.target.value })}
                placeholder="https://gateway.example.com/v1"
                className={`${field} font-mono`}
              />
            </label>

            <div className="flex items-center justify-between rounded-md border border-border px-3 py-2.5">
              <div className="flex flex-col gap-0.5">
                <span className="text-[13px] text-fg">{t("providerEditPage.activeLabel")}</span>
                <SubHeader text={t("providerEditPage.disabledHint")} />
              </div>
              <button
                type="button"
                onClick={() => patch({ enabled: !draft.enabled })}
                className={`ad-interactive ad-press relative h-5.5 w-10 shrink-0 cursor-pointer rounded-full hover:opacity-90 ${draft.enabled ? "bg-busy" : "bg-surface-2"}`}
              >
                <span
                  className={`absolute top-0.75 left-0.75 size-4 rounded-full bg-fg transition-transform duration-base ease-out-quart ${
                    draft.enabled ? "translate-x-4.5" : "translate-x-0"
                  }`}
                />
              </button>
            </div>

            <div className="flex flex-col gap-2">
              <span className="text-xs text-fg-3">{t("providerEditPage.headerStyleLabel")}</span>
              <div className="flex rounded-md border border-border p-0.5">
                {(["bearer", "custom"] as HeaderStyle[]).map((style) => (
                  <button
                    key={style}
                    type="button"
                    onClick={() => patch({ headerStyle: style, customHeaderName: style === "custom" ? draft.customHeaderName : null })}
                    className={`ad-interactive ad-press flex-1 cursor-pointer rounded px-3 py-1.5 text-[12.5px] hover:text-fg ${
                      draft.headerStyle === style ? "bg-surface-2 font-semibold text-fg" : "text-fg-3"
                    }`}
                  >
                    {style === "bearer" ? t("providerEditPage.authBearerLabel") : t("providerEditPage.customHeaderLabel")}
                  </button>
                ))}
              </div>
              {draft.headerStyle === "custom" ? (
                <label className="flex flex-col gap-1.5">
                  <span className="text-xs text-fg-3">{t("providerEditPage.headerNameLabel")}</span>
                  <input
                    value={draft.customHeaderName ?? ""}
                    onChange={(e) => patch({ customHeaderName: e.target.value === "" ? null : e.target.value })}
                    placeholder="x-api-key"
                    className={`${field} font-mono`}
                  />
                  <SubHeader text={t("providerEditPage.gatewayHint")} />
                </label>
              ) : (
                <SubHeader text={t("providerEditPage.gatewayHint")} />
              )}
            </div>

            {creating ? (
              <div className="rounded-md border border-dashed border-border px-3 py-2.5 text-[11.5px] text-fg-3">
                {t("providerEditPage.keyAfterSaveHint")}
              </div>
            ) : existing !== null ? (
              <KeyField providerId={existing.id} keyMasked={existing.keyMasked} keyInline={existing.keyInline} />
            ) : null}

            {existing?.keyInline === true && (
              <button
                type="button"
                disabled={migrate.isPending}
                onClick={() => migrate.mutate(existing.id)}
                className="ad-interactive ad-press cursor-pointer rounded-md border border-waiting/50 px-3 py-1.75 text-xs font-medium text-waiting hover:bg-waiting/10 disabled:cursor-default disabled:opacity-45"
              >
                {migrate.isPending ? t("providerEditPage.migrating") : t("providerEditPage.migrateToFile")}
              </button>
            )}

            <div className="flex items-center gap-2 border-t border-border pt-4">
              <button
                type="button"
                disabled={!canSave || save.isPending || (!creating && !dirty)}
                onClick={() => save.mutate(draft)}
                className="ad-interactive ad-press cursor-pointer rounded-md bg-busy px-4 py-1.75 text-xs font-semibold text-bg hover:opacity-90 disabled:cursor-default disabled:opacity-45"
              >
                {save.isPending ? t("providerEditPage.savingLabel") : t("common.save")}
              </button>
              {dirty && !save.isPending && (
                <span className="text-[11.5px] font-medium text-waiting">{t("providerEditPage.notSavedLabel")}</span>
              )}
              {!creating && (
                <button
                  type="button"
                  disabled={remove.isPending}
                  onClick={() => {
                    if (confirm(t("providerEditPage.deleteConfirm", { id: draft.id }))) remove.mutate(draft.id);
                  }}
                  className="ad-interactive ad-press flex cursor-pointer items-center gap-1.5 rounded-md border border-border px-3 py-1.75 text-xs font-medium text-fg-2 hover:text-err disabled:cursor-default disabled:opacity-45"
                >
                  <Trash2 size={14} />
                  {t("providerEditPage.deleteProviderButton")}
                </button>
              )}
              {limitsIncomplete && (
                <span className="text-[11.5px] text-waiting">
                  {t(
                    incomplete.length === 1
                      ? "providerEditPage.modelsLimitIncomplete.one"
                      : "providerEditPage.modelsLimitIncomplete.other",
                    { n: incomplete.length },
                  )}
                </span>
              )}
            </div>

            <SubHeader text={t("providerEditPage.restartNote")} />
          </section>

          <section className="flex min-h-0 min-w-0 flex-1 flex-col gap-3">
            <div className="flex items-center justify-between">
              <span className="text-sm font-semibold">{t("providerEditPage.modelSectionLabel")}</span>
              <span className="text-xs text-fg-3">
                {t(
                  draft.models.length === 1 ? "providerEditPage.modelCount.one" : "providerEditPage.modelCount.other",
                  { n: draft.models.length },
                )}
              </span>
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
