import { useT } from "@/i18n";
import { formatUsd, formatNotional } from "@/lib/cost";
import { formatTokens } from "@/lib/format";
import { summarize } from "@/lib/summarize";
import type { Session } from "@/lib/types";

export const KpiRow = ({ sessions }: { sessions: Session[] }) => {
  const t = useT();
  const s = summarize(sessions);
  // A subscription is already paid for, so the headline is money that moved and
  // the API-rate figure is only a subtitle, and only when the two differ.
  const notionalOnly = s.notional !== s.spend;
  const items = [
    {
      label: t("kpiRow.activeAgents"),
      value: String(s.active),
      sub: t("kpiRow.activeAgentsSub", { busy: s.busy, waiting: s.waiting, idle: s.idle }),
      tone: "text-fg",
      alert: "",
      hint: "",
    },
    {
      label: t("kpiRow.needsInput"),
      value: String(s.waiting),
      sub: s.waiting ? t("kpiRow.waitingForYou") : t("kpiRow.none"),
      tone: s.waiting ? "text-waiting" : "text-fg",
      alert: "",
      hint: "",
    },
    {
      label: t("kpiRow.activeTokens"),
      value: formatTokens(s.tokens),
      sub: t("kpiRow.activeTokensSub"),
      tone: "text-fg",
      alert: "",
      hint: "",
    },
    {
      label: t("kpiRow.estCost"),
      value: formatUsd(s.spend),
      sub:
        (notionalOnly ? t("kpiRow.apiEquivalent", { value: formatNotional(s.notional) }) : t("kpiRow.allPriced")) +
        (s.unpriced === 0
          ? ""
          : ` ${t(s.unpriced === 1 ? "kpiRow.unpriced.one" : "kpiRow.unpriced.other", { n: s.unpriced })}`),
      tone: s.unpriced === 0 ? "text-fg" : "text-waiting",
      alert: "",
      hint: notionalOnly ? t("cost.notionalHint") : "",
    },
  ];
  if (s.stalled > 0) items[0].alert = t("kpiRow.stalled", { n: s.stalled });
  return (
    <div className="flex">
      {items.map((k, i) => (
        <div
          key={k.label}
          className={`flex min-w-0 flex-1 flex-col gap-1.5 ${i === 0 ? "pr-6" : "border-l border-border px-6"}`}
        >
          <span className="truncate text-xs text-fg-3">{k.label}</span>
          <span className={`font-mono text-[clamp(20px,2.2vw,30px)] font-semibold whitespace-nowrap ${k.tone}`}>
            {k.value}
          </span>
          <span className="truncate text-xs text-fg-2" title={k.hint || k.sub}>
            {k.sub}
            {k.alert && <span className="text-err"> {k.alert}</span>}
          </span>
        </div>
      ))}
    </div>
  );
};
