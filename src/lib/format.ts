import { locale, t } from "@/i18n";
import type { TokenUsage } from "@/lib/types";

const oneDecimal = (n: number) =>
  new Intl.NumberFormat(locale(), { minimumFractionDigits: 1, maximumFractionDigits: 1, useGrouping: false }).format(n);

export const formatTokens = (n: number): string => {
  if (n >= 1_000_000) return t("format.million", { n: oneDecimal(n / 1_000_000) });
  if (n >= 1_000) return t("format.thousand", { n: oneDecimal(n / 1_000) });
  return String(n);
};

export const formatDuration = (ms: number): string => {
  const minutes = Math.floor(ms / 60_000);
  if (minutes < 1) return t("format.lessThanMinute");
  if (minutes < 60) return t("format.minutes", { m: minutes });
  return t("format.hoursMinutes", { h: Math.floor(minutes / 60), m: minutes % 60 });
};

export const totalTokens = (usage: TokenUsage): number => usage.input + usage.output + usage.cacheRead + usage.cacheWrite;

/** Seconds below a minute, then the coarser `formatDuration`. */
export const formatShort = (ms: number): string =>
  ms < 60_000 ? t("format.seconds", { s: Math.max(0, Math.floor(ms / 1000)) }) : formatDuration(ms);
