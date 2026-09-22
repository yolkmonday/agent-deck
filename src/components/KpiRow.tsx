import { formatUsd, formatNotional, NOTIONAL_HINT } from "@/lib/cost";
import { formatTokens } from "@/lib/format";
import { summarize } from "@/lib/summarize";
import type { Session } from "@/lib/types";

export const KpiRow = ({ sessions }: { sessions: Session[] }) => {
  const s = summarize(sessions);
  // A subscription is already paid for, so the headline is money that moved and
  // the API-rate figure is only a subtitle, and only when the two differ.
  const notionalOnly = s.notional !== s.spend;
  const items = [
    { label: "Agent aktif", value: String(s.active), sub: `${s.busy} sibuk · ${s.waiting} nunggu · ${s.idle} diam`, tone: "text-fg", alert: "", hint: "" },
    { label: "Butuh input", value: String(s.waiting), sub: s.waiting ? "menunggu jawaban kamu" : "tidak ada", tone: s.waiting ? "text-waiting" : "text-fg", alert: "", hint: "" },
    { label: "Token sesi aktif", value: formatTokens(s.tokens), sub: "gabungan semua sesi berjalan", tone: "text-fg", alert: "", hint: "" },
    {
      label: "Estimasi biaya",
      value: formatUsd(s.spend),
      sub:
        (notionalOnly ? formatNotional(s.notional) + " setara API" : "semua model punya harga") +
        (s.unpriced === 0 ? "" : ` · ${s.unpriced} model belum punya harga`),
      tone: s.unpriced === 0 ? "text-fg" : "text-waiting",
      alert: "",
      hint: notionalOnly ? NOTIONAL_HINT : "",
    },
  ];
  if (s.stalled > 0) items[0].alert = `· ${s.stalled} macet`;
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
