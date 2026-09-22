import { X } from "lucide-react";
import { AgentIcon } from "@/components/BrandIcon";
import type { TermSession } from "@/lib/api";
import { useTerminal } from "@/store/terminal";

const AGENT_TEXT = { claude: "text-claude", opencode: "text-opencode", codex: "text-codex" } as const;

export const TerminalTabs = ({ onClose }: { onClose: (id: string) => void }) => {
  const sessions = useTerminal((s) => s.sessions);
  const activeId = useTerminal((s) => s.activeId);
  const select = useTerminal((s) => s.select);
  return (
    <div className="flex shrink-0 items-center gap-1 overflow-x-auto border-b border-border px-3 py-2">
      {sessions.map((s: TermSession) => (
        <div
          key={s.id}
          className={`flex items-center gap-2 rounded-md border px-2.5 py-1.5 text-[12.5px] ${
            s.id === activeId ? "border-border bg-surface-2 font-semibold" : "border-transparent text-fg-2"
          }`}
        >
          <button
            type="button"
            onClick={() => select(s.id)}
            className="ad-interactive ad-press flex cursor-pointer items-center gap-2 hover:text-fg"
          >
            <AgentIcon
              agent={(s.profileId as keyof typeof AGENT_TEXT) in AGENT_TEXT ? (s.profileId as "claude" | "opencode" | "codex") : "claude"}
              size={14}
              className={AGENT_TEXT[s.profileId as keyof typeof AGENT_TEXT]}
            />
            <span className="max-w-40 truncate">{s.label}</span>
            <span className={`size-1.5 rounded-full ${s.alive ? "bg-ok" : "bg-idle"}`} />
          </button>
          <button
            type="button"
            onClick={() => onClose(s.id)}
            title="Tutup sesi"
            className="ad-interactive ad-press cursor-pointer text-fg-3 hover:text-err"
          >
            <X size={13} />
          </button>
        </div>
      ))}
    </div>
  );
};
