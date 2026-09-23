export const PageTabs = ({
  tabs,
  value,
  onChange,
}: {
  tabs: { id: string; label: string }[];
  value: string;
  onChange: (id: string) => void;
}) => (
  <div role="tablist" className="flex items-center gap-1 rounded-lg border border-border bg-surface p-0.75">
    {tabs.map((tab) => {
      const active = tab.id === value;
      return (
        <button
          key={tab.id}
          type="button"
          role="tab"
          aria-selected={active}
          onClick={() => onChange(tab.id)}
          className={`ad-interactive ad-press cursor-pointer rounded-md px-2.75 py-1.5 text-xs font-medium ${active ? "bg-surface-2 text-fg" : "text-fg-3 hover:text-fg-2"}`}
        >
          {tab.label}
        </button>
      );
    })}
  </div>
);
