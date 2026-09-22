# Agent Deck P4: Embedded Terminal

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Start new Claude Code, opencode or Codex sessions inside the dashboard, see their output, and type into them — so a `waiting` agent can be answered without switching to a terminal.

**Architecture:** A `terminal` module in the Tauri app owns a registry of PTYs (`portable-pty`). Each PTY gets a reader thread that emits `term://data/<id>` events. Commands create, write to, resize and kill sessions. The frontend renders one xterm.js instance per tab.

**Tech Stack:** Same as P3, plus `portable-pty = "0.9"` on the Rust side and `@xterm/xterm` + `@xterm/addon-fit` on the frontend.

**Spec:** `docs/superpowers/specs/2026-09-22-agent-deck-design.md` (section 3.3). Mockup: `docs/design/05-terminal.png`.

## Global Constraints

Same as P3, plus these, which are the point of this phase:

- **The exact command is shown to the user before it runs and is never hidden.** No shell alias is
  applied: the binary is executed directly with explicit arguments. The user's `codex` shell alias
  adds `-s danger-full-access`, and inheriting that silently would be wrong.
- Sessions are owned by the dashboard only. Sessions started in other terminals stay read-only.
- Killing a session is an explicit user action with a confirmation in the UI.
- Never pass secrets, `.env` contents or API keys into the spawned environment beyond what the
  user's own environment already has.
- No `Co-Authored-By`, never push, Bun only, `@/` alias, Indonesian UI text, English code.

## Command API (the contract)

```ts
// invoke("term_profiles") -> TermProfile[]
interface TermProfile { id: string; label: string; program: string; args: string[]; available: boolean }
// available = the program resolves on PATH.

// invoke("term_start", { profileId: string, cwd: string }) -> TermSession
interface TermSession { id: string; profileId: string; label: string; cwd: string; command: string; startedAtMs: number; alive: boolean }
// `command` is the full display string, e.g. "claude" or "opencode --auto".

// invoke("term_list") -> TermSession[]
// invoke("term_write", { id: string, data: string }) -> null
// invoke("term_resize", { id: string, cols: number, rows: number }) -> null
// invoke("term_kill", { id: string }) -> null
// invoke("term_scrollback", { id: string }) -> string   // everything emitted so far, for re-attach

// event "term://data"  payload { id: string; chunk: string }
// event "term://exit"  payload { id: string; code: number | null }
```

Profiles (fixed list in P4):

| id | label | program | args |
|---|---|---|---|
| `claude` | Claude Code | `claude` | none |
| `opencode` | opencode | `opencode` | none |
| `codex` | Codex | `codex` | none |

---

### Task 1: PTY registry

**Files:**
- Create: `src-tauri/src/terminal.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod terminal;`), `src-tauri/Cargo.toml` (add `portable-pty = "0.9"`)

**Interfaces:**
- `struct TermProfile { pub id: String, pub label: String, pub program: String, pub args: Vec<String>, pub available: bool }` (serde camelCase)
- `struct TermSession { pub id: String, pub profile_id: String, pub label: String, pub cwd: String, pub command: String, pub started_at_ms: i64, pub alive: bool }` (serde camelCase)
- `struct TerminalRegistry` with:
  - `fn new() -> TerminalRegistry`
  - `fn profiles() -> Vec<TermProfile>` (associated function; `available` via a PATH lookup helper `fn on_path(program: &str) -> bool` that checks each `PATH` entry for an executable file — do NOT shell out to `which`)
  - `fn start<F>(&mut self, profile_id: &str, cwd: &str, on_data: F, on_exit: ...) -> anyhow::Result<TermSession>` where the callbacks are `Send + 'static` closures invoked from the reader thread
  - `fn list(&self) -> Vec<TermSession>`
  - `fn write(&self, id: &str, data: &str) -> anyhow::Result<()>`
  - `fn resize(&self, id: &str, cols: u16, rows: u16) -> anyhow::Result<()>`
  - `fn kill(&mut self, id: &str) -> anyhow::Result<()>`
  - `fn scrollback(&self, id: &str) -> String`
- `start` rejects an unknown `profile_id`, a `cwd` that is not an existing directory, and a profile
  whose program is not on PATH, each with a distinct error message.
- Scrollback is kept per session in memory, capped at 256 KB (drop from the front, on a UTF-8
  character boundary).
- Session ids are `format!("t{}", counter)` with a monotonic counter, so they are stable and short.

- [ ] **Step 1: Write the failing tests.** Use `/bin/echo` and `/bin/cat` as stand-in programs by
  adding a test-only constructor `TerminalRegistry::with_profiles(Vec<TermProfile>)`:
1. `on_path_finds_a_real_binary_and_rejects_a_fake_one`: `on_path("sh")` is true, `on_path("definitely-not-a-real-binary-xyz")` is false.
2. `start_rejects_unknown_profile`: error message contains the profile id.
3. `start_rejects_missing_cwd`: a nonexistent directory errors.
4. `start_runs_and_streams_output`: a profile running `/bin/echo hello` produces a data callback containing `hello` within 5 seconds, then an exit callback.
5. `write_reaches_the_process`: a profile running `/bin/cat`, write `"ping\n"`, and a data callback containing `ping` arrives within 5 seconds.
6. `list_reflects_alive_then_dead`: after the echo process exits, `list()` shows `alive: false`.
7. `scrollback_is_capped`: feed more than 256 KB and assert the stored scrollback is at most 256 KB and still valid UTF-8.
8. `kill_terminates_a_long_running_process`: start `/bin/cat`, `kill`, and `list()` reports `alive: false` within 5 seconds.
  Use a polling helper with a deadline rather than a fixed sleep, so the tests are not flaky.
- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes** — 8 tests pass. Run the suite twice to check for flakiness.
- [ ] **Step 5: Commit** — `git commit -m "feat(app): add pty terminal registry"`

---

### Task 2: Terminal commands and events

**Files:** Modify `src-tauri/src/lib.rs`

**Interfaces:**
- The seven commands from the Command API, backed by `Mutex<TerminalRegistry>` in `AppState`.
- `term_start` wires the callbacks to `app.emit("term://data", ...)` and `app.emit("term://exit", ...)`.
- All commands return `Result<T, String>`.
- On app exit, every session is killed (use `tauri::RunEvent::ExitRequested` or a `Drop` on the
  registry) so no orphan agent process survives the window closing.

- [ ] **Step 1:** Implement.
- [ ] **Step 2:** Verify — `cargo check --manifest-path src-tauri/Cargo.toml --workspace` exits 0.
- [ ] **Step 3: Commit** — `git commit -m "feat(app): expose terminal commands and events"`

---

### Task 3: Terminal page

**Files:**
- Create: `src/pages/TerminalPage.tsx`, `src/components/TerminalTabs.tsx`, `src/components/TerminalView.tsx`, `src/components/NewSessionDialog.tsx`, `src/store/terminal.ts`
- Modify: `src/lib/api.ts`, `src/lib/nav.ts` (enable `terminal`), `src/App.tsx`, `package.json`

**Interfaces:**
- `bun add @xterm/xterm @xterm/addon-fit`, import `@xterm/xterm/css/xterm.css` in `src/index.css`
  or `main.tsx`.
- `src/store/terminal.ts` (zustand): `{ sessions: TermSession[]; activeId: string | null; refresh(): Promise<void>; start(profileId, cwd): Promise<void>; select(id): void; remove(id): void }`
  plus `startTerminalEvents(): Promise<() => void>` that listens to both events and forwards data to
  the right `TerminalView` through a module-level `Map<string, (chunk: string) => void>` registry.
- `TerminalView` props `{ session: TermSession }`: creates one `Terminal` with the FitAddon, dark
  theme matching `src/index.css` variables, writes `term_scrollback` on mount, subscribes for later
  chunks, sends `onData` to `term_write`, and calls `term_resize` on fit. It must dispose the
  terminal and unsubscribe on unmount.
- `NewSessionDialog`: pick a profile (disabled when `available: false`, with the note
  `Tidak ditemukan di PATH`) and a working directory. The directory field is prefilled from a
  dropdown of the projects currently seen in the live snapshot, and is also free text.
  **It shows the exact command that will run**, e.g. `claude` in `~/Dev/noor`.
- `TerminalTabs`: one tab per session with the agent colour dot, a live status dot, and a close
  button that asks `Hentikan sesi ini?` before calling `term_kill`.
- Empty state: `Belum ada sesi. Klik "Sesi baru" untuk mulai.`
- Include a visible line in the page footer: `Sesi di sini mati kalau app ditutup.`

- [ ] **Step 1:** Implement the store, components and page; enable the nav item.
- [ ] **Step 2:** Verify — `bunx tsc --noEmit`, `bun test`, `bun run build` all clean.
- [ ] **Step 3: Commit** — `git commit -m "feat(ui): add embedded terminal page"`

---

### Task 4: Answer a waiting session from the Live board

**Files:** Modify `src/components/SessionCard.tsx`, `src/pages/LivePage.tsx`

**Interfaces:**
- A `waiting` session card gets a `Jawab` button. Behaviour:
  - If a dashboard-owned terminal session already has the same `cwd`, switch to the Terminal page
    and select that tab.
  - Otherwise show a short note in the card: `Sesi ini jalan di terminal lain. Buka di sana.`
    Do NOT try to inject input into a process the dashboard does not own.
- Non-waiting cards get a `Terminal` button that starts a new dashboard session in that `cwd`
  after confirmation, since that is a new process, not an attach.
- `App.tsx` gains a `goToTerminal(sessionId?: string)` handler passed down for this.

- [ ] **Step 1:** Implement.
- [ ] **Step 2:** Verify — `bunx tsc --noEmit`, `bun test`, `bun run build` all clean.
- [ ] **Step 3: Commit** — `git commit -m "feat(ui): wire live board buttons to terminal"`

---

## Done when

- `cd src-tauri && cargo test 2>&1` passes, including the 8 new terminal tests, run twice with no flakes.
- `bunx tsc --noEmit`, `bun test`, `bun run build`, `cargo check --workspace` all clean.
- One commit per task, no `Co-Authored-By`, nothing pushed.

## Self-Review

- **Spec coverage:** new sessions via PTY (Tasks 1-3), answering a waiting agent (Task 4), external
  sessions stay monitor-only (Task 4 states it explicitly), the exact command is displayed
  (Task 3), sessions die with the app (Task 2 kills them on exit and Task 3 says so in the UI).
- **Deliberate gap:** the spec's optional "use the login shell" toggle is NOT implemented. Running
  the binary directly is the only mode in P4, because the alias would silently add
  `-s danger-full-access`. If the user wants the toggle later it is a separate task.
- **Type consistency:** `TermProfile` and `TermSession` are identical in the Command API section,
  Task 1's Rust structs and Task 3's TypeScript.
