const OPTIONS = [
  { days: 1, label: "Hari ini" },
  { days: 7, label: "7 hari" },
  { days: 14, label: "14 hari" },
  { days: 30, label: "30 hari" },
];

export const RangeFilter = ({
  value,
  onChange,
  options = [1, 7, 14, 30],
  labels,
}: {
  value: number;
  onChange: (days: number) => void;
  options?: number[];
  labels?: Record<number, string>;
}) => (
  <div className="flex items-center gap-1 rounded-lg border border-border bg-surface p-0.75">
    {options.map((days) => {
      const active = days === value;
      const label = labels?.[days] ?? OPTIONS.find((o) => o.days === days)?.label ?? `${days} hari`;
      return (
        <button
          key={days}
          type="button"
          onClick={() => onChange(days)}
          className={`ad-interactive ad-press cursor-pointer rounded-md px-2.75 py-1.5 text-xs font-medium ${active ? "bg-surface-2 text-fg" : "text-fg-3 hover:text-fg-2"}`}
        >
          {label}
        </button>
      );
    })}
  </div>
);
