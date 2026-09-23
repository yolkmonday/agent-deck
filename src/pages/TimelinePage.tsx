import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { RangeFilter } from "@/components/RangeFilter";
import { SpanDetail } from "@/components/SpanDetail";
import { TimelineLane } from "@/components/TimelineLane";
import { useT } from "@/i18n";
import { timelineSpans } from "@/lib/api";
import type { TimelineSpan } from "@/lib/api";
import { ticksFor, windowFor } from "@/lib/timeline";

const RANGE_MINUTES = [30, 120, 1440];
const TICK_COUNT = 6;

export const TimelinePage = () => {
  const t = useT();
  const RANGE_LABELS: Record<number, string> = {
    30: t("timelinePage.range30"),
    120: t("timelinePage.range120"),
    1440: t("timelinePage.range1440"),
  };
  const [minutes, setMinutes] = useState(30);
  const [selected, setSelected] = useState<TimelineSpan | null>(null);
  const nowMs = Date.now();
  const { fromMs, toMs } = windowFor(minutes, nowMs);
  const { data, isError, error, isPending } = useQuery({
    queryKey: ["timeline", minutes],
    queryFn: () => timelineSpans(fromMs, toMs),
  });

  const lanes = data ?? [];
  const ticks = ticksFor(fromMs, toMs, TICK_COUNT);
  const empty = !isPending && !isError && lanes.length === 0;

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <header className="flex items-center justify-between px-7 py-4.5">
        <div className="flex flex-col gap-0.75">
          <h1 className="text-[22px] font-semibold">{t("timelinePage.title")}</h1>
          <span className="text-[12.5px] text-fg-2">{t("timelinePage.subtitle")}</span>
        </div>
        <RangeFilter value={minutes} onChange={setMinutes} options={RANGE_MINUTES} labels={RANGE_LABELS} />
      </header>
      <div className="flex min-h-0 flex-1">
        <main className="flex min-w-0 flex-1 flex-col gap-4 overflow-y-auto px-7 pb-7">
          {isError && (
            <div className="rounded-md border border-err/40 bg-err/10 px-3 py-2 font-mono text-xs text-err">
              {String(error)}
            </div>
          )}

          {isPending && (
            <div className="flex h-64 items-center justify-center text-sm text-fg-3">{t("timelinePage.loading")}</div>
          )}

          {empty && (
            <div className="rounded-[10px] border border-dashed border-border p-8 text-center text-sm text-fg-3">
              {t("timelinePage.empty")}
            </div>
          )}

          {lanes.length > 0 && (
            <div className="flex flex-col rounded-[10px] border border-border bg-surface px-5 py-4">
              <div className="flex items-stretch">
                <div className="w-[180px] shrink-0 pr-4" />
                <div className="relative h-7 min-w-0 flex-1">
                  {ticks.map((t) => (
                    <span
                      key={t.ms}
                      style={{ left: `${((t.ms - fromMs) / (toMs - fromMs)) * 100}%` }}
                      className="absolute top-0 -translate-x-1/2 font-mono text-[11px] text-fg-3 first:translate-x-0 last:-translate-x-full"
                    >
                      {t.label}
                    </span>
                  ))}
                </div>
              </div>
              <div className="relative flex flex-col">
                {ticks.map((t) => (
                  <span
                    key={t.ms}
                    style={{ left: `calc(180px + (100% - 180px) * ${(t.ms - fromMs) / (toMs - fromMs)})` }}
                    className="pointer-events-none absolute top-0 bottom-0 w-px bg-border/40 last:bg-transparent"
                  />
                ))}
                {lanes.map((lane) => (
                  <TimelineLane
                    key={lane.sessionId}
                    lane={lane}
                    fromMs={fromMs}
                    toMs={toMs}
                    nowMs={nowMs}
                    selectedId={selected?.id ?? null}
                    onSelect={setSelected}
                  />
                ))}
              </div>
            </div>
          )}
        </main>
        <SpanDetail span={selected} nowMs={nowMs} />
      </div>
    </div>
  );
};
