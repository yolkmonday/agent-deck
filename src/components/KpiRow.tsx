import { formatTokens } from "@/lib/format";
import { summarize } from "@/lib/summarize";
import type { Session } from "@/lib/types";

export const KpiRow = ({ sessions }: { sessions: Session[] }) => {
  const s = summarize(sessions);
  const items = [
    { label: "Agent aktif", value: String(s.active), sub: `${s.busy} sibuk · ${s.waiting} nunggu · ${s.idle} diam`, tone: "text-fg" },
    { label: "Butuh input", value: String(s.waiting), sub: s.waiting ? "menunggu jawaban kamu" : "tidak ada", tone: s.waiting ? "text-waiting" : "text-fg" },
    { label: "Token sesi aktif", value: formatTokens(s.tokens), sub: "gabungan semua sesi berjalan", tone: "text-fg" },
    { label: "Estimasi biaya", value: "-", sub: "tersedia setelah tabel harga (P2)", tone: "text-fg-3" },
  ];
  return (
    <div className="flex">
      {items.map((k, i) => (
        <div key={k.label} className={`flex flex-1 flex-col gap-1.5 ${i === 0 ? "pr-6" : "border-l border-border px-6"}`}>
          <span className="text-xs text-fg-3">{k.label}</span>
          <span className={`font-mono text-[30px] font-semibold ${k.tone}`}>{k.value}</span>
          <span className="text-xs text-fg-2">{k.sub}</span>
        </div>
      ))}
    </div>
  );
};
