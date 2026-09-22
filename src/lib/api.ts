import { invoke } from "@tauri-apps/api/core";
import type { Agent, TokenUsage } from "@/lib/types";

export interface IndexStatus {
  indexedFiles: number;
  messages: number;
  running: boolean;
  lastRunMs: number | null;
}

export interface DailyRow {
  date: string;
  agent: Agent;
  tokens: TokenUsage;
  costUsd: number;
}

export interface ModelRow {
  model: string;
  agent: Agent;
  tokens: TokenUsage;
  costUsd: number;
  messages: number;
}

export interface ProjectRow {
  project: string;
  tokens: TokenUsage;
  costUsd: number;
  messages: number;
}

export interface Totals {
  tokens: TokenUsage;
  costUsd: number;
  messages: number;
  cacheHitPct: number;
}

export interface PriceEntry {
  model: string;
  inputPerM: number;
  outputPerM: number;
  cacheReadPerM: number;
  cacheWritePerM: number;
}

export interface TimelineSpan {
  id: string;
  tool: string;
  detail: string | null;
  startMs: number;
  endMs: number | null;
  status: "ok" | "error" | "running";
  tokens: TokenUsage | null;
}

export interface TimelineLane {
  sessionId: string;
  agent: Agent;
  project: string;
  model: string | null;
  spans: TimelineSpan[];
}

export interface SavingsSource {
  available: boolean;
  savedTokens: number;
  totalTokens: number;
  savingsPct: number;
  entries: number;
}

export interface SavingsDay {
  date: string;
  rtkSaved: number;
  leanCtxSaved: number;
}

export interface SavingsCommand {
  command: string;
  savedTokens: number;
  savingsPct: number;
  runs: number;
}

export interface SavingsSummary {
  rtk: SavingsSource;
  leanCtx: SavingsSource;
  daily: SavingsDay[];
  topCommands: SavingsCommand[];
  warnings: string[];
}

export const indexStatus = () => invoke<IndexStatus>("index_status");
export const reindex = () => invoke<IndexStatus>("reindex");
export const historyDaily = (days: number) => invoke<DailyRow[]>("history_daily", { days });
export const historyByModel = (days: number) => invoke<ModelRow[]>("history_by_model", { days });
export const historyByProject = (days: number) => invoke<ProjectRow[]>("history_by_project", { days });
export const historyTotals = (days: number) => invoke<Totals>("history_totals", { days });
export const pricingGet = () => invoke<PriceEntry[]>("pricing_get");
export const pricingSet = (entries: PriceEntry[]) => invoke<PriceEntry[]>("pricing_set", { entries });
export const timelineSpans = (fromMs: number, toMs: number) =>
  invoke<TimelineLane[]>("timeline_spans", { fromMs, toMs });
export const savingsSummary = (days: number) => invoke<SavingsSummary>("savings_summary", { days });
