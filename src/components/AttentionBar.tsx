import { Settings } from "lucide-react";
import { useState } from "react";
import { SettingsPopover } from "@/components/SettingsPopover";
import type { AttentionMode, Settings as SettingsValue } from "@/lib/api";
import type { Session } from "@/lib/types";
import { waitingLabel } from "@/lib/attention";

export const AttentionBar = ({
  sessions,
  nowMs,
  settings,
  onJump,
  onMode,
  onNotifySound,
}: {
  sessions: Session[];
  nowMs: number;
  settings: SettingsValue;
  onJump: () => void;
  onMode: (mode: AttentionMode) => void;
  onNotifySound: (enabled: boolean) => void;
}) => {
  const [open, setOpen] = useState(false);
  const first = sessions[0];
  if (!first) return null;

  return (
    <div className="flex items-center gap-3 border-b border-waiting/40 bg-waiting/10 px-7 py-2.5">
      <span className="size-2 shrink-0 rounded-full bg-waiting" />
      <span className="truncate text-[13px] font-semibold text-waiting">{waitingLabel(first, nowMs)}</span>
      {sessions.length > 1 && <span className="shrink-0 text-[12.5px] text-fg-2">dan {sessions.length - 1} lainnya</span>}
      <button
        type="button"
        onClick={onJump}
        className="ml-auto shrink-0 cursor-pointer rounded-md bg-waiting px-3 py-1.25 text-[12px] font-semibold text-bg"
      >
        Buka
      </button>
      <div className="relative shrink-0">
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          title="Pengaturan notifikasi"
          className="cursor-pointer rounded-md border border-border p-1.5 text-fg-2"
        >
          <Settings size={14} />
        </button>
        {open && (
          <SettingsPopover
            mode={settings.attentionMode}
            notifySound={settings.notifySound}
            onMode={onMode}
            onNotifySound={onNotifySound}
            onClose={() => setOpen(false)}
          />
        )}
      </div>
    </div>
  );
};
