import type { PerfAgg } from "@/lib/api";

export type PerfSortKey = "tpsP50" | "tpsP10" | "ttftP50Ms" | "samples";
export const LOW_SAMPLES = 5;
export const ESTIMATE_HINT = "Perkiraan dari timestamp sesi, termasuk waktu tunggu token pertama.";

const oneDecimal = new Intl.NumberFormat("id-ID", { minimumFractionDigits: 1, maximumFractionDigits: 1 });

export const formatTps = (v: number, precise: boolean): string => `${precise ? "" : "~"}${oneDecimal.format(v)}`;

export const formatTtft = (ms: number | null): string => {
  if (ms === null) return "-";
  if (ms < 1000) return `${Math.round(ms)} ms`;
  return `${oneDecimal.format(ms / 1000)} dtk`;
};

export const sortPerf = (rows: PerfAgg[], key: PerfSortKey, desc: boolean): PerfAgg[] =>
  rows
    .map((r, i) => ({ r, i }))
    .sort((a, b) => {
      const av = a.r[key];
      const bv = b.r[key];
      if (av === null && bv === null) return a.i - b.i;
      if (av === null) return 1;
      if (bv === null) return -1;
      const d = desc ? bv - av : av - bv;
      return d !== 0 ? d : a.i - b.i;
    })
    .map(({ r }) => r);

const matches = (r: PerfAgg, q: string): boolean =>
  r.key.toLowerCase().includes(q) ||
  r.models.some((m) => m.toLowerCase().includes(q)) ||
  r.children.some((c) => matches(c, q));

export const filterPerf = (rows: PerfAgg[], q: string): PerfAgg[] => {
  const needle = q.trim().toLowerCase();
  return needle === "" ? rows : rows.filter((r) => matches(r, needle));
};

export const chartSeries = (rows: PerfAgg[], keys: string[]): Array<Record<string, string | number>> => {
  const byDay = new Map<string, Record<string, string | number>>();
  for (const r of rows.filter((x) => keys.includes(x.key))) {
    for (const d of r.daily) {
      const point = byDay.get(d.day) ?? { day: d.day };
      point[r.key] = d.tpsP50;
      byDay.set(d.day, point);
    }
  }
  return [...byDay.values()].sort((x, y) => String(x.day).localeCompare(String(y.day)));
};
