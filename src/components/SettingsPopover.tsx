import { useEffect, useRef } from "react";
import { UpdateSection } from "@/components/UpdateSection";
import type { AttentionMode } from "@/lib/api";
import { type Lang, type MessageKey, useLang, useT } from "@/i18n";

const OPTIONS: { value: AttentionMode; labelKey: MessageKey }[] = [
  { value: "off", labelKey: "settings.attention.off" },
  { value: "notify", labelKey: "settings.attention.notify" },
  { value: "auto", labelKey: "settings.attention.auto" },
];

const LANGS: { value: Lang; label: string }[] = [
  { value: "en", label: "English" },
  { value: "id", label: "Bahasa Indonesia" },
];

export const SettingsPopover = ({
  mode,
  notifySound,
  stallMinutes,
  slowToolMinutes,
  onMode,
  onNotifySound,
  onMinutes,
  onClose,
}: {
  mode: AttentionMode;
  notifySound: boolean;
  stallMinutes: number;
  slowToolMinutes: number;
  onMode: (mode: AttentionMode) => void;
  onNotifySound: (enabled: boolean) => void;
  onMinutes: (value: { stallMinutes: number; slowToolMinutes: number }) => void;
  onClose: () => void;
}) => {
  const ref = useRef<HTMLDivElement | null>(null);
  const t = useT();
  const lang = useLang((s) => s.lang);
  const setLang = useLang((s) => s.setLang);

  // The backend rejects anything below 1, so a half-typed value must not reach it.
  const atLeastOne = (n: number, fallback: number) => (Number.isFinite(n) && n >= 1 ? Math.floor(n) : fallback);

  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  return (
    <div
      ref={ref}
      className="ad-rise absolute top-full right-0 z-50 mt-2 flex w-72 flex-col gap-2 rounded-[10px] border border-border bg-surface p-3.5 shadow-lg"
    >
      <span className="text-[12px] font-semibold">{t("settings.attention.title")}</span>
      {OPTIONS.map(({ value, labelKey }) => (
        <label
          key={value}
          className="ad-interactive flex cursor-pointer items-center gap-2.5 rounded-md px-1.5 py-1.25 text-[13px] text-fg-2 hover:bg-surface-2"
        >
          <input
            type="radio"
            name="attention-mode"
            checked={mode === value}
            onChange={() => onMode(value)}
            className="accent-waiting"
          />
          {t(labelKey)}
        </label>
      ))}
      <label className="ad-interactive flex cursor-pointer items-center gap-2.5 rounded-md px-1.5 py-1.25 text-[13px] text-fg-2 hover:bg-surface-2">
        <input
          type="checkbox"
          checked={notifySound}
          onChange={(e) => onNotifySound(e.target.checked)}
          className="accent-waiting"
        />
        {t("settings.notifySound")}
      </label>
      <label className="flex items-center justify-between gap-2.5 px-1.5 py-1.25 text-[13px] text-fg-2">
        {t("settings.stallAfter")}
        <input
          type="number"
          min={1}
          value={stallMinutes}
          onChange={(e) =>
            onMinutes({ stallMinutes: atLeastOne(Number(e.target.value), stallMinutes), slowToolMinutes })
          }
          className="w-14 rounded-md border border-border bg-bg px-2 py-1 text-right font-mono text-[12.5px] text-fg"
        />
      </label>
      <label className="flex items-center justify-between gap-2.5 px-1.5 py-1.25 text-[13px] text-fg-2">
        {t("settings.slowToolAfter")}
        <input
          type="number"
          min={1}
          value={slowToolMinutes}
          onChange={(e) =>
            onMinutes({ stallMinutes, slowToolMinutes: atLeastOne(Number(e.target.value), slowToolMinutes) })
          }
          className="w-14 rounded-md border border-border bg-bg px-2 py-1 text-right font-mono text-[12.5px] text-fg"
        />
      </label>
      <UpdateSection />
      <div className="flex flex-col gap-1.5 border-t border-border pt-2">
        <span className="text-[12px] font-semibold">{t("settings.language")}</span>
        <div className="flex gap-1 rounded-md bg-surface-2 p-0.5">
          {LANGS.map(({ value, label }) => (
            <button
              key={value}
              type="button"
              aria-pressed={lang === value}
              onClick={() => setLang(value)}
              className={`ad-interactive flex-1 rounded-[5px] px-2 py-1 text-[12.5px] ${
                lang === value ? "bg-surface text-fg shadow-sm" : "text-fg-3 hover:text-fg-2"
              }`}
            >
              {label}
            </button>
          ))}
        </div>
      </div>
      <span className="border-t border-border pt-2 text-[11.5px] leading-[1.45] text-fg-3">{t("settings.hint")}</span>
    </div>
  );
};
