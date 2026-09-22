# Agent Deck P15: Nudge, restart and kill a stuck agent

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** When the heartbeat says a session is stuck, act on it from the dashboard instead of hunting for the terminal.

**Architecture:** A `recover` module owns the three actions. Every one of them re-verifies the target process immediately before signalling it, so a recycled pid can never be hit. The UI exposes them only on a session that is actually unhealthy, and every destructive action asks first.

**Depends on:** P4 (the PTY registry, for sessions the dashboard owns) and P12 (health).

## What is actually possible — and what is not

This has to be stated plainly, because the honest answer is narrower than "revive it":

| Action | Session the dashboard started | Session started in another terminal |
|---|---|---|
| **Nudge** (send a keystroke) | Yes — we own its stdin | **No.** We do not own its stdin and cannot type into it. |
| **Kill** | Yes | Yes, by pid — but it is someone else's process |
| **Restart** | Yes (kill + start fresh) | Kill + start fresh, in the same folder |

**A restart is not a resume.** The old conversation is gone; a new agent process starts in the same
directory. The UI must say this in the confirmation, every time. Anything softer would be a lie.

## Global Constraints

Same as previous phases. Plus these, which exist because this phase can destroy work in progress:

- **Every destructive action is confirmed by the user, per action.** No "don't ask again", no bulk
  kill, no auto-recovery. The heartbeat reports; the human decides.
- **Re-verify before signalling.** Between the snapshot and the click, a pid can die and be reused by
  an unrelated process. Immediately before any signal, read the pid's current command line and
  refuse unless it still matches a known agent binary (`claude`, `opencode`, `codex`). Refusing is
  the correct outcome, not an error to work around.
- **SIGTERM first, then SIGKILL.** Send SIGTERM, wait up to 5 s for the process to exit, and only
  then SIGKILL. An agent that gets a chance to clean up may flush its transcript.
- **Never signal pid 1, pid 0, a negative pid, or the dashboard's own pid.** Guard explicitly.
- No `Co-Authored-By`. Never push.

## Command API (the contract)

```ts
// invoke("recover_nudge", { sessionId: string }) -> RecoverResult
// invoke("recover_kill", { pid: number, cwd: string }) -> RecoverResult
// invoke("recover_restart", { pid: number, cwd: string, profileId: string }) -> RecoverResult
interface RecoverResult {
  ok: boolean;
  action: "nudge" | "kill" | "restart";
  message: string;      // Indonesian, shown as-is
  newTermId: string | null;  // set only by a successful restart
}
```

Failures come back as `ok: false` with a readable `message`, never as a thrown error, so a refusal
(for example "pid sudah bukan proses agent") reads as information rather than a crash.

---

### Task 1: Process verification and signalling

**Files:** Create `src-tauri/src/recover.rs`; modify `src-tauri/src/lib.rs` (add `mod recover;`)

**Interfaces:**
- `fn agent_kind(cmd: &str) -> Option<&'static str>` — returns `"claude"`, `"opencode"` or `"codex"`
  when the command's executable basename matches, else `None`. The GUI app `OpenCode` (capital O)
  must NOT match; P1 already relies on that distinction.
- `fn verify(pid: u32, procs: &dyn ProcessTable) -> Result<String, String>` — returns the matched
  agent kind, or an Indonesian error: `proses sudah tidak ada` when the pid is gone,
  `pid sudah bukan proses agent` when the command no longer matches.
- `fn terminate(pid: u32, procs: &dyn ProcessTable, wait_ms: u64) -> Result<bool, String>` — refuses
  pid `<= 1` and the current process id with `pid tidak boleh dimatikan`; sends SIGTERM, polls for
  exit up to `wait_ms`, then SIGKILL. Returns whether SIGKILL was needed.
- Signals go through `libc::kill`; add `libc = "0.2"`.

- [ ] **Step 1: Write the failing tests:**
1. `agent_kind_matches_the_three_binaries`: `/x/bin/claude`, `opencode run ...`, `/y/codex` all match; `/Applications/OpenCode.app/Contents/MacOS/OpenCode` and `/usr/bin/vim` do not.
2. `verify_rejects_a_dead_pid`: a fake table with no such pid gives `proses sudah tidak ada`.
3. `verify_rejects_a_recycled_pid`: the pid exists but its command is `/usr/bin/vim` → `pid sudah bukan proses agent`.
4. `verify_accepts_a_live_agent`: returns `"claude"`.
5. `terminate_refuses_protected_pids`: 0, 1 and `std::process::id()` each return `pid tidak boleh dimatikan`.
6. `terminate_ends_a_real_child`: spawn `/bin/cat` (which ignores nothing and exits on SIGTERM), terminate it, assert it is gone and SIGKILL was not needed. Poll with a deadline; do not use a fixed sleep.
7. `terminate_falls_back_to_sigkill`: spawn `/bin/sh -c 'trap "" TERM; sleep 30'`, terminate with a short `wait_ms`, assert it returns `true` (SIGKILL was needed) and the process is gone.
- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes** — 7 tests pass, green on two consecutive runs.
- [ ] **Step 5: Commit** — `git commit -m "feat(app): add guarded process verification and termination"`

---

### Task 2: The three commands

**Files:** Modify `src-tauri/src/lib.rs`

**Interfaces:**
- `recover_nudge(session_id)` — looks the id up in the terminal registry. If the dashboard does not
  own it, return `ok: false` with `Sesi ini jalan di terminal lain, tidak bisa dikirimi tombol.`
  Otherwise write `"\r"` to its PTY and return `Enter dikirim ke sesi.`
- `recover_kill(pid, cwd)` — `verify`, then `terminate` with a 5000 ms grace. Message on success:
  `Proses <kind> di <cwd> dihentikan.` plus ` (terpaksa SIGKILL)` when it came to that.
- `recover_restart(pid, cwd, profile_id)` — `verify`, `terminate`, then start a new PTY session in
  `cwd` with that profile, returning its id in `newTermId`. Message:
  `Sesi baru dimulai di <cwd>. Percakapan lama tidak ikut pindah.`
- All three return `Result<RecoverResult, String>` and never panic.

- [ ] **Step 1:** Implement.
- [ ] **Step 2:** Verify — `cargo check --manifest-path src-tauri/Cargo.toml --workspace` exits 0 and the whole Rust suite passes.
- [ ] **Step 3: Commit** — `git commit -m "feat(app): expose nudge, kill and restart commands"`

---

### Task 3: The UI

**Files:** Modify `src/components/SessionCard.tsx`, `src/components/AttentionBar.tsx`, `src/lib/api.ts`; create `src/components/RecoverMenu.tsx`

**Interfaces:**
- `RecoverMenu` appears **only** when `session.health !== "ok"`. It is a small `Tindakan` button that
  opens a popover with the three actions, each disabled when it does not apply:
  - `Kirim Enter` — disabled with the note `hanya untuk sesi yang dimulai dari sini` when the
    dashboard owns no terminal for that `cwd`.
  - `Restart` — confirmation text:
    `Hentikan <project> lalu mulai sesi baru di folder yang sama? Percakapan lama akan hilang.`
  - `Hentikan` — confirmation text:
    `Hentikan proses <project> (pid <pid>)? Pekerjaan yang belum tersimpan bisa hilang.`
    Rendered in the `err` colour.
- Each confirmation is a real two-step: the button turns into `Yakin?` + `Batal`, and only the second
  click sends the command. Do not use `window.confirm` here — it blocks the whole webview and the
  live loop keeps running behind it.
- The result `message` renders inline under the card for about 6 seconds, in `ok` or `err` colour.
- On a successful restart, switch to the Terminal page and select `newTermId`.
- The attention bar's stalled row gets the same `Tindakan` button for the first stalled session.

- [ ] **Step 1:** Implement.
- [ ] **Step 2:** Verify — `bunx tsc --noEmit`, `bun test`, `bun run build` all clean.
- [ ] **Step 3: Commit** — `git commit -m "feat(ui): act on a stuck session from the card"`

---

## Done when

- The whole Rust suite passes with 7 new tests, green twice.
- `bunx tsc --noEmit`, `bun test`, `bun run build`, `cargo check --workspace` all clean.
- One commit per task, no `Co-Authored-By`, nothing pushed.
- Manual check by the reviewer: start a throwaway `sleep 600` under a fake agent name, confirm the
  dashboard refuses to kill it (`pid sudah bukan proses agent`).

## Self-Review

- **Coverage:** the user asked to revive and to kill. Nudge and restart cover "revive" as far as it
  honestly goes; kill covers the rest.
- **The central honesty problem:** "revive" is not possible for a session the dashboard did not
  start, and a restart is a new conversation, not a resumed one. Both are stated in the UI text
  rather than glossed over.
- **The central safety problem** is pid recycling. Task 1 tests 2 and 3 exist for exactly that, and
  the refusal path is treated as a normal outcome with a clear message.
- **Deliberate gap:** still no automatic recovery. The dashboard never kills anything on its own,
  because a false "stalled" reading — and P12 has already produced one — would then destroy real
  work instead of merely showing a wrong badge.
