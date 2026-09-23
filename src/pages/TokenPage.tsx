import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { BillingDialog } from "@/components/BillingDialog";
import { ModelTable } from "@/components/ModelTable";
import { MonthlySummary } from "@/components/MonthlySummary";
import { PageTabs } from "@/components/PageTabs";
import { PerfPanel } from "@/components/PerfPanel";
import { PricingDialog } from "@/components/PricingDialog";
import { RangeFilter } from "@/components/RangeFilter";
import { TokenChart } from "@/components/TokenChart";
import { historyByModel, historyDaily, historyTotals } from "@/lib/api";
import { formatPct, formatUsd, stackByDate } from "@/lib/cost";
import { formatTokens } from "@/lib/format";

const Kpi = ({ label, value, sub, tone = "text-fg" }: { label: string; value: string; sub: string; tone?: string }) => (
  <div className="flex flex-1 flex-col gap-1.5 border-l border-border pl-6 first:border-0 first:pl-0">
    <span className="text-xs text-fg-3">{label}</span>
    <span className={`font-mono text-[30px] font-semibold ${tone}`}>{value}</span>
    <span className="text-xs text-fg-2">{sub}</span>
  </div>
);

export const TokenPage = () => {
  const [tab, setTab] = useState<"biaya" | "performa">("biaya");
  const [days, setDays] = useState(7);
  const [pricing, setPricing] = useState(false);
  const [billing, setBilling] = useState(false);
  const totals = useQuery({ queryKey: ["history", "totals", days], queryFn: () => historyTotals(days) });
  const daily = useQuery({ queryKey: ["history", "daily", days], queryFn: () => historyDaily(days) });
  const models = useQuery({ queryKey: ["history", "models", days], queryFn: () => historyByModel(days) });

  const isError = totals.isError || daily.isError || models.isError;
  const error = totals.error ?? daily.error ?? models.error;
  const loading = totals.isPending || daily.isPending || models.isPending;
  const t = totals.data;
  const modelRows = models.data ?? [];
  const dailyRows = daily.data ?? [];
  const empty = !loading && !isError && dailyRows.length === 0;

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <header className="flex items-center justify-between px-7 py-4.5">
        <div className="flex items-center gap-4">
          <div className="flex flex-col gap-0.75">
            <h1 className="text-[22px] font-semibold">Token &amp; Biaya</h1>
            <span className="text-[12.5px] text-fg-2">Pemakaian token dan estimasi biaya semua agent</span>
          </div>
          <PageTabs
            tabs={[
              { id: "biaya", label: "Biaya" },
              { id: "performa", label: "Performa" },
            ]}
            value={tab}
            onChange={(id) => setTab(id as "biaya" | "performa")}
          />
        </div>
        <div className="flex items-center gap-2.5">
          {tab === "biaya" && <RangeFilter value={days} onChange={setDays} />}
          <button
            type="button"
            onClick={() => setBilling(true)}
            className="ad-interactive ad-press cursor-pointer rounded-lg border border-border px-3 py-1.75 text-xs font-medium text-fg-2 hover:border-fg-3 hover:text-fg"
          >
            Langganan &amp; saldo
          </button>
          <button
            type="button"
            onClick={() => setPricing(true)}
            className="ad-interactive ad-press cursor-pointer rounded-lg border border-border px-3 py-1.75 text-xs font-medium text-fg-2 hover:border-fg-3 hover:text-fg"
          >
            Harga model
          </button>
        </div>
      </header>
      {pricing && <PricingDialog onClose={() => setPricing(false)} />}
      {billing && <BillingDialog onClose={() => setBilling(false)} />}
      <main className="flex min-w-0 flex-1 flex-col gap-6.5 overflow-y-auto px-7 pb-7">
        {tab === "biaya" ? (
          <>
            <MonthlySummary />
            <div className="flex">
              <Kpi label="Total token" value={formatTokens(t ? t.tokens.input + t.tokens.output + t.tokens.cacheRead + t.tokens.cacheWrite : 0)} sub={`${t?.messages ?? 0} pesan`} />
              <Kpi label="Estimasi biaya" value={formatUsd(t?.costUsd ?? 0)} sub="berdasarkan tabel harga" />
              <Kpi label="Cache hit" value={formatPct(t?.cacheHitPct ?? 0)} sub="cache read dibanding input" tone={t && t.cacheHitPct >= 50 ? "text-ok" : "text-fg"} />
              <Kpi label="Token reasoning" value={formatTokens(t?.tokens.reasoning ?? 0)} sub="tidak dihitung biaya" tone="text-fg-3" />
            </div>

            {isError && (
              <div className="rounded-md border border-err/40 bg-err/10 px-3 py-2 font-mono text-xs text-err">
                {String(error)}
              </div>
            )}

            {loading && <div className="flex h-64 items-center justify-center text-sm text-fg-3">Memuat data…</div>}

            {empty && (
              <div className="rounded-[10px] border border-dashed border-border p-8 text-center text-sm text-fg-3">
                Belum ada data. Index sedang berjalan atau belum ada aktivitas.
              </div>
            )}

            {!loading && !isError && dailyRows.length > 0 && (
              <>
                <section className="flex flex-col gap-3">
                  <div className="flex items-center justify-between">
                    <span className="text-sm font-semibold">Token per hari</span>
                    <span className="text-xs text-fg-3">Ditumpuk per agent</span>
                  </div>
                  <div className="rounded-[10px] border border-border bg-surface px-5 py-4">
                    <TokenChart data={stackByDate(dailyRows)} />
                  </div>
                </section>

                <section className="flex flex-col gap-3">
                  <div className="flex items-center justify-between">
                    <span className="text-sm font-semibold">Biaya per model</span>
                    <span className="text-xs text-fg-3">Urut biaya tertinggi</span>
                  </div>
                  <div className="rounded-[10px] border border-border bg-surface px-5 py-4">
                    <ModelTable rows={modelRows} />
                  </div>
                </section>
              </>
            )}
          </>
        ) : (
          <PerfPanel />
        )}
      </main>
    </div>
  );
};
