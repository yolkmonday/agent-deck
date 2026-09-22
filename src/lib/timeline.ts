import type { TimelineSpan } from "@/lib/api";

const MIN_WIDTH_PCT = 0.5;

export const windowFor = (rangeMinutes: number, nowMs: number): { fromMs: number; toMs: number } => ({
  fromMs: nowMs - rangeMinutes * 60_000,
  toMs: nowMs,
});

const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n));

export const layoutSpan = (
  span: TimelineSpan,
  fromMs: number,
  toMs: number,
  nowMs: number,
): { leftPct: number; widthPct: number } => {
  const total = toMs - fromMs;
  if (total <= 0) return { leftPct: 0, widthPct: MIN_WIDTH_PCT };
  const rawStart = span.startMs;
  const rawEnd = span.endMs ?? nowMs;
  const start = clamp(rawStart, fromMs, toMs);
  const end = clamp(rawEnd, fromMs, toMs);
  const leftPct = clamp(((start - fromMs) / total) * 100, 0, 100);
  const widthPct = clamp(((end - start) / total) * 100, MIN_WIDTH_PCT, 100 - leftPct);
  return { leftPct, widthPct };
};

const pad2 = (n: number) => String(n).padStart(2, "0");

export const ticksFor = (fromMs: number, toMs: number, count: number): { ms: number; label: string }[] => {
  if (count <= 0) return [];
  if (count === 1) return [{ ms: fromMs, label: labelFor(fromMs) }];
  const step = (toMs - fromMs) / (count - 1);
  return Array.from({ length: count }, (_, i) => {
    const ms = fromMs + step * i;
    return { ms, label: labelFor(ms) };
  });
};

const labelFor = (ms: number): string => {
  const d = new Date(ms);
  return `${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
};
