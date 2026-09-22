import type { TimelineSpan } from "@/lib/api";
import { formatDuration, formatTokens, totalTokens } from "@/lib/format";

const statusLabel = { ok: "Selesai", error: "Gagal", running: "Berjalan" } as const;
const statusPill = {
  ok: "bg-ok/15 text-ok",
  error: "bg-err/15 text-err",
  running: "bg-waiting/15 text-waiting",
} as const;

const clock = (ms: number) =>
  new Date(ms).toLocaleTimeString("id-ID", { hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false }).replace(/\./g, ":");

const Row = ({ label, value }: { label: string; value: string }) => (
  <div className="flex items-center justify-between gap-3 border-b border-border/60 py-2.5 last:border-0">
    <span className="text-xs text-fg-3">{label}</span>
    <span className="truncate font-mono text-[12.5px] text-fg">{value}</span>
  </div>
);

export const SpanDetail = ({ span, nowMs }: { span: TimelineSpan | null; nowMs: number }) => (
  <aside className="flex w-82.5 shrink-0 flex-col gap-3.5 border-l border-border px-5.5 py-5.5">
    <span className="text-sm font-semibold">Detail blok</span>
    {!span ? (
      <span className="text-xs text-fg-3">Pilih satu blok untuk lihat detail.</span>
    ) : (
      <div className="flex flex-col gap-3.5">
        <div className="flex items-center justify-between gap-2">
          <span className="truncate font-mono text-[13px] font-semibold text-fg">{span.tool}</span>
          <span className={`shrink-0 rounded-full px-2.25 py-0.75 text-[11.5px] font-semibold ${statusPill[span.status]}`}>
            {statusLabel[span.status]}
          </span>
        </div>
        {span.detail && (
          <div className="rounded-md bg-bg px-3 py-2.5">
            <span className="break-words font-mono text-xs leading-[1.4] text-fg-2">{span.detail}</span>
          </div>
        )}
        <div className="flex flex-col">
          <Row label="Mulai" value={clock(span.startMs)} />
          <Row label="Selesai" value={span.endMs === null ? "belum selesai" : clock(span.endMs)} />
          <Row label="Durasi" value={formatDuration((span.endMs ?? nowMs) - span.startMs)} />
          <Row
            label="Token"
            value={span.tokens ? formatTokens(totalTokens(span.tokens)) : "-"}
          />
          <Row label="Token output" value={span.tokens ? formatTokens(span.tokens.output) : "-"} />
        </div>
      </div>
    )}
  </aside>
);
