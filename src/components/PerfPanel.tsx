import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { PerfChart } from "@/components/PerfChart";
import { PerfTable } from "@/components/PerfTable";
import { RangeFilter } from "@/components/RangeFilter";
import { perfByModel, type PerfRange } from "@/lib/api";
import { filterPerf, sortPerf, type PerfSortKey } from "@/lib/perf";

const RANGE_BY_DAYS: Record<number, PerfRange> = { 1: "24h", 7: "7d", 30: "30d" };
const DAYS_BY_RANGE: Record<PerfRange, number> = { "24h": 1, "7d": 7, "30d": 30 };
const MAX_SELECTED = 5;

export const PerfPanel = () => {
  const [range, setRange] = useState<PerfRange>("7d");
  const [grouped, setGrouped] = useState(false);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<{ key: PerfSortKey; desc: boolean }>({ key: "tpsP50", desc: true });
  const [selected, setSelected] = useState<string[]>([]);

  const perf = useQuery({ queryKey: ["perf", range, grouped], queryFn: () => perfByModel(range, grouped) });
  const rows = perf.data ?? [];

  useEffect(() => {
    if (selected.length > 0 || rows.length === 0) return;
    const top3 = [...rows].sort((a, b) => b.samples - a.samples).slice(0, 3);
    setSelected(top3.map((r) => r.key));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows]);

  const onSort = (key: PerfSortKey) =>
    setSort((prev) => (prev.key === key ? { key, desc: !prev.desc } : { key, desc: true }));

  const onSelect = (key: string) =>
    setSelected((prev) => {
      if (prev.includes(key)) return prev.filter((k) => k !== key);
      const next = [...prev, key];
      return next.length > MAX_SELECTED ? next.slice(next.length - MAX_SELECTED) : next;
    });

  const filtered = filterPerf(rows, query);
  const sorted = sortPerf(filtered, sort.key, sort.desc);

  return (
    <div className="flex flex-col gap-5">
      <div className="flex items-center gap-2.5">
        <RangeFilter
          value={DAYS_BY_RANGE[range]}
          onChange={(days) => setRange(RANGE_BY_DAYS[days] ?? "7d")}
          options={[1, 7, 30]}
          labels={{ 1: "24 jam", 7: "7 hari", 30: "30 hari" }}
        />
        <button
          type="button"
          onClick={() => setGrouped((v) => !v)}
          aria-pressed={grouped}
          className={`ad-interactive ad-press cursor-pointer rounded-lg border border-border px-3 py-1.75 text-xs font-medium ${grouped ? "bg-surface-2 text-fg" : "text-fg-2 hover:border-fg-3 hover:text-fg"}`}
        >
          Kelompokkan per model
        </button>
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Cari model…"
          className="rounded-md border border-border bg-bg px-3 py-2 text-[13px]"
        />
      </div>

      {perf.isError && (
        <div className="rounded-md border border-err/40 bg-err/10 px-3 py-2 font-mono text-xs text-err">
          {String(perf.error)}
        </div>
      )}

      {perf.isPending && <div className="flex h-64 items-center justify-center text-sm text-fg-3">Memuat data…</div>}

      {!perf.isPending && !perf.isError && rows.length === 0 && (
        <div className="rounded-[10px] border border-dashed border-border p-8 text-center text-sm text-fg-3">
          Belum ada data performa. Data terisi saat indexer membaca riwayat sesi.
        </div>
      )}

      {!perf.isPending && !perf.isError && rows.length > 0 && (
        <>
          <div className="rounded-[10px] border border-border bg-surface px-5 py-4">
            <PerfTable rows={sorted} sort={sort} onSort={onSort} selected={selected} onSelect={onSelect} />
          </div>
          <div className="rounded-[10px] border border-border bg-surface px-5 py-4">
            <PerfChart rows={rows} keys={selected} />
          </div>
        </>
      )}
    </div>
  );
};
