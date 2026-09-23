import { useQuery } from "@tanstack/react-query";
import { Icon } from "@iconify/react";
import { useT, type MessageKey } from "@/i18n";
import { billingSummary, type AccountPeriod } from "@/lib/api";
import { formatUsd, formatNotional } from "@/lib/cost";
import { formatTokens, totalTokens } from "@/lib/format";
import type { BillingMode } from "@/lib/types";

const modeLabel: Record<BillingMode, MessageKey> = {
  subscription: "monthlySummary.modeSubscription",
  prepaid: "monthlySummary.modePrepaid",
  payg: "monthlySummary.modePayg",
};

const modeTone: Record<BillingMode, string> = {
  subscription: "bg-claude/15 text-claude",
  prepaid: "bg-opencode/15 text-opencode",
  payg: "bg-surface-2 text-fg-2",
};

const Progress = ({ left, total }: { left: number; total: number }) => {
  const pct = total <= 0 ? 0 : Math.max(0, Math.min(100, (left / total) * 100));
  return (
    <span className="block h-1 w-full overflow-hidden rounded-full bg-surface-2">
      <span
        className={`block h-full rounded-full ${pct <= 15 ? "bg-err" : pct <= 40 ? "bg-waiting" : "bg-ok"}`}
        style={{ width: `${pct}%` }}
      />
    </span>
  );
};

/** The one number that matters for this mode, and the period it covers. */
const Figure = ({ p }: { p: AccountPeriod }) => {
  const t = useT();
  if (p.expired) {
    return <span className="font-mono text-[12.5px] text-fg-3">{formatUsd(p.spendUsd)}</span>;
  }
  if (p.mode === "subscription") {
    return (
      <span className="flex items-baseline gap-2 whitespace-nowrap">
        <span className="font-mono text-[12.5px] font-semibold text-fg">
          {t("monthlySummary.perMonth", { value: formatUsd(p.committedUsd ?? 0) })}
        </span>
        <span className="text-[11.5px] text-fg-3">
          {p.daysLeft === null
            ? t("monthlySummary.active")
            : t(p.daysLeft === 1 ? "monthlySummary.daysLeft.one" : "monthlySummary.daysLeft.other", {
                n: p.daysLeft,
              })}
        </span>
        <span className="font-mono text-[11.5px] text-fg-3" title={t("cost.notionalHint")}>
          {t("monthlySummary.apiEquivalent", { value: formatNotional(p.notionalUsd) })}
        </span>
      </span>
    );
  }
  if (p.mode === "prepaid") {
    return (
      <span className="flex items-baseline gap-2 whitespace-nowrap">
        <span className="font-mono text-[12.5px] font-semibold text-fg">
          {t("monthlySummary.creditLeftOf", {
            left: formatUsd(p.creditLeftUsd ?? 0),
            total: formatUsd((p.creditLeftUsd ?? 0) + p.spendUsd),
          })}
        </span>
        <span className="text-[11.5px] text-fg-3">
          {p.daysLeft === null
            ? t("monthlySummary.noExpiry")
            : t(p.daysLeft === 1 ? "monthlySummary.expiresIn.one" : "monthlySummary.expiresIn.other", {
                n: p.daysLeft,
              })}
        </span>
      </span>
    );
  }
  return (
    <span className="font-mono text-[12.5px] font-semibold whitespace-nowrap text-fg">
      {t("monthlySummary.spentThisMonth", { value: formatUsd(p.spendUsd) })}
    </span>
  );
};

export const MonthlySummary = () => {
  const t = useT();
  const query = useQuery({ queryKey: ["billing", "summary"], queryFn: billingSummary });
  const periods = query.data?.periods ?? [];

  // With nothing to say there is no reason to take up the top of the page.
  if (query.isPending || query.isError || periods.length === 0) return null;

  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <span className="text-sm font-semibold">{t("monthlySummary.title")}</span>
        <span className="text-xs text-fg-3">
          {query.data.warnings.length > 0 ? (
            <span className="text-waiting">{query.data.warnings.join(" · ")}</span>
          ) : (
            t("monthlySummary.cycleSummary", { total: formatUsd(query.data.totalSpendUsd) })
          )}
        </span>
      </div>
      <div className="flex flex-col divide-y divide-border rounded-[10px] border border-border bg-surface px-5">
        {periods.map((p) => (
          <div key={p.accountId || "__unmatched"} className="flex flex-col gap-2 py-3.5">
            <div className="flex min-w-0 items-center justify-between gap-3">
              <span className="flex min-w-0 items-center gap-2">
                <span className="truncate text-[13px] font-semibold" title={p.label}>
                  {p.label}
                </span>
                <span
                  className={`shrink-0 rounded-full px-2 py-0.5 text-[10.5px] font-semibold ${modeTone[p.mode]}`}
                >
                  {t(modeLabel[p.mode])}
                </span>
                {p.expired && (
                  <span className="flex shrink-0 items-center gap-1 rounded-full bg-err/15 px-2 py-0.5 text-[10.5px] font-semibold text-err">
                    <Icon icon="lucide:octagon-alert" width={11} height={11} />
                    {t("monthlySummary.expired")}
                  </span>
                )}
              </span>
              <Figure p={p} />
            </div>
            <div className="flex items-center justify-between gap-3 text-[11.5px] text-fg-3">
              <span className="font-mono">
                {p.fromDate} → {p.toDate}
              </span>
              <span className="font-mono">
                {t(totalTokens(p.tokens) === 1 ? "monthlySummary.tokensCount.one" : "monthlySummary.tokensCount.other", {
                  value: formatTokens(totalTokens(p.tokens)),
                })}
              </span>
            </div>
            {p.mode === "prepaid" && (
              <Progress left={p.creditLeftUsd ?? 0} total={(p.creditLeftUsd ?? 0) + p.spendUsd} />
            )}
          </div>
        ))}
      </div>
    </section>
  );
};
