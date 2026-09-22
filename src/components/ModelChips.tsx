import { CircleCheck, History } from "lucide-react";
import { ModelIcon } from "@/components/BrandIcon";

interface ModelChipsProps {
  available: string[];
  recentlyUsed: string[];
}

export const ModelChips = ({ available, recentlyUsed }: ModelChipsProps) => {
  const used = new Set(recentlyUsed);
  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-2">
        <span className="flex items-center gap-1.5 text-[11.5px] font-medium text-fg-3">
          <CircleCheck size={13} className="text-ok" />
          Tersedia
        </span>
        <div className="flex flex-wrap gap-1.5">
          {available.map((m) => (
            <span
              key={m}
              className="flex items-center gap-1.5 rounded-md border border-border bg-surface px-2.5 py-1 font-mono text-[12px] text-fg"
            >
              <ModelIcon model={m} size={13} />
              {m}
            </span>
          ))}
        </div>
      </div>
      <div className="flex flex-col gap-2">
        <span className="flex items-center gap-1.5 text-[11.5px] font-medium text-fg-3">
          <History size={13} />
          Pernah dipakai
        </span>
        {recentlyUsed.length === 0 ? (
          <span className="text-xs text-fg-3">Belum ada riwayat pemakaian.</span>
        ) : (
          <div className="flex flex-wrap gap-1.5">
            {recentlyUsed.map((m) => (
              <span
                key={m}
                className={`flex items-center gap-1.5 rounded-md border px-2.5 py-1 font-mono text-[12px] ${
                  used.has(m) && available.includes(m)
                    ? "border-border bg-surface-2 text-fg-2"
                    : "border-border/60 text-fg-3"
                }`}
              >
                <ModelIcon model={m} size={13} />
                {m}
              </span>
            ))}
          </div>
        )}
      </div>
    </div>
  );
};
