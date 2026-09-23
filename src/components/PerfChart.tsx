import { CartesianGrid, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { chartSeries, flattenOneLevel } from "@/lib/perf";
import type { PerfAgg } from "@/lib/api";

const COLORS = ["var(--color-claude)", "var(--color-opencode)", "var(--color-codex)", "var(--color-ok)", "var(--color-busy)"];

const short = (date: string) => `${date.slice(8, 10)}/${date.slice(5, 7)}`;

const TooltipBox = ({
  active,
  payload,
  label,
}: {
  active?: boolean;
  payload?: { dataKey?: string | number; name?: string; value?: number; color?: string }[];
  label?: string;
}) => {
  if (!active || !payload?.length) return null;
  return (
    <div className="rounded-lg border border-border bg-surface-2 px-3 py-2 shadow-lg">
      <div className="mb-1.5 text-xs font-semibold text-fg">{label}</div>
      {payload.map((p) => (
        <div key={String(p.dataKey)} className="flex items-center gap-2 text-[11.5px] text-fg-2">
          <span className="size-1.75 rounded-full" style={{ background: p.color }} />
          <span className="flex-1">{p.name ?? String(p.dataKey)}</span>
          <span className="font-mono text-fg">{p.value?.toFixed(1)}</span>
        </div>
      ))}
    </div>
  );
};

export const PerfChart = ({ rows, keys }: { rows: PerfAgg[]; keys: string[] }) => {
  const data = chartSeries(rows, keys);
  const byKey = flattenOneLevel(rows);
  if (data.length === 0) return <div className="flex h-64 items-center justify-center text-sm text-fg-3">Belum ada data.</div>;

  return (
    <div className="h-64 w-full">
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={data} margin={{ top: 4, right: 4, bottom: 0, left: -8 }}>
          <CartesianGrid vertical={false} stroke="var(--color-border)" strokeDasharray="3 4" />
          <XAxis
            dataKey="day"
            tickFormatter={short}
            tick={{ fill: "var(--color-fg-3)", fontSize: 11 }}
            tickLine={false}
            axisLine={{ stroke: "var(--color-border)" }}
          />
          <YAxis
            tick={{ fill: "var(--color-fg-3)", fontSize: 11 }}
            tickLine={false}
            axisLine={false}
            width={64}
          />
          <Tooltip content={<TooltipBox />} cursor={{ stroke: "var(--color-border)" }} />
          {keys.map((key, i) => (
            <Line
              key={key}
              type="monotone"
              dataKey={key}
              name={byKey.get(key)?.label ?? key}
              dot={false}
              strokeWidth={2}
              stroke={COLORS[i % COLORS.length]}
            />
          ))}
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
};
