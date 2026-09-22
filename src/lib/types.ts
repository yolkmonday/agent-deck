export type Agent = "claude" | "opencode" | "codex";
export type Status = "busy" | "waiting" | "idle";
export type ActivityKind = "tool" | "waiting" | "thinking" | "done";

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

export interface Session {
  id: string;
  agent: Agent;
  pid: number | null;
  project: string;
  cwd: string;
  model: string | null;
  branch: string | null;
  status: Status;
  activity: Activity | null;
  tokens: TokenUsage;
  startedAtMs: number | null;
  updatedAtMs: number;
}

export interface LiveSnapshot {
  sessions: Session[];
  warnings: string[];
  generatedAtMs: number;
}

export interface FeedEvent {
  id: string;
  timeMs: number;
  project: string;
  agent: Agent;
  color: "waiting" | "busy" | "ok" | "idle";
  text: string;
}
