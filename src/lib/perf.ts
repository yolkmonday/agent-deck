import { locale, t } from "@/i18n";
import type { PerfAgg } from "@/lib/api";

export type PerfSortKey = "tpsP50" | "tpsP10" | "ttftP50Ms" | "samples";
export const LOW_SAMPLES = 5;

const oneDecimal = (n: number) =>
  new Intl.NumberFormat(locale(), { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(n);

export const formatTps = (v: number, precise: boolean): string => `${precise ? "" : "~"}${oneDecimal(v)}`;

export const formatTtft = (ms: number | null): string => {
  if (ms === null) return "-";
  if (ms < 1000) return `${Math.round(ms)} ms`;
  return t("format.seconds", { s: oneDecimal(ms / 1000) });
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
  r.label.toLowerCase().includes(q) ||
  r.models.some((m) => m.toLowerCase().includes(q)) ||
  r.children.some((c) => matches(c, q));

export const filterPerf = (rows: PerfAgg[], q: string): PerfAgg[] => {
  const needle = q.trim().toLowerCase();
  return needle === "" ? rows : rows.filter((r) => matches(r, needle));
};

// Flatten one level (top-level rows + their direct children) so a key can be matched
// whether it belongs to a family row or one of its children in grouped mode. A key
// present at both levels keeps the first occurrence (top-level rows are visited first).
export const flattenOneLevel = (rows: PerfAgg[]): Map<string, PerfAgg> => {
  const byKey = new Map<string, PerfAgg>();
  for (const r of rows) {
    if (!byKey.has(r.key)) byKey.set(r.key, r);
    for (const c of r.children) {
      if (!byKey.has(c.key)) byKey.set(c.key, c);
    }
  }
  return byKey;
};

export const chartSeries = (rows: PerfAgg[], keys: string[]): Array<Record<string, string | number>> => {
  const byKey = flattenOneLevel(rows);
  const byDay = new Map<string, Record<string, string | number>>();
  for (const key of keys) {
    const r = byKey.get(key);
    if (!r) continue;
    for (const d of r.daily) {
      const point = byDay.get(d.day) ?? { day: d.day };
      point[key] = d.tpsP50;
      byDay.set(d.day, point);
    }
  }
  return [...byDay.values()].sort((x, y) => String(x.day).localeCompare(String(y.day)));
};
