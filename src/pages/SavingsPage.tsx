import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { RangeFilter } from "@/components/RangeFilter";
import { SavingsChart } from "@/components/SavingsChart";
import { type MessageKey, useT } from "@/i18n";
import { savingsSummary } from "@/lib/api";
import type { SavingsCommand, SavingsSource } from "@/lib/api";
import { formatPct } from "@/lib/cost";
import { formatTokens } from "@/lib/format";

const WARNING_KEYS: Record<string, MessageKey> = {
  "database not found": "savingsPage.warningDbNotFound",
  "stats file not found": "savingsPage.warningStatsNotFound",
  "failed to read database": "savingsPage.warningReadDbFailed",
  "failed to parse stats": "savingsPage.warningParseStatsFailed",
};

const translateWarning = (warning: string, t: (key: MessageKey) => string): string => {
  const [source, ...rest] = warning.split(": ");
  const detail = rest.join(": ");
  if (!detail) return warning;
  const key = WARNING_KEYS[detail];
  return key ? `${source}: ${t(key)}` : warning;
};

const Kpi = ({ label, value, sub, tone = "text-fg" }: { label: string; value: string; sub: string; tone?: string }) => (
  <div className="flex flex-1 flex-col gap-1.5 border-l border-border pl-6 first:border-0 first:pl-0">
    <span className="text-xs text-fg-3">{label}</span>
    <span className={`font-mono text-[30px] font-semibold ${tone}`}>{value}</span>
    <span className="text-xs text-fg-2">{sub}</span>
  </div>
);

const sourceSub = (
  source: SavingsSource | undefined,
  fallback: string,
  t: ReturnType<typeof useT>,
) =>
  source?.available
    ? t(source.entries === 1 ? "savingsPage.commandsSaved.one" : "savingsPage.commandsSaved.other", {
        n: source.entries,
        pct: formatPct(source.savingsPct),
      })
    : fallback;

const TopCommands = ({ rows, t }: { rows: SavingsCommand[]; t: ReturnType<typeof useT> }) =>
  rows.length === 0 ? (
    <div className="flex h-64 items-center justify-center text-sm text-fg-3">{t("savingsPage.noTopCommands")}</div>
  ) : (
    <ul className="flex flex-col">
      {rows.map((r) => (
        <li key={r.command} className="flex items-center gap-3 border-b border-border/60 py-2.5 last:border-0">
          <span className="min-w-0 flex-1 truncate font-mono text-[12.5px] text-fg">{r.command}</span>
          <span className="shrink-0 font-mono text-[12.5px] text-fg-2">{formatTokens(r.savedTokens)}</span>
          <span className="w-11 shrink-0 text-right font-mono text-[12.5px] font-semibold text-ok">
            {formatPct(r.savingsPct)}
          </span>
        </li>
      ))}
    </ul>
  );

export const SavingsPage = () => {
  const t = useT();
  const [days, setDays] = useState(7);
  const { data, isError, error, isPending } = useQuery({
    queryKey: ["savings", days],
    queryFn: () => savingsSummary(days),
  });

  const rtk = data?.rtk;
  const lean = data?.leanCtx;
  const daily = data?.daily ?? [];
  const topCommands = data?.topCommands ?? [];
  const warnings = data?.warnings ?? [];
  const totalSaved = (rtk?.savedTokens ?? 0) + (lean?.savedTokens ?? 0);
  const totalSource = (rtk?.totalTokens ?? 0) + (lean?.totalTokens ?? 0);
  const empty = !isPending && !isError && daily.length === 0 && topCommands.length === 0;

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <header className="flex items-center justify-between px-7 py-4.5">
        <div className="flex flex-col gap-0.75">
          <h1 className="text-[22px] font-semibold">{t("savingsPage.title")}</h1>
          <span className="text-[12.5px] text-fg-2">{t("savingsPage.subtitle")}</span>
        </div>
        <RangeFilter value={days} onChange={setDays} />
      </header>
      <main className="flex min-w-0 flex-1 flex-col gap-6.5 overflow-y-auto px-7 pb-7">
        <div className="flex">
          <Kpi
            label={t("savingsPage.totalSavedLabel")}
            value={formatTokens(totalSaved)}
            sub={
              totalSource > 0
                ? t("savingsPage.savedPct", { pct: formatPct((totalSaved / totalSource) * 100) })
                : t("savingsPage.noData")
            }
            tone="text-ok"
          />
          <Kpi
            label="rtk"
            value={formatTokens(rtk?.savedTokens ?? 0)}
            sub={sourceSub(rtk, t("savingsPage.notAvailable"), t)}
            tone={rtk?.available ? "text-fg" : "text-fg-3"}
          />
          <Kpi
            label="lean-ctx"
            value={formatTokens(lean?.savedTokens ?? 0)}
            sub={sourceSub(lean, t("savingsPage.notAvailable"), t)}
            tone={lean?.available ? "text-fg" : "text-fg-3"}
          />
          <Kpi
            label={t("savingsPage.topCommandLabel")}
            value={topCommands[0]?.command ?? "-"}
            sub={
              topCommands[0]
                ? t("savingsPage.topCommandSaved", {
                    tokens: formatTokens(topCommands[0].savedTokens),
                    pct: formatPct(topCommands[0].savingsPct),
                  })
                : t("savingsPage.noData")
            }
            tone={topCommands[0] ? "text-fg" : "text-fg-3"}
          />
        </div>

        {isError && (
          <div className="rounded-md border border-err/40 bg-err/10 px-3 py-2 font-mono text-xs text-err">
            {String(error)}
          </div>
        )}

        {warnings.length > 0 && (
          <div className="flex flex-col gap-0.5 text-xs text-fg-3">
            {warnings.map((w) => (
              <span key={w}>{translateWarning(w, t)}</span>
            ))}
          </div>
        )}

        {isPending && (
          <div className="flex h-64 items-center justify-center text-sm text-fg-3">{t("savingsPage.loading")}</div>
        )}

        {empty && (
          <div className="rounded-[10px] border border-dashed border-border p-8 text-center text-sm text-fg-3">
            {t("savingsPage.empty")}
          </div>
        )}

        {!isPending && !isError && !empty && (
          <>
            <section className="flex flex-col gap-3">
              <div className="flex items-center justify-between">
                <span className="text-sm font-semibold">{t("savingsPage.dailyTitle")}</span>
                <span className="text-xs text-fg-3">{t("savingsPage.stackedBySource")}</span>
              </div>
              <div className="rounded-[10px] border border-border bg-surface px-5 py-4">
                <SavingsChart data={daily} />
              </div>
            </section>

            <section className="flex flex-col gap-3">
              <div className="flex items-center justify-between">
                <span className="text-sm font-semibold">{t("savingsPage.topCommandLabel")}</span>
                <span className="text-xs text-fg-3">{t("savingsPage.sortedByTokensSaved")}</span>
              </div>
              <div className="rounded-[10px] border border-border bg-surface px-5 py-4">
                <TopCommands rows={topCommands} t={t} />
              </div>
            </section>
          </>
        )}
      </main>
    </div>
  );
};
