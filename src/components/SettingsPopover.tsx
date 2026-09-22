import { useEffect, useRef } from "react";
import type { AttentionMode } from "@/lib/api";

const OPTIONS: { value: AttentionMode; label: string }[] = [
  { value: "off", label: "Diam" },
  { value: "notify", label: "Beri tahu" },
  { value: "auto", label: "Langsung buka" },
];

export const SettingsPopover = ({
  mode,
  notifySound,
  onMode,
  onNotifySound,
  onClose,
}: {
  mode: AttentionMode;
  notifySound: boolean;
  onMode: (mode: AttentionMode) => void;
  onNotifySound: (enabled: boolean) => void;
  onClose: () => void;
}) => {
  const ref = useRef<HTMLDivElement | null>(null);

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
      className="absolute top-full right-0 z-50 mt-2 flex w-72 flex-col gap-2 rounded-[10px] border border-border bg-surface p-3.5 shadow-lg"
    >
      <span className="text-[12px] font-semibold">Saat ada agent bertanya</span>
      {OPTIONS.map(({ value, label }) => (
        <label
          key={value}
          className="flex cursor-pointer items-center gap-2.5 rounded-md px-1.5 py-1.25 text-[13px] text-fg-2"
        >
          <input
            type="radio"
            name="attention-mode"
            checked={mode === value}
            onChange={() => onMode(value)}
            className="accent-waiting"
          />
          {label}
        </label>
      ))}
      <label className="flex cursor-pointer items-center gap-2.5 rounded-md px-1.5 py-1.25 text-[13px] text-fg-2">
        <input
          type="checkbox"
          checked={notifySound}
          onChange={(e) => onNotifySound(e.target.checked)}
          className="accent-waiting"
        />
        Suara notifikasi
      </label>
      <span className="border-t border-border pt-2 text-[11.5px] leading-[1.45] text-fg-3">
        Langsung buka akan memindahkan layar sendiri saat ada agent yang bertanya.
      </span>
    </div>
  );
};
