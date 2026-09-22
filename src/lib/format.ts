import type { TokenUsage } from "@/lib/types";

const oneDecimalComma = (n: number) => n.toFixed(1).replace(".", ",");

export const formatTokens = (n: number): string => {
  if (n >= 1_000_000) return `${oneDecimalComma(n / 1_000_000)} jt`;
  if (n >= 1_000) return `${oneDecimalComma(n / 1_000)} rb`;
  return String(n);
};

export const formatDuration = (ms: number): string => {
  const minutes = Math.floor(ms / 60_000);
  if (minutes < 1) return "<1 mnt";
  if (minutes < 60) return `${minutes} mnt`;
  return `${Math.floor(minutes / 60)} j ${minutes % 60} mnt`;
};

export const totalTokens = (t: TokenUsage): number => t.input + t.output + t.cacheRead + t.cacheWrite;

/** Seconds below a minute, then the coarser `formatDuration`. */
export const formatShort = (ms: number): string =>
  ms < 60_000 ? `${Math.max(0, Math.floor(ms / 1000))} dtk` : formatDuration(ms);
