export type Agent = "claude" | "opencode" | "codex";
export type Status = "busy" | "waiting" | "idle";
export type ActivityKind = "tool" | "waiting" | "thinking" | "done";
export type Health = "ok" | "slow" | "stalled";
export type BillingMode = "subscription" | "prepaid" | "payg";

export interface TokenUsage {
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
  reasoning: number;
}

export interface Activity {
  kind: ActivityKind;
  label: string;
  detail: string | null;
}

export interface SubAgent {
  id: string;
  agentType: string;
  description: string;
  model: string | null;
  tokens: TokenUsage;
  costUsd: number;
  priced: boolean;
  startedMs: number | null;
}

export interface Session {
  id: string;
  agent: Agent;
  pid: number | null;
  project: string;
  cwd: string;
  group: string;
  groupRoot: string;
  isWorktree: boolean;
  worktreeName: string | null;
  /** What the session is for: opencode's title, or claude's ai-title falling
   *  back to its first real prompt. `null` when neither exists yet. */
  task: string | null;
  model: string | null;
  branch: string | null;
  status: Status;
  activity: Activity | null;
  tokens: TokenUsage;
  ownTokens: TokenUsage;
  subagents: SubAgent[];
  costUsd: number;
  /** How the model behind `costUsd` is paid for. `costUsd` is always the
   *  API-rate figure; only a subscription makes it notional rather than spend. */
  billingMode: BillingMode;
  priced: boolean;
  startedAtMs: number | null;
  updatedAtMs: number;
  quietMs: number;
  toolRunningMs: number | null;
  health: Health;
  healthReason: string | null;
}

export interface Orphan {
  agent: Agent;
  pid: number;
  cwd: string;
  ageMs: number;
}

export interface LiveSnapshot {
  sessions: Session[];
  warnings: string[];
  generatedAtMs: number;
  costUsd: number;
  unpriced: number;
  orphans: Orphan[];
}

export interface FeedEvent {
  id: string;
  timeMs: number;
  project: string;
  agent: Agent;
  color: "waiting" | "busy" | "ok" | "idle";
  text: string;
}
