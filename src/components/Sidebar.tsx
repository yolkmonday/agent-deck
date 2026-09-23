import { Radar, Settings } from "lucide-react";
import { useState } from "react";
import { SettingsPopover } from "@/components/SettingsPopover";
import { useT } from "@/i18n";
import type { AttentionMode, Settings as SettingsValue } from "@/lib/api";
import { NAV_ITEMS, type PageKey } from "@/lib/nav";

export const Sidebar = ({
  page,
  onSelect,
  onJumpWaiting,
  waitingCount,
  settings,
  onMode,
  onNotifySound,
  onMinutes,
}: {
  page: PageKey;
  onSelect: (page: PageKey) => void;
  onJumpWaiting: () => void;
  waitingCount: number;
  settings: SettingsValue;
  onMode: (mode: AttentionMode) => void;
  onNotifySound: (enabled: boolean) => void;
  onMinutes: (value: { stallMinutes: number; slowToolMinutes: number }) => void;
}) => {
  const t = useT();
  const [settingsOpen, setSettingsOpen] = useState(false);
  return (
    <aside className="flex w-58 shrink-0 flex-col border-r border-border">
      <div className="flex items-center gap-2.5 px-5 py-5.5">
        <Radar size={20} className="text-busy" />
        <span className="text-[15px] font-semibold">Agent Deck</span>
      </div>
      <nav className="flex flex-col gap-0.5 px-3 py-1">
        {NAV_ITEMS.map(({ key, labelKey, icon: Icon, enabled }) => {
          const active = key === page;
          return (
            <button
              key={key}
              type="button"
              disabled={!enabled}
              onClick={() => onSelect(key)}
              className={`ad-interactive ad-press flex items-center gap-2.5 rounded-lg px-3 py-2.25 text-left text-[13.5px] ${active ? "bg-surface-2 font-semibold text-fg" : "font-medium text-fg-3 hover:text-fg"} ${enabled ? "cursor-pointer" : "cursor-default opacity-45"}`}
            >
              <Icon size={16} />
              <span className="flex-1">{t(labelKey)}</span>
              {key === "live" && waitingCount > 0 && (
                <span
                  role="button"
                  tabIndex={0}
                  title={t("sidebar.openWaitingTitle")}
                  onClick={(e) => {
                    e.stopPropagation();
                    onJumpWaiting();
                  }}
                  onKeyDown={(e) => {
                    if (e.key !== "Enter" && e.key !== " ") return;
                    e.stopPropagation();
                    onJumpWaiting();
                  }}
                  className="ad-interactive ad-press cursor-pointer rounded-full bg-waiting px-1.75 py-0.5 text-[11px] font-semibold text-bg hover:opacity-90"
                >
                  {t("sidebar.waitingCount", { count: waitingCount })}
                </span>
              )}
            </button>
          );
        })}
      </nav>
      <div className="relative mt-auto px-3 py-3">
        <button
          type="button"
          onClick={() => setSettingsOpen((v) => !v)}
          aria-label={t("sidebar.settings")}
          className="ad-interactive ad-press flex w-full cursor-pointer items-center gap-2.5 rounded-lg px-3 py-2.25 text-left text-[13.5px] font-medium text-fg-3 hover:text-fg-2"
        >
          <Settings size={16} />
          <span className="flex-1">{t("sidebar.settings")}</span>
        </button>
        {settingsOpen && (
          <SettingsPopover
            placement="up"
            mode={settings.attentionMode}
            notifySound={settings.notifySound}
            stallMinutes={settings.stallMinutes}
            slowToolMinutes={settings.slowToolMinutes}
            onMode={onMode}
            onNotifySound={onNotifySound}
            onMinutes={onMinutes}
            onClose={() => setSettingsOpen(false)}
          />
        )}
      </div>
    </aside>
  );
};
