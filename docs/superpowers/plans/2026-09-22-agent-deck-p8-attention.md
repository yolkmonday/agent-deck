# Agent Deck P8: Attention routing — jump to the session that needs an answer

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** When any agent is waiting for input, the dashboard says so everywhere and takes you to exactly the right terminal in one click — or automatically, if you want that.

**Architecture:** A `setting` table plus two commands hold the user's preference. A frontend `attention` module derives the waiting set, detects transitions, and resolves each waiting session to a jump target: a dashboard-owned terminal tab when one matches its directory, otherwise its Live card. An attention bar renders on every page, the sidebar badge becomes a jump button, and an OS notification fires when the window is not focused.

**Tech Stack:** Same as P7, plus `tauri-plugin-notification` (Rust) and `@tauri-apps/plugin-notification` 2.4.0 (JS).

**Spec:** Extends `docs/superpowers/specs/2026-09-22-agent-deck-design.md` section 3.3.

## Global Constraints

Same as previous phases. Plus, specific to this one:

- **Honesty about what the dashboard can do.** It can only type into sessions it started itself.
  A waiting session running in an external terminal is routed to its Live card with the existing
  message `Sesi ini jalan di terminal lain. Buka di sana.` Never imply the app can answer it.
- **Never steal focus without permission.** Auto-jump is off by default. The user opts in.
- UI text Indonesian, code and commit messages English. No `Co-Authored-By`. Never push.

## Behaviour

Three modes, stored as `attentionMode`:

| Mode | What happens when a session starts waiting |
|---|---|
| `off` | Only the attention bar and the sidebar badge update. |
| `notify` (default) | Plus an OS notification, but only while the app window is NOT focused. |
| `auto` | Plus the app jumps to that session immediately, and focuses its terminal input when it owns one. |

In every mode the attention bar is always visible while at least one session waits, and clicking
`Buka` jumps.

Jump target resolution for a waiting session `s`:
1. A dashboard-owned terminal session whose `cwd` equals `s.cwd` and which is still `alive` →
   `{ kind: "terminal", termId }`. If several match, the most recently started wins.
2. Otherwise → `{ kind: "live", sessionId: s.id }`.

## Command API (the contract)

```ts
// invoke("settings_get") -> Settings
// invoke("settings_set", { settings: Settings }) -> Settings
interface Settings {
  attentionMode: "off" | "notify" | "auto";
  notifySound: boolean;     // reserved; the UI shows it but P8 does not play a sound
}
```

Unknown or missing values fall back to the defaults (`notify`, `false`) rather than erroring.

---

### Task 1: `setting` table and commands

**Files:**
- Modify: `src-tauri/crates/collector/src/store.rs`, `src-tauri/src/lib.rs`

**Interfaces:**
- Schema addition in `Store::open`:

```sql
CREATE TABLE IF NOT EXISTS setting (key TEXT PRIMARY KEY, value TEXT NOT NULL);
```

- `Store::setting(&self, key: &str) -> anyhow::Result<Option<String>>`
- `Store::set_setting(&mut self, key: &str, value: &str) -> anyhow::Result<()>` (upsert)
- `Store::settings_all(&self) -> anyhow::Result<Vec<(String, String)>>`
- In `lib.rs`: `struct Settings { pub attention_mode: String, pub notify_sound: bool }` (serde
  camelCase) with commands `settings_get` and `settings_set`. `settings_get` reads the rows and
  falls back to `attention_mode: "notify"`, `notify_sound: false`. `settings_set` rejects an
  `attention_mode` outside the three values with the error `mode tidak dikenal`.

- [ ] **Step 1: Write the failing tests:**

In `store.rs`:
1. `setting_round_trips_and_upserts`: set `a=1`, read `Some("1")`, set `a=2`, read `Some("2")`, and `settings_all` has one row.
2. `missing_setting_is_none`: reading an unset key returns `None`.

In `lib.rs` tests:
3. `settings_default_when_empty`: `attentionMode` is `notify` and `notifySound` is false on a fresh store.
4. `settings_round_trip_through_commands`: setting `auto` + true reads back identically.
5. `settings_reject_unknown_mode`: `"turbo"` returns `Err("mode tidak dikenal")` and does NOT change the stored value.
6. `settings_unknown_stored_value_falls_back`: write `attention_mode = "garbage"` directly into the table, then `settings_get` returns `notify` rather than erroring.
- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes** — 6 tests pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(app): add settings store and commands"`

---

### Task 2: Notification plugin

**Files:** Modify `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs`, `src-tauri/capabilities/default.json`, `package.json`

**Interfaces:**
- `cargo add tauri-plugin-notification` inside `src-tauri` (let cargo pick the version that matches
  Tauri 2; record it in the commit message). Register it with `.plugin(tauri_plugin_notification::init())`
  before `.setup(...)`.
- `bun add @tauri-apps/plugin-notification`.
- Add the `notification:default` permission to the app's capability file.
- Add a command `window_focused() -> bool` returning whether the main window is currently focused,
  so the frontend can decide whether to notify.

- [ ] **Step 1:** Implement.
- [ ] **Step 2:** Verify — `cargo check --manifest-path src-tauri/Cargo.toml --workspace` exits 0 and the whole Rust suite still passes.
- [ ] **Step 3: Commit** — `git commit -m "feat(app): add notification plugin and window focus command"`

---

### Task 3: Attention logic

**Files:**
- Create: `src/lib/attention.ts`, `src/lib/attention.test.ts`
- Modify: `src/lib/api.ts` (add `settingsGet`, `settingsSet`, `windowFocused` and the `Settings` type)

**Interfaces:**
- `waitingSessions(sessions: Session[]): Session[]` — status `waiting`, oldest `updatedAtMs` first,
  so the longest-waiting agent is handled first.
- `newlyWaiting(prev: Session[] | null, next: Session[]): Session[]` — sessions that are waiting now
  and were not waiting before. `prev === null` returns `[]`, so opening the app does not fire a
  burst of notifications for sessions that were already waiting.
- `type JumpTarget = { kind: "terminal"; termId: string } | { kind: "live"; sessionId: string }`
- `resolveTarget(session: Session, terms: TermSession[]): JumpTarget` — implements the two rules in
  the Behaviour section; among several alive matches, the largest `startedAtMs` wins; a dead
  terminal session never matches.
- `waitingLabel(session: Session, nowMs: number): string` — `"noor butuh jawaban · 8 mnt"`, using the
  existing `formatDuration` against `updatedAtMs`.

- [ ] **Step 1: Write the failing tests** in `src/lib/attention.test.ts`:
1. `waiting_sessions_oldest_first`: three sessions, two waiting with different `updatedAtMs`; the older one comes first and the busy one is excluded.
2. `newly_waiting_is_empty_on_first_snapshot`: `prev === null` returns `[]`.
3. `newly_waiting_detects_only_transitions`: a session already waiting in `prev` is not returned; one that flipped from busy to waiting is.
4. `newly_waiting_includes_a_brand_new_waiting_session`: a session absent from `prev` and waiting in `next` is returned.
5. `resolve_target_prefers_a_matching_live_terminal`: a term session with the same cwd and `alive: true` gives `{kind:"terminal"}` with its id.
6. `resolve_target_ignores_dead_terminals`: the only match has `alive: false`, so the result is `{kind:"live"}`.
7. `resolve_target_picks_the_newest_match`: two alive matches; the one with the larger `startedAtMs` wins.
8. `resolve_target_falls_back_to_live_for_external_sessions`: no cwd match gives `{kind:"live"}` with the session id.
9. `waiting_label_formats_project_and_duration`: exact string for a session waiting 8 minutes.
- [ ] **Step 2: Run to verify it fails** — `bun test`.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes** — 9 tests pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(ui): add attention routing logic"`

---

### Task 4: Attention bar, jump wiring and notifications

**Files:**
- Create: `src/components/AttentionBar.tsx`, `src/components/SettingsPopover.tsx`
- Modify: `src/App.tsx`, `src/components/Sidebar.tsx`, `src/store/live.ts`, `src/store/terminal.ts`, `src/pages/LivePage.tsx`

**Interfaces:**
- `App.tsx` owns a `jumpTo(target: JumpTarget)` function: for `terminal` it switches to the Terminal
  page, selects that tab and calls a focus hook on the terminal view; for `live` it switches to the
  Live page and sets a `highlightSessionId` that `LivePage` passes to the matching `SessionCard`,
  which renders a brief ring using the `waiting` colour and scrolls itself into view.
- `AttentionBar` renders above the page content whenever `waitingSessions` is non-empty: a waiting
  dot, `waitingLabel` for the first one, `dan N lainnya` when there are more, a `Buka` button, and a
  gear that opens `SettingsPopover`. It uses the `waiting` colour tokens already in `index.css`.
- Notification effect, in `App.tsx` or a small hook: on each live snapshot, compute `newlyWaiting`.
  For each, when the mode is `notify` and `windowFocused()` is false, send one notification titled
  `<project> butuh jawaban` with the body `<model> · menunggu input`. When the mode is `auto`, call
  `jumpTo(resolveTarget(...))` for the first one instead. Request notification permission once, on
  first use, and if permission is denied fall back to the bar only, without throwing.
- `SettingsPopover`: three radio options labelled `Diam`, `Beri tahu` and `Langsung buka`, with the
  one-line help `Langsung buka akan memindahkan layar sendiri saat ada agent yang bertanya.`
  It saves through `settingsSet`.
- The sidebar's existing waiting badge becomes a button that jumps to the first waiting session.
- `TerminalView` gains an imperative focus path (a ref or a registry entry) so `jumpTo` can put the
  cursor in the right terminal.
- All existing tests must still pass.

- [ ] **Step 1:** Implement.
- [ ] **Step 2:** Verify — `bunx tsc --noEmit`, `bun test`, `bun run build` all clean.
- [ ] **Step 3: Commit** — `git commit -m "feat(ui): add attention bar and jump to waiting session"`

---

## Done when

- The whole Rust suite passes with 6 new tests; `bun test` passes with 9 new tests.
- `bunx tsc --noEmit`, `bun run build`, `cargo check --workspace` all clean.
- One commit per task, no `Co-Authored-By`, nothing pushed.

## Self-Review

- **Coverage:** always-visible bar, one-click jump, optional auto-jump, OS notification only when
  unfocused, and a clickable sidebar badge.
- **Deliberate limit, stated in the UI:** a waiting session the dashboard does not own routes to its
  Live card, not to a fake input. The app cannot type into a process it did not start, and P8 does
  not pretend otherwise.
- **Default is conservative:** `notify`, not `auto`, because a window that jumps on its own while
  the user is reading something else is hostile. The user opts into that.
- **Type consistency:** `Settings` is identical in the Command API section, Task 1's Rust struct and
  Task 3's TypeScript. `JumpTarget` is defined once in `attention.ts` and consumed by `App.tsx`.
