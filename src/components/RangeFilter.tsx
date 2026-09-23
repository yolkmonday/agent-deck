import { useT, type MessageKey } from "@/i18n";

const OPTIONS: { days: number; labelKey: MessageKey }[] = [
  { days: 1, labelKey: "rangeFilter.today" },
  { days: 7, labelKey: "rangeFilter.days7" },
  { days: 14, labelKey: "rangeFilter.days14" },
  { days: 30, labelKey: "rangeFilter.days30" },
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
}) => {
  const t = useT();
  return (
    <div className="flex items-center gap-1 rounded-lg border border-border bg-surface p-0.75">
      {options.map((days) => {
        const active = days === value;
        const knownKey = OPTIONS.find((o) => o.days === days)?.labelKey;
        const label = labels?.[days] ?? (knownKey ? t(knownKey) : t("rangeFilter.days", { days }));
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
};
