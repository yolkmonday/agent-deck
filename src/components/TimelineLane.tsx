import { AgentIcon, ModelIcon } from "@/components/BrandIcon";
import type { TimelineLane as Lane, TimelineSpan } from "@/lib/api";
import { layoutSpan } from "@/lib/timeline";

const agentText = { claude: "text-claude", opencode: "text-opencode", codex: "text-codex" } as const;
const agentName = { claude: "Claude", opencode: "opencode", codex: "Codex" } as const;

const spanClass: Record<TimelineSpan["status"], string> = {
  ok: "ad-interactive border-busy/60 bg-busy/25 hover:bg-busy/40",
  error: "ad-interactive border-err/60 bg-err/25 hover:bg-err/40",
  running: "ad-interactive border-waiting/60 bg-waiting/25 hover:bg-waiting/40",
};

export const TimelineLane = ({
  lane,
  fromMs,
  toMs,
  nowMs,
  selectedId,
  onSelect,
}: {
  lane: Lane;
  fromMs: number;
  toMs: number;
  nowMs: number;
  selectedId: string | null;
  onSelect: (span: TimelineSpan) => void;
}) => (
  <div className="flex min-w-0 items-stretch border-b border-border/60 last:border-0">
    <div className="w-[180px] shrink-0 pr-4">
      <div className="flex flex-col gap-0.75 py-2.5">
        <span className="flex items-center gap-1.5 truncate text-[12.5px] font-semibold">
          <AgentIcon agent={lane.agent} size={14} className={agentText[lane.agent]} />
          <span className="truncate">{lane.project}</span>
        </span>
        <span className="truncate font-mono text-[11px] text-fg-3">
          <span className="flex items-center gap-1.5">
            <ModelIcon model={lane.model} size={13} />
            <span className="truncate">{lane.model ?? agentName[lane.agent]}</span>
          </span>
        </span>
      </div>
    </div>
    <div className="relative min-h-11.5 min-w-0 flex-1">
      {lane.spans.map((s) => {
        const { leftPct, widthPct } = layoutSpan(s, fromMs, toMs, nowMs);
        const selected = s.id === selectedId;
        return (
          <button
            key={s.id}
            type="button"
            title={s.detail ? `${s.tool}: ${s.detail}` : s.tool}
            onClick={() => onSelect(s)}
            style={{ left: `${leftPct}%`, width: `${widthPct}%` }}
            className={`ad-press absolute top-2 bottom-2 cursor-pointer overflow-hidden rounded-[3px] border text-left ${spanClass[s.status]} ${
              selected ? "ring-1 ring-fg/70" : ""
            }`}
          />
        );
      })}
    </div>
  </div>
);
