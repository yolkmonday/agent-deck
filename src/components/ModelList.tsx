import { useMutation } from "@tanstack/react-query";
import { CheckCircle2, Loader2, Plus, Search, Trash2, XCircle, Zap } from "lucide-react";
import { useEffect, useState } from "react";
import { ModelPickerDialog } from "@/components/ModelPickerDialog";
import { useT } from "@/i18n";
import type { ModelTestResult, OcModel } from "@/lib/api";
import { modelTest, modelsFetch } from "@/lib/api";
import {
  applyDefaultLimits,
  applyLimits,
  DEFAULT_LIMITS_NOTE,
  filterModels,
  incompleteModels,
  mergeFetched,
  removeSelected,
  toggleAll,
  toggleOne,
} from "@/lib/models";

interface ModelListProps {
  providerId: string;
  providerReady: boolean;
  models: OcModel[];
  onChange: (models: OcModel[]) => void;
}

const blankModel = (): OcModel => ({ id: "", name: null, contextLimit: null, outputLimit: null });

const DraftRow = ({
  onAdd,
  onCancel,
}: {
  onAdd: (model: OcModel) => void;
  onCancel: () => void;
}) => {
  const t = useT();
  const [draft, setDraft] = useState<OcModel>(blankModel());
  const limitsOk = draft.contextLimit !== null && draft.outputLimit !== null;
  const valid = draft.id.trim() !== "" && limitsOk;

  const field = (label: string, key: "id" | "name" | "contextLimit" | "outputLimit", numeric = false) => (
    <label className="flex flex-col gap-1">
      <span className="text-[11px] text-fg-3">{label}</span>
      <input
        value={draft[key] === null ? "" : String(draft[key])}
        inputMode={numeric ? "numeric" : undefined}
        onChange={(e) => {
          const raw = e.target.value;
          if (numeric) setDraft((p) => ({ ...p, [key]: raw === "" ? null : Number(raw) }));
          else setDraft((p) => ({ ...p, [key]: raw === "" ? null : raw }));
        }}
        className="rounded-md border border-border bg-bg px-2 py-1.5 font-mono text-[12px] text-fg outline-none focus:border-busy"
      />
    </label>
  );

  return (
    <div className="flex flex-col gap-2.5 rounded-md border border-busy/40 bg-surface-2 p-3">
      <div className="grid grid-cols-2 gap-2.5">
        {field(t("modelList.idLabel"), "id")}
        {field(t("modelList.displayNameLabel"), "name")}
        {field(t("modelList.contextLimitLabel"), "contextLimit", true)}
        {field(t("modelList.outputLimitLabel"), "outputLimit", true)}
      </div>
      <span className="text-[11px] text-fg-3">{t("modelList.limitsRequired")}</span>
      <div className="flex justify-end gap-2">
        <button
          type="button"
          onClick={onCancel}
          className="ad-interactive ad-press cursor-pointer rounded-md border border-border px-3 py-1.5 text-[12px] text-fg-2 hover:border-fg-3 hover:text-fg"
        >
          {t("common.cancel")}
        </button>
        <button
          type="button"
          disabled={!valid}
          onClick={() =>
            onAdd({
              id: draft.id.trim(),
              name: draft.name?.trim() || null,
              contextLimit: draft.contextLimit,
              outputLimit: draft.outputLimit,
            })
          }
          className="ad-interactive ad-press cursor-pointer rounded-md bg-busy px-3 py-1.5 text-[12px] font-semibold text-bg hover:opacity-90 disabled:cursor-default disabled:opacity-45"
        >
          {t("common.add")}
        </button>
      </div>
    </div>
  );
};

const TestResult = ({ result }: { result: ModelTestResult }) => {
  const t = useT();
  return (
    <span className={`flex items-center gap-1.5 font-mono text-[11px] ${result.ok ? "text-ok" : "text-err"}`}>
      {result.ok ? <CheckCircle2 size={12} /> : <XCircle size={12} />}
      {result.ok
        ? t("modelList.testOk", { ms: result.latencyMs })
        : (result.error ?? t("modelList.httpStatus", { status: result.status ?? "?" }))}
    </span>
  );
};

export const ModelList = ({ providerId, providerReady, models, onChange }: ModelListProps) => {
  const t = useT();
  const [search, setSearch] = useState("");
  const [adding, setAdding] = useState(false);
  const [addError, setAddError] = useState<string | null>(null);
  const [fetched, setFetched] = useState<string[] | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [settingLimits, setSettingLimits] = useState(false);
  const [limitContext, setLimitContext] = useState("");
  const [limitOutput, setLimitOutput] = useState("");
  const [limitError, setLimitError] = useState<string | null>(null);
  const [tests, setTests] = useState<Record<string, ModelTestResult>>({});

  // Selection points at ids from the previous provider once the target changes.
  useEffect(() => {
    setSelected([]);
    setSettingLimits(false);
    setLimitError(null);
  }, [providerId]);

  const fetchModels = useMutation({
    mutationFn: () => modelsFetch(providerId),
    onSuccess: (ids) => {
      setAddError(null);
      if (ids.length === 0) {
        setAddError(t("modelList.noModelsReturned"));
        setFetched(null);
        return;
      }
      setFetched(ids);
    },
    onError: (e) => setAddError(String(e)),
  });

  const test = useMutation({
    mutationFn: (model: string) => modelTest(providerId, model),
    onSuccess: (result, model) => setTests((p) => ({ ...p, [model]: result })),
    onError: (e, model) =>
      setTests((p) => ({
        ...p,
        [model]: { ok: false, status: null, latencyMs: 0, reply: null, error: String(e), usedHeader: "bearer" },
      })),
  });

  const rows = filterModels(models, search);
  const visibleIds = rows.map((m) => m.id);
  const allVisibleSelected = visibleIds.length > 0 && visibleIds.every((id) => selected.includes(id));
  const someVisibleSelected = visibleIds.some((id) => selected.includes(id));

  const missingLimit = incompleteModels(models).length;

  const firstSelected = models.find((m) => selected.includes(m.id)) ?? null;

  const openLimitForm = () => {
    setLimitError(null);
    setSettingLimits(true);
    const peers = models.filter((m) => selected.includes(m.id));
    const contextAgrees = peers.every((m) => m.contextLimit === firstSelected?.contextLimit);
    const outputAgrees = peers.every((m) => m.outputLimit === firstSelected?.outputLimit);
    setLimitContext(contextAgrees && firstSelected?.contextLimit !== null ? String(firstSelected?.contextLimit) : "");
    setLimitOutput(outputAgrees && firstSelected?.outputLimit !== null ? String(firstSelected?.outputLimit) : "");
  };

  const applyLimitForm = () => {
    const context = Number(limitContext);
    const output = Number(limitOutput);
    if (limitContext.trim() === "" || limitOutput.trim() === "" || !(context > 0) || !(output > 0)) {
      setLimitError(t("modelList.mustBePositive"));
      return;
    }
    onChange(applyLimits(models, selected, { context, output }));
    setSettingLimits(false);
    setLimitError(null);
    setSelected([]);
  };

  const bulk = (next: OcModel[]) => {
    onChange(next);
    setSelected([]);
    setSettingLimits(false);
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      <div className="flex items-center gap-2">
        <div className="flex min-w-0 flex-1 items-center gap-2 rounded-md border border-border bg-bg px-2.5 py-1.75">
          <Search size={14} className="shrink-0 text-fg-3" />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t("modelList.searchPlaceholder")}
            className="min-w-0 flex-1 bg-transparent font-mono text-[12px] text-fg outline-none"
          />
        </div>
        <button
          type="button"
          disabled={!providerReady || fetchModels.isPending}
          onClick={() => fetchModels.mutate()}
          className="ad-interactive ad-press flex cursor-pointer items-center gap-1.5 rounded-md border border-border px-3 py-1.75 text-xs font-medium text-fg-2 hover:text-fg disabled:cursor-default disabled:opacity-45"
        >
          {fetchModels.isPending ? <Loader2 size={14} className="animate-spin" /> : <Zap size={14} />}
          {t("modelList.fetchFromEndpoint")}
        </button>
        <button
          type="button"
          onClick={() => setAdding(true)}
          className="ad-interactive ad-press flex cursor-pointer items-center gap-1.5 rounded-md border border-border px-3 py-1.75 text-xs font-medium text-fg-2 hover:text-fg"
        >
          <Plus size={14} />
          {t("modelList.manualModel")}
        </button>
      </div>

      {(!providerReady || fetchModels.isPending) && (
        <span className="text-[11.5px] text-fg-3">
          {fetchModels.isPending ? t("modelList.fetchingList") : t("modelList.saveProviderFirst")}
        </span>
      )}

      {addError !== null && (
        <div className="rounded-md border border-err/40 bg-err/10 px-3 py-2 font-mono text-[11.5px] text-err">
          {addError}
        </div>
      )}

      {missingLimit > 0 && (
        <span className="text-[11.5px] text-waiting">
          {t(missingLimit === 1 ? "modelList.missingLimit.one" : "modelList.missingLimit.other", { n: missingLimit })}
        </span>
      )}

      {selected.length > 0 && (
        <div className="flex flex-col gap-2.5 rounded-md border border-busy/40 bg-surface-2 px-3 py-2.5">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-[12px] font-medium text-fg">
              {t("modelList.selectedCount", { n: selected.length })}
            </span>
            <button
              type="button"
              onClick={() => (settingLimits ? setSettingLimits(false) : openLimitForm())}
              className="ad-interactive ad-press cursor-pointer rounded-md border border-border px-2.5 py-1 text-[11.5px] text-fg-2 hover:text-fg"
            >
              {t("modelList.setLimit")}
            </button>
            <button
              type="button"
              onClick={() => bulk(applyDefaultLimits(models, selected))}
              className="ad-interactive ad-press cursor-pointer rounded-md border border-border px-2.5 py-1 text-[11.5px] text-fg-2 hover:text-fg"
            >
              {t("modelList.fillDefaults")}
            </button>
            <button
              type="button"
              onClick={() => {
                if (
                  confirm(
                    t(selected.length === 1 ? "modelList.confirmDelete.one" : "modelList.confirmDelete.other", {
                      n: selected.length,
                    }),
                  )
                )
                  bulk(removeSelected(models, selected));
              }}
              className="ad-interactive ad-press cursor-pointer rounded-md border border-border px-2.5 py-1 text-[11.5px] text-fg-2 hover:border-err/60 hover:text-err"
            >
              {t("common.delete")}
            </button>
            <button
              type="button"
              onClick={() => {
                setSelected([]);
                setSettingLimits(false);
              }}
              className="ad-interactive ad-press cursor-pointer rounded-md px-2.5 py-1 text-[11.5px] text-fg-3 hover:text-fg"
            >
              {t("modelList.clearSelection")}
            </button>
            <span className="text-[11px] text-fg-3">{DEFAULT_LIMITS_NOTE}</span>
          </div>
          <span className="text-[11px] text-fg-3">{t("modelList.deleteConfigOnly")}</span>
          {settingLimits && (
            <div className="flex flex-wrap items-end gap-2.5">
              <label className="flex flex-col gap-1">
                <span className="text-[11px] text-fg-3">{t("modelList.contextLabel")}</span>
                <input
                  value={limitContext}
                  inputMode="numeric"
                  onChange={(e) => setLimitContext(e.target.value)}
                  className="w-28 rounded-md border border-border bg-bg px-2 py-1.5 font-mono text-[12px] text-fg outline-none focus:border-busy"
                />
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-[11px] text-fg-3">{t("modelList.outputLabel")}</span>
                <input
                  value={limitOutput}
                  inputMode="numeric"
                  onChange={(e) => setLimitOutput(e.target.value)}
                  className="w-28 rounded-md border border-border bg-bg px-2 py-1.5 font-mono text-[12px] text-fg outline-none focus:border-busy"
                />
              </label>
              <button
                type="button"
                onClick={applyLimitForm}
                className="ad-interactive ad-press cursor-pointer rounded-md bg-busy px-3 py-1.5 text-[12px] font-semibold text-bg hover:opacity-90"
              >
                {t("modelList.apply")}
              </button>
              {limitError !== null && <span className="text-[11px] text-err">{limitError}</span>}
            </div>
          )}
        </div>
      )}

      {adding && (
        <DraftRow
          onCancel={() => setAdding(false)}
          onAdd={(model) => {
            onChange([...models, model]);
            setAdding(false);
          }}
        />
      )}

      {fetched !== null && (
        <ModelPickerDialog
          fetched={fetched}
          existing={models}
          onCancel={() => setFetched(null)}
          onAdd={(ids) => {
            onChange(mergeFetched(models, ids));
            setFetched(null);
          }}
        />
      )}

      <div className="min-h-0 flex-1 overflow-y-auto rounded-[10px] border border-border bg-surface px-4 py-2">
        {rows.length === 0 ? (
          <div className="py-10 text-center text-sm text-fg-3">
            {models.length === 0 ? t("modelList.noModelsYet") : t("modelList.noModelsMatch")}
          </div>
        ) : (
          <table className="w-full border-collapse">
            <thead>
              <tr className="border-b border-border">
                <th className="w-8 pb-2.5 text-left">
                  <input
                    type="checkbox"
                    checked={allVisibleSelected}
                    ref={(el) => {
                      if (el !== null) el.indeterminate = !allVisibleSelected && someVisibleSelected;
                    }}
                    disabled={visibleIds.length === 0}
                    onChange={() => setSelected((p) => toggleAll(p, visibleIds))}
                    className="size-3.5 cursor-pointer accent-busy disabled:cursor-default disabled:opacity-45"
                  />
                </th>
                <th className="pb-2.5 text-left text-[11px] font-medium text-fg-3">{t("modelList.headerModel")}</th>
                <th className="pb-2.5 text-right text-[11px] font-medium text-fg-3">
                  {t("modelList.contextLabel")}
                </th>
                <th className="pb-2.5 text-right text-[11px] font-medium text-fg-3">
                  {t("modelList.outputLabel")}
                </th>
                <th />
                <th />
              </tr>
            </thead>
            <tbody>
              {rows.map((model) => {
                const result = tests[model.id];
                const busy = test.isPending && test.variables === model.id;
                const checked = selected.includes(model.id);
                return (
                  <tr key={model.id} className="border-b border-border/60 last:border-0">
                    <td className="py-2.5 pr-2">
                      <input
                        type="checkbox"
                        checked={checked}
                        onChange={() => setSelected((p) => toggleOne(p, model.id))}
                        className="size-3.5 cursor-pointer accent-busy"
                      />
                    </td>
                    <td className="py-2.5 pr-4">
                      <div className="flex flex-col gap-0.5">
                        <span className="font-mono text-[12.5px] text-fg">{model.id}</span>
                        {model.name !== null && <span className="text-[11.5px] text-fg-3">{model.name}</span>}
                      </div>
                    </td>
                    <td className="py-2.5 pr-4 text-right font-mono text-[12px] text-fg-2">
                      {model.contextLimit ?? <span className="text-waiting">{t("modelList.notFilled")}</span>}
                    </td>
                    <td className="py-2.5 pr-4 text-right font-mono text-[12px] text-fg-2">
                      {model.outputLimit ?? <span className="text-waiting">{t("modelList.notFilled")}</span>}
                    </td>
                    <td className="py-2.5 pr-3 text-right">
                      {busy ? (
                        <span className="flex items-center gap-1.5 text-[11px] text-fg-3">
                          <Loader2 size={12} className="animate-spin" />
                          {t("modelList.testing")}
                        </span>
                      ) : result ? (
                        <TestResult result={result} />
                      ) : null}
                    </td>
                    <td className="py-2.5 text-right">
                      <div className="flex items-center justify-end gap-1.5">
                        <button
                          type="button"
                          disabled={!providerReady || busy}
                          onClick={() => test.mutate(model.id)}
                          className="ad-interactive ad-press cursor-pointer rounded-md border border-border px-2.5 py-1 text-[11.5px] text-fg-2 hover:text-fg disabled:cursor-default disabled:opacity-45"
                        >
                          {t("modelList.test")}
                        </button>
                        <button
                          type="button"
                          onClick={() => onChange(models.filter((m) => m.id !== model.id))}
                          className="ad-interactive ad-press cursor-pointer rounded-md p-1.5 text-fg-3 hover:bg-err/10 hover:text-err"
                        >
                          <Trash2 size={14} />
                        </button>
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
};
