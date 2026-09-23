import { Fragment, useState } from "react";
import { AgentIcon } from "@/components/BrandIcon";
import { useT } from "@/i18n";
import type { PerfAgg } from "@/lib/api";
import { formatTps, formatTtft, LOW_SAMPLES, type PerfSortKey } from "@/lib/perf";

const SortHead = ({
  children,
  sortKey,
  sort,
  onSort,
}: {
  children: string;
  sortKey: PerfSortKey;
  sort: { key: PerfSortKey; desc: boolean };
  onSort: (key: PerfSortKey) => void;
}) => {
  const active = sort.key === sortKey;
  return (
    <th className="pb-2.5 text-right text-[11px] font-medium text-fg-3">
      <button
        type="button"
        onClick={() => onSort(sortKey)}
        className="ad-interactive ad-press cursor-pointer inline-flex items-center gap-1"
      >
        {children}
        {active && <span>{sort.desc ? "↓" : "↑"}</span>}
      </button>
    </th>
  );
};

const Row = ({
  row,
  depth,
  selected,
  onSelect,
  expanded,
  onToggleExpand,
}: {
  row: PerfAgg;
  depth: number;
  selected: boolean;
  onSelect: (key: string) => void;
  expanded: boolean;
  onToggleExpand: (key: string) => void;
}) => {
  const t = useT();
  const lowSamples = row.samples < LOW_SAMPLES;
  const hasChildren = row.children.length > 0;
  const estimateHint = t("format.estimateHint");
  return (
    <tr
      onClick={() => onSelect(row.key)}
      title={lowSamples ? "Sampel sedikit" : undefined}
      className={`cursor-pointer border-b border-border/60 last:border-0 ${lowSamples ? "opacity-50" : ""} ${selected ? "bg-surface-2" : ""}`}
    >
      <td className={`py-3 ${depth > 0 ? "pl-6" : ""}`}>
        <span className="flex items-center gap-1.5 font-mono text-[12.5px] text-fg">
          {hasChildren && (
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                onToggleExpand(row.key);
              }}
              className="ad-interactive ad-press cursor-pointer text-fg-3"
            >
              {expanded ? "▾" : "▸"}
            </button>
          )}
          <AgentIcon agent={row.agent} size={14} />
          {row.label}
        </span>
      </td>
      <td className="py-3 text-right font-mono text-[12.5px] font-semibold text-fg" title={!row.precise ? estimateHint : undefined}>
        {formatTps(row.tpsP50, row.precise)}
      </td>
      <td className="py-3 text-right font-mono text-[12.5px] text-fg-2" title={!row.precise ? estimateHint : undefined}>
        {formatTps(row.tpsP10, row.precise)}
      </td>
      <td className="py-3 text-right font-mono text-[12.5px] text-fg-2">{formatTtft(row.ttftP50Ms)}</td>
      <td className="py-3 text-right font-mono text-[12.5px] text-fg-2">{row.samples}</td>
    </tr>
  );
};

export const PerfTable = ({
  rows,
  sort,
  onSort,
  selected,
  onSelect,
}: {
  rows: PerfAgg[];
  sort: { key: PerfSortKey; desc: boolean };
  onSort: (key: PerfSortKey) => void;
  selected: string[];
  onSelect: (key: string) => void;
}) => {
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  const onToggleExpand = (key: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });

  return (
    <table className="w-full border-collapse">
      <thead>
        <tr className="border-b border-border">
          <th className="pb-2.5 text-left text-[11px] font-medium text-fg-3">Model</th>
          <SortHead sortKey="tpsP50" sort={sort} onSort={onSort}>TPS p50</SortHead>
          <SortHead sortKey="tpsP10" sort={sort} onSort={onSort}>TPS p10</SortHead>
          <SortHead sortKey="ttftP50Ms" sort={sort} onSort={onSort}>TTFT p50</SortHead>
          <SortHead sortKey="samples" sort={sort} onSort={onSort}>Sampel</SortHead>
        </tr>
      </thead>
      <tbody>
        {rows.map((r) => (
          <Fragment key={r.key}>
            <Row
              row={r}
              depth={0}
              selected={selected.includes(r.key)}
              onSelect={onSelect}
              expanded={expanded.has(r.key)}
              onToggleExpand={onToggleExpand}
            />
            {expanded.has(r.key) &&
              r.children.map((c) => (
                <Row
                  key={c.key}
                  row={c}
                  depth={1}
                  selected={selected.includes(c.key)}
                  onSelect={onSelect}
                  expanded={false}
                  onToggleExpand={onToggleExpand}
                />
              ))}
          </Fragment>
        ))}
      </tbody>
    </table>
  );
};
