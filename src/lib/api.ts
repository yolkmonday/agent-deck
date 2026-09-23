import { invoke, Channel } from "@tauri-apps/api/core";
import type { Agent, BillingMode, TokenUsage } from "@/lib/types";

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

export interface BillingAccount {
  id: string;
  label: string;
  mode: BillingMode;
  matches: string[];
  monthlyUsd: number | null;
  renewalDay: number | null;
  creditUsd: number | null;
  startedOn: string | null;
  expiresOn: string | null;
}

export interface AccountPeriod {
  accountId: string;
  label: string;
  mode: BillingMode;
  fromDate: string;
  toDate: string;
  tokens: TokenUsage;
  spendUsd: number;
  notionalUsd: number;
  committedUsd: number | null;
  creditLeftUsd: number | null;
  daysLeft: number | null;
  expired: boolean;
}

export interface BillingSummary {
  periods: AccountPeriod[];
  totalSpendUsd: number;
  totalNotionalUsd: number;
  warnings: string[];
}

export interface TermProfile {
  id: string;
  label: string;
  program: string;
  args: string[];
  available: boolean;
}

export interface TermSession {
  id: string;
  profileId: string;
  label: string;
  cwd: string;
  command: string;
  startedAtMs: number;
  alive: boolean;
}

export interface TermDataEvent {
  id: string;
  chunk: string;
}

export interface TermExitEvent {
  id: string;
  code: number | null;
}

export type RecoverAction = "nudge" | "kill" | "restart";

/** A refusal comes back as `ok: false` with a readable message, not as a thrown
 *  error, so "pid sudah bukan proses agent" reads as information. */
export interface RecoverResult {
  ok: boolean;
  action: RecoverAction;
  message: string;
  newTermId: string | null;
}

export interface OcModel {
  id: string;
  name: string | null;
  contextLimit: number | null;
  outputLimit: number | null;
}

export type OcAuth = "config" | "cli" | "none";
export type HeaderStyle = "bearer" | "custom";

export interface OcProvider {
  id: string;
  name: string;
  npm: string;
  baseUrl: string;
  auth: OcAuth;
  headerStyle: HeaderStyle;
  customHeaderName: string | null;
  enabled: boolean;
  keyMasked: string | null;
  keyInline: boolean;
  source: "global" | "project";
  models: OcModel[];
}

export interface ModelsOverview {
  claude: { models: string[]; recentlyUsed: string[] };
  opencode: OcProvider[];
  codex: { models: string[]; note: string };
}

export interface OcProviderInput {
  id: string;
  name: string;
  npm: string;
  baseUrl: string;
  headerStyle: HeaderStyle;
  customHeaderName: string | null;
  enabled: boolean;
  models: OcModel[];
}

export interface ModelTestResult {
  ok: boolean;
  status: number | null;
  latencyMs: number;
  reply: string | null;
  error: string | null;
  usedHeader: HeaderStyle;
}

export interface ConfigBackup {
  path: string;
  atMs: number;
}

export interface Project {
  id: string;
  name: string;
  path: string;
  defaultProfile: string | null;
  color: string | null;
  sortOrder: number;
  lastUsedMs: number | null;
  exists: boolean;
}

export interface ProjectInput {
  name: string;
  path: string;
  defaultProfile: string | null;
  color: string | null;
  sortOrder: number;
}

export interface ProjectSuggestion {
  name: string;
  path: string;
  source: "live" | "history";
  messages: number;
}

export type TailEntry =
  | { kind: "user"; ms: number; text: string }
  | { kind: "assistant"; ms: number; text: string; model: string | null }
  | { kind: "thinking"; ms: number; text: string }
  | { kind: "tool"; ms: number; name: string; input: string; status: "running" | "ok" | "error" }
  | { kind: "result"; ms: number; toolName: string; preview: string; isError: boolean };

export interface TranscriptTail {
  entries: TailEntry[];
  fileSize: number;
  found: boolean;
}

export type AttentionMode = "off" | "notify" | "auto";

export const DEFAULT_SETTINGS: Settings = {
  attentionMode: "notify",
  notifySound: false,
  stallMinutes: 5,
  slowToolMinutes: 10,
};

export interface Settings {
  attentionMode: AttentionMode;
  notifySound: boolean;
  stallMinutes: number;
  slowToolMinutes: number;
}

const modes: AttentionMode[] = ["off", "notify", "auto"];

const minutes = (value: unknown, fallback: number): number =>
  typeof value === "number" && Number.isFinite(value) && value >= 1 ? Math.floor(value) : fallback;

export const normalizeSettings = (s: Partial<Settings> | null | undefined): Settings => ({
  attentionMode: modes.includes(s?.attentionMode as AttentionMode)
    ? (s?.attentionMode as AttentionMode)
    : DEFAULT_SETTINGS.attentionMode,
  notifySound: typeof s?.notifySound === "boolean" ? s.notifySound : DEFAULT_SETTINGS.notifySound,
  stallMinutes: minutes(s?.stallMinutes, DEFAULT_SETTINGS.stallMinutes),
  slowToolMinutes: minutes(s?.slowToolMinutes, DEFAULT_SETTINGS.slowToolMinutes),
});

export const settingsGet = () => invoke<Settings>("settings_get").then(normalizeSettings);
export const settingsSet = (settings: Settings) =>
  invoke<Settings>("settings_set", { settings }).then(normalizeSettings);
export const windowFocused = () => invoke<boolean>("window_focused");

export const projectsList = () => invoke<Project[]>("projects_list");
export const projectCreate = (input: ProjectInput) => invoke<Project>("project_create", { input });
export const projectUpdate = (id: string, input: ProjectInput) =>
  invoke<Project>("project_update", { id, input });
export const projectDelete = (id: string) => invoke<null>("project_delete", { id });
export const projectTouch = (id: string) => invoke<Project>("project_touch", { id });
export const projectSuggestions = () => invoke<ProjectSuggestion[]>("project_suggestions");

export const indexStatus = () => invoke<IndexStatus>("index_status");
export const reindex = () => invoke<IndexStatus>("reindex");
export const historyDaily = (days: number) => invoke<DailyRow[]>("history_daily", { days });
export const historyByModel = (days: number) => invoke<ModelRow[]>("history_by_model", { days });
export const historyByProject = (days: number) => invoke<ProjectRow[]>("history_by_project", { days });
export const historyTotals = (days: number) => invoke<Totals>("history_totals", { days });
export const pricingGet = () => invoke<PriceEntry[]>("pricing_get");
export const pricingSet = (entries: PriceEntry[]) => invoke<PriceEntry[]>("pricing_set", { entries });
export const billingAccounts = () => invoke<BillingAccount[]>("billing_accounts");
export const billingSave = (accounts: BillingAccount[]) =>
  invoke<BillingAccount[]>("billing_save", { accounts });
export const billingSummary = () => invoke<BillingSummary>("billing_summary", {});
export const timelineSpans = (fromMs: number, toMs: number) =>
  invoke<TimelineLane[]>("timeline_spans", { fromMs, toMs });
export const savingsSummary = (days: number) => invoke<SavingsSummary>("savings_summary", { days });

export const sessionTail = (sessionId: string, cwd: string, agentId?: string | null) =>
  invoke<TranscriptTail>("session_tail", { sessionId, cwd, agentId: agentId ?? null });

export const termProfiles = () => invoke<TermProfile[]>("term_profiles");
export const termStart = (profileId: string, cwd: string, cols: number, rows: number) =>
  invoke<TermSession>("term_start", { profileId, cwd, cols, rows });
export const termList = () => invoke<TermSession[]>("term_list");
export const termWrite = (id: string, data: string) => invoke<null>("term_write", { id, data });
export const termResize = (id: string, cols: number, rows: number) =>
  invoke<null>("term_resize", { id, cols, rows });
export const termKill = (id: string) => invoke<null>("term_kill", { id });
// Streams the session's scrollback + live output as raw bytes over an IPC
// channel: the backend sends the full scrollback first, then registers the
// channel for subsequent output, atomically (see TerminalRegistry::attach).
export const termAttach = (id: string, channel: Channel<ArrayBuffer>) =>
  invoke<null>("term_attach", { id, channel });

export const recoverNudge = (sessionId: string) =>
  invoke<RecoverResult>("recover_nudge", { sessionId });
export const recoverKill = (pid: number, cwd: string) =>
  invoke<RecoverResult>("recover_kill", { pid, cwd });
export const recoverRestart = (pid: number, cwd: string, profileId: string) =>
  invoke<RecoverResult>("recover_restart", { pid, cwd, profileId });

export const modelsOverview = () => invoke<ModelsOverview>("models_overview");
export const providerSave = (provider: OcProviderInput) =>
  invoke<OcProvider>("provider_save", { provider });
export const providerDelete = (id: string) => invoke<null>("provider_delete", { id });
export const secretSet = (providerId: string, key: string) =>
  invoke<string>("secret_set", { providerId, key });
export const secretClear = (providerId: string) => invoke<null>("secret_clear", { providerId });
export const secretReveal = (providerId: string) => invoke<string>("secret_reveal", { providerId });
export const secretMigrateInline = (providerId: string) =>
  invoke<string>("secret_migrate_inline", { providerId });
export const modelsFetch = (providerId: string) => invoke<string[]>("models_fetch", { providerId });
export const modelTest = (providerId: string, model: string) =>
  invoke<ModelTestResult>("model_test", { providerId, model });
export const configBackups = () => invoke<ConfigBackup[]>("config_backups");
export const configRestore = (path: string) => invoke<null>("config_restore", { path });
