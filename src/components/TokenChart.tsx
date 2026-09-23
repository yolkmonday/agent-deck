import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { useT } from "@/i18n";
import type { DailyStack } from "@/lib/cost";
import { formatTokens } from "@/lib/format";

const SERIES = [
  { key: "claude", label: "Claude", color: "var(--color-claude)" },
  { key: "opencode", label: "opencode", color: "var(--color-opencode)" },
  { key: "codex", label: "Codex", color: "var(--color-codex)" },
] as const;

const short = (date: string) => `${date.slice(8, 10)}/${date.slice(5, 7)}`;

const TooltipBox = ({
  active,
  payload,
  label,
}: {
  active?: boolean;
  payload?: { dataKey?: string | number; value?: number }[];
  label?: string;
}) => {
  const t = useT();
  if (!active || !payload?.length) return null;
  const sum = payload.reduce((acc, p) => acc + (p.value ?? 0), 0);
  return (
    <div className="rounded-lg border border-border bg-surface-2 px-3 py-2 shadow-lg">
      <div className="mb-1.5 text-xs font-semibold text-fg">{label}</div>
      {SERIES.map((s) => {
        const value = payload.find((p) => p.dataKey === s.key)?.value ?? 0;
        if (!value) return null;
        return (
          <div key={s.key} className="flex items-center gap-2 text-[11.5px] text-fg-2">
            <span className="size-1.75 rounded-full" style={{ background: s.color }} />
            <span className="flex-1">{s.label}</span>
            <span className="font-mono text-fg">{formatTokens(value)}</span>
          </div>
        );
      })}
      <div className="mt-1.5 flex items-center gap-2 border-t border-border pt-1.5 text-[11.5px] text-fg-2">
        <span className="flex-1">{t("tokenChart.total")}</span>
        <span className="font-mono text-fg">{formatTokens(sum)}</span>
      </div>
    </div>
  );
};

export const TokenChart = ({ data }: { data: DailyStack[] }) => {
  const t = useT();
  return data.length === 0 ? (
    <div className="flex h-64 items-center justify-center text-sm text-fg-3">{t("tokenChart.empty")}</div>
  ) : (
    <div className="h-64 w-full">
      <ResponsiveContainer width="100%" height="100%">
        <BarChart data={data} margin={{ top: 4, right: 4, bottom: 0, left: -8 }}>
          <CartesianGrid vertical={false} stroke="var(--color-border)" strokeDasharray="3 4" />
          <XAxis
            dataKey="date"
            tickFormatter={short}
            tick={{ fill: "var(--color-fg-3)", fontSize: 11 }}
            tickLine={false}
            axisLine={{ stroke: "var(--color-border)" }}
          />
          <YAxis
            tickFormatter={(v: number) => formatTokens(v)}
            tick={{ fill: "var(--color-fg-3)", fontSize: 11 }}
            tickLine={false}
            axisLine={false}
            width={64}
          />
          <Tooltip content={<TooltipBox />} cursor={{ fill: "var(--color-surface-2)", opacity: 0.6 }} />
          {SERIES.map((s) => (
            <Bar key={s.key} dataKey={s.key} stackId="tokens" fill={s.color} radius={[2, 2, 0, 0]} />
          ))}
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
};
