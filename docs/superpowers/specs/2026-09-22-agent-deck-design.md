# Agent Deck: design spec

Date: 2026-09-22. Status: draft for review.

## 1. Goal

A local Tauri desktop dashboard that shows, in one place, what every coding agent on this Mac is doing right now and how many tokens it costs. It replaces opening 3-5 terminals one by one. It also manages opencode providers, models and keys, and hosts new agent terminals.

Agents covered: Claude Code, opencode, Codex CLI. Extra data: rtk and lean-ctx token savings.

### Non-goals (v1)

- Taking over sessions already running in other terminals (they are monitored only).
- Cloud sync, multi-user, or remote access.
- Writing to any agent's own data (all agent data is read-only).
- Managing Claude Code or Codex settings.

## 2. Screens

Mockups are in `docs/design/` (dark theme, UI text in Indonesian). Source canvas: pen.dev frames 01-07.

| # | Screen | File |
|---|---|---|
| 01 | Live Board: session cards (agent, model, status busy/waiting/idle, current activity, tokens, cost), KPI row, activity feed | `01-live-board.png` |
| 02 | Token & Biaya: per-day stacked bars by agent, per-model table | `02-token-biaya.png` |
| 03 | Timeline: per-session lanes of tool spans, detail panel | `03-timeline.png` |
| 04 | Hemat Token: rtk and lean-ctx savings per day, top commands | `04-hemat-token.png` |
| 05 | Terminal: embedded terminal tabs, session info, quick replies | `05-terminal.png` |
| 06 | Model & Provider: models per agent, opencode provider table, DeepSeek Flash 4.1 candidates | `06-model-provider.png` |
| 07 | Kelola Provider: provider form, auth header type, masked key, model list with per-model test | `07-kelola-provider.png` |

All numbers in the mockups are dummy data, except the three `cbai/*` test results on screen 07.

## 3. Architecture

One Tauri v2 process. Rust backend does all reading and aggregation; a React frontend renders. The collector is its own crate so it can become a daemon later without a rewrite.

```
agent-deck/
  src-tauri/
    crates/collector/   # lib: sources, pricing, store (no Tauri dependency)
    src/                # tauri app: commands, events, terminal, opencode_admin
  src/                  # React + Vite + TypeScript + Tailwind + shadcn
  docs/
```

Frontend: Zustand for the live snapshot, TanStack Query for history queries, xterm.js for terminals, recharts for charts, lucide-react for icons. Design tokens come from the mockups (`bg #0B0D10`, `surface #12151A`, status colors busy `#4C9AFF`, waiting `#F5A524`, idle `#69717D`, ok `#3DD68C`, agent colors claude `#E8825C`, opencode `#34D0E0`, codex `#A78BFA`; Inter and JetBrains Mono).

### 3.1 Collector sources (all read-only)

Facts below come from a probe on this machine (2026-09-22).

**Claude Code**
- Live state: `~/.claude/sessions/<pid>.json` has `pid`, `sessionId`, `cwd`, `status` (`busy|idle|waiting`), `updatedAt`, `waitingFor`, `name`. Files of dead pids are not cleaned up: validate every pid with `kill -0`. Do not read the `.key` files next to them.
- Transcripts: `~/.claude/projects/<encoded-cwd>/<sessionId>.jsonl`. The folder name encoding is lossy: take the real cwd from the `cwd` field in records.
- Token fields on `assistant` records: `message.model`, `message.usage.{input_tokens, output_tokens, cache_creation_input_tokens, cache_read_input_tokens, cache_creation.ephemeral_1h_input_tokens, cache_creation.ephemeral_5m_input_tokens, output_tokens_details.thinking_tokens}`, `timestamp`, `sessionId`, `cwd`, `gitBranch`, `message.id`, `requestId`.
- **Dedupe by `message.id`.** Each content block is written as its own record with the same usage; summing raw records overcounts about 2.5x.
- Current tool: last `assistant` `tool_use.name` without a following `tool_result`; a `system` record with `subtype: turn_duration` means the turn ended. Transcript can lag `sessions/*.json` by seconds.
- Subagents: `projects/<enc>/<parentSessionId>/subagents/agent-<id>.jsonl` plus `.meta.json` (`agentType`, `model`, `toolUseId`). Their usage is not in the parent file.
- Skip `~/.claude-mem/observer-sessions` (about 2 GB of noise) and processes started with `--output-format stream-json`.
- Do not scan the whole tree on each refresh (5 GB, 5000+ files): tail active files by byte offset.
- `stats-cache.json` is stale (last computed 2026-09-08): baseline only.

**opencode**
- SQLite `~/.local/share/opencode/opencode.db` (about 2.8 GB, WAL): open with `-readonly`. Tables: `session` (aggregated `tokens_*`, `cost`, `model` JSON, `directory`, `time_updated`), `message` (`data` JSON with `providerID`, `modelID`, `cost`, `tokens{input,output,reasoning,cache{read,write}}`, `time{created,completed}`), `part` (`data.type` = `tool|step-finish|...`, tool `state.status`), `project`, `todo`.
- `cost` is 0 for custom providers (`kn/*`, `oa/*`): compute cost from the pricing table.
- Running detection: `ps` for `opencode` processes, `lsof -a -d cwd -p <pid> -Fn` for the directory (or the `--dir` argument), matched to `session.directory`. About 18 old assistant messages have no `completed`, so the DB alone is not proof of "running".
- No per-process status file exists: status is coarser than Claude (active or not, last tool).
- The HTTP API (`/api/session/active`) is unverified; do not depend on it in v1.
- Never open `auth.json` from the collector.

**Codex CLI**
- `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`: `event_msg` with `token_count` (`info.total_token_usage`, `info.last_token_usage`, `rate_limits`), `turn_context` (`model`, `cwd`), `session_meta`.
- Index: `~/.codex/state_5.sqlite` table `threads`; open with `file:...?immutable=1` (plain `-readonly` fails).
- Not running at probe time; detect with `ps`.

**Savings**
- rtk: `rtk gain -f json` (add `-d` for daily); raw data in `~/Library/Application Support/rtk/history.db` table `commands`.
- lean-ctx: `~/.lean-ctx/stats.json` (`daily[]`), `events.jsonl` (`ToolCall` with `tokens_original`, `tokens_saved`), `cost_attribution.json`.

### 3.2 Data flow

- **Live loop (1 s):** read Claude session files, `ps`/`lsof` for opencode and Codex, tail active transcripts, read newest opencode rows. Diff against the previous snapshot and emit `live://snapshot` events.
- **History indexer (background):** first run builds `agent-deck.db` (dashboard-owned SQLite in the app data dir) with per-message usage, cached by file `mtime` + size; later runs are incremental. Queries (per day, model, project, agent) are served from this index.
- **Cost:** tokens times a user-editable pricing table (defaults for known models; custom opencode providers priced by the table).
- **Privacy:** the index stores tool names and a short label only. Conversation text and full command lines are never persisted.

### 3.3 Terminal (screen 05)

- New sessions only, via a PTY (`portable-pty`) streamed to xterm.js through Tauri events. Sessions owned by the dashboard can receive input (answering a `waiting` agent).
- External sessions are monitored only, with an "open in original terminal" action.
- Default: run the agent binary directly with explicit arguments and show the exact command in the UI. Shell aliases are not applied (the current `codex` alias adds `-s danger-full-access`). An opt-in toggle can use the login shell.
- Sessions die with the app in v1 (a tmux backend is out of scope).

### 3.4 opencode admin (screens 06, 07)

- Reads and edits `~/.config/opencode/opencode.jsonc` with a comment-preserving JSONC editor. Before each write: backup as `opencode.jsonc.bak-<timestamp>` (keep the last 10), write to a temp file, atomic rename, re-parse to validate.
- Providers: name, id, `npm` adapter (`@ai-sdk/openai-compatible`), `options.baseURL`, enabled state via `disabled_providers`, header style (`Authorization: Bearer` via `apiKey`, or a custom header such as `x-api-key` via `options.headers`).
- Models: add manual or fetch from `/v1/models`; fields `name`, `limit.context`, `limit.output`. Custom models default to a limit of 0, so the form must always set the limits.
- Keys: stored in `~/.config/opencode/secrets/<provider>.key` (mode 0600) and referenced from config as `{file:~/.config/opencode/secrets/<provider>.key}`. Documented opencode behavior: `{file:}` and `{env:}` are substituted in any string value, `~/` is expanded, relative paths resolve against the config directory, the file content is trimmed, and a missing file is an error (`bad file reference`), so the dashboard must validate the file exists before saving.
- One-time migration wizard for existing plaintext keys: preview diff, explicit confirmation.
- Key values reach the frontend only masked. "Reveal" is explicit and time-limited. Keys are never logged.
- Test connection: send a tiny chat completion per model with enough `max_tokens` for reasoning models (a 50-token cap truncated GLM 5.2). Report status, latency and error. Detect whether the gateway wants Bearer or `x-api-key` (the `aki` gateway returned 401 for Bearer and 200 for `x-api-key` during the probe).
- Config precedence: project `opencode.json` overrides global. Show where each setting comes from. Config changes need an opencode restart (no documented hot reload; unverified).
- The `opencode providers login/logout/list` CLI is used for OAuth credentials and for reading credential state.

### 3.5 Security

- Strict CSP; no shell access from the webview beyond named Tauri commands.
- Minimal Tauri capabilities (fs scoped to the needed paths).
- All agent data is opened read-only. The only writes are the dashboard's own index, `opencode.jsonc` (with backup) and the key files.
- The app never pushes, and never sends agent data or keys over the network except the explicit provider test call.

## 4. Phases

Each phase is usable on its own.

1. **P1** Scaffold, collector crate, Live Board (Claude and opencode).
2. **P2** Token & Biaya and Timeline, pricing table, history index.
3. **P3** Hemat Token (rtk, lean-ctx).
4. **P4** Terminal (PTY, tabs, answer a waiting agent).
5. **P5** Model & Provider and Kelola Provider (admin, keys, tests).

## 5. Testing

- Rust: `cargo test` with redacted fixtures for Claude jsonl (including the dedupe case: 241 records must collapse to 94 messages), Codex jsonl, and small opencode and rtk SQLite files. Pid liveness is tested with a fake process table.
- opencode admin: tests on temp config files that contain comments, a custom header, and `{file:}` references; check backup, atomic write, and round-trip validity.
- Frontend: `bun test` for formatting, aggregation and pricing helpers. Visual check against `docs/design/`.
- Manual: run against the real machine with 3+ live sessions.

## 6. Open items

- Real pricing values per model (user supplies or confirms the defaults).
- Whether opencode's HTTP API can replace `ps`/`lsof` polling later.
- Codex model list is read from sessions; not yet probed in detail.
- Bearer vs `x-api-key` behavior of `@ai-sdk/openai-compatible` when `apiKey` is omitted: test directly.
