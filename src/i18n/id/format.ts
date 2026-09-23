import type { format as en } from "@/i18n/en/format";

export const format: Record<keyof typeof en, string> = {
  "format.million": "{n} jt",
  "format.thousand": "{n} rb",
  "format.lessThanMinute": "<1 mnt",
  "format.minutes": "{m} mnt",
  "format.hoursMinutes": "{h} j {m} mnt",
  "format.seconds": "{s} dtk",
  "format.estimateHint": "Perkiraan dari timestamp sesi, termasuk waktu tunggu token pertama.",
};
