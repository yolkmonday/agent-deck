import type { DailyRow } from "@/lib/api";
import { totalTokens } from "@/lib/format";

export const formatUsd = (n: number): string => {
  if (n === 0) return "$0,00";
  if (Math.abs(n) < 0.01) return "<$0,01";
  return `$${n.toFixed(2).replace(".", ",")}`;
};

export const formatPct = (n: number): string => `${Math.round(n)}%`;

export interface DailyStack {
  date: string;
  claude: number;
  opencode: number;
  codex: number;
  total: number;
}

export const stackByDate = (rows: DailyRow[]): DailyStack[] => {
  const byDate = new Map<string, DailyStack>();
  for (const row of rows) {
    const bucket = byDate.get(row.date) ?? { date: row.date, claude: 0, opencode: 0, codex: 0, total: 0 };
    const tokens = totalTokens(row.tokens);
    bucket[row.agent] += tokens;
    bucket.total += tokens;
    byDate.set(row.date, bucket);
  }
  return [...byDate.values()].sort((a, b) => a.date.localeCompare(b.date));
};
