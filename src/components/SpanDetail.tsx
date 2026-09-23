import { locale, useT, type MessageKey } from "@/i18n";
import type { TimelineSpan } from "@/lib/api";
import { formatDuration, formatTokens, totalTokens } from "@/lib/format";

const statusLabel: Record<TimelineSpan["status"], MessageKey> = {
  ok: "spanDetail.statusOk",
  error: "spanDetail.statusError",
  running: "spanDetail.statusRunning",
};
const statusPill = {
  ok: "bg-ok/15 text-ok",
  error: "bg-err/15 text-err",
  running: "bg-waiting/15 text-waiting",
} as const;

const clock = (ms: number) =>
  new Date(ms).toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false }).replace(/\./g, ":");

const Row = ({ label, value }: { label: string; value: string }) => (
  <div className="flex items-center justify-between gap-3 border-b border-border/60 py-2.5 last:border-0">
    <span className="text-xs text-fg-3">{label}</span>
    <span className="truncate font-mono text-[12.5px] text-fg">{value}</span>
  </div>
);

export const SpanDetail = ({ span, nowMs }: { span: TimelineSpan | null; nowMs: number }) => {
  const t = useT();
  return (
    <aside className="flex w-82.5 shrink-0 flex-col gap-3.5 border-l border-border px-5.5 py-5.5">
      <span className="text-sm font-semibold">{t("spanDetail.title")}</span>
      {!span ? (
        <span className="text-xs text-fg-3">{t("spanDetail.selectPrompt")}</span>
      ) : (
        <div className="flex flex-col gap-3.5">
          <div className="flex items-center justify-between gap-2">
            <span className="truncate font-mono text-[13px] font-semibold text-fg">{span.tool}</span>
            <span className={`shrink-0 rounded-full px-2.25 py-0.75 text-[11.5px] font-semibold ${statusPill[span.status]}`}>
              {t(statusLabel[span.status])}
            </span>
          </div>
          {span.detail && (
            <div className="rounded-md bg-bg px-3 py-2.5">
              <span className="break-words font-mono text-xs leading-[1.4] text-fg-2">{span.detail}</span>
            </div>
          )}
          <div className="flex flex-col">
            <Row label={t("spanDetail.start")} value={clock(span.startMs)} />
            <Row
              label={t("spanDetail.end")}
              value={span.endMs === null ? t("spanDetail.notFinished") : clock(span.endMs)}
            />
            <Row label={t("spanDetail.duration")} value={formatDuration((span.endMs ?? nowMs) - span.startMs)} />
            <Row
              label={t("spanDetail.tokens")}
              value={span.tokens ? formatTokens(totalTokens(span.tokens)) : "-"}
            />
            <Row
              label={t("spanDetail.tokensOutput")}
              value={span.tokens ? formatTokens(span.tokens.output) : "-"}
            />
          </div>
        </div>
      )}
    </aside>
  );
};
