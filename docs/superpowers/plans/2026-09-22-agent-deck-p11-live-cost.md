# Agent Deck P11: Live cost

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** The Live board shows a real cost per session and a real total, instead of the `-` placeholder left over from P1.

**Architecture:** `LiveCollector::snapshot` takes the current price table and fills a new `cost_usd` field on every session, reusing the already-tested `PriceTable::cost_usd`. The frontend sums it for the KPI and shows it on each card.

**Tech Stack:** Same as P10. No new dependency.

**Spec:** Closes the gap in `docs/superpowers/specs/2026-09-22-agent-deck-design.md` section 3.2 ("Cost"), which P1 deferred with the note "tersedia setelah tabel harga (P2)".

## Why this phase exists

P2 shipped the price table, but the Live KPI still renders a hardcoded `-` with the stale subtitle
`tersedia setelah tabel harga (P2)`. The pricing math already exists and is tested; only the wiring
is missing.

## Global Constraints

Same as previous phases. Plus:

- Pricing math must NOT be duplicated in TypeScript. `PriceTable::cost_usd` in the collector crate
  stays the single source of truth; the frontend only sums and formats.
- An unknown model prices at 0. That must be visible rather than silently folded into the total —
  see Task 3.
- UI text Indonesian, code and commit messages English. No `Co-Authored-By`. Never push.

---

### Task 1: `cost_usd` on the live session

**Files:** Modify `src-tauri/crates/collector/src/model.rs`, `src-tauri/crates/collector/src/live.rs`

**Interfaces:**
- `Session` gains `pub cost_usd: f64` (serde camelCase → `costUsd`) and
  `pub priced: bool` — false when the session's model is unknown to the price table or the model is
  `None`, so the UI can say the total is incomplete.
- `LiveCollector::snapshot` signature becomes
  `fn snapshot(&mut self, now_ms: i64, prices: &PriceTable) -> LiveSnapshot`.
- For each session: `cost_usd = prices.cost_usd(model, &tokens)` when a model is known, else `0.0`;
  `priced` is true only when the model is `Some` AND the price table matched it. Add
  `PriceTable::matches(&self, model: &str) -> bool` for that check, so `priced` is not inferred from
  a zero cost (a genuinely free model would look unpriced otherwise).
- `LiveSnapshot` gains `pub cost_usd: f64` (the sum) and `pub unpriced: usize` (how many sessions had
  `priced: false`). `same_content` must compare these too.

- [ ] **Step 1: Write the failing tests:**

In `pricing.rs`:
1. `matches_reports_known_and_unknown`: `claude-sonnet-5` and `kn/deepseek-v4-1-flash` match; `totally-unknown` does not.

In `live.rs` (extend the existing fixtures, which already build sessions with tokens and a model):
2. `session_cost_uses_the_price_table`: a claude session with 1,000,000 output tokens gets the table's claude output price, and `priced` is true.
3. `unknown_model_is_zero_and_unpriced`: a session whose transcript model is `weird-model-9` has `cost_usd == 0.0`, `priced == false`, and the snapshot's `unpriced` is 1.
4. `session_without_a_model_is_unpriced`: a session with no assistant record yet (model `None`) has `priced == false`.
5. `snapshot_total_sums_session_costs`: two priced sessions sum, within `1e-9`.
6. `same_content_notices_a_cost_change`: two snapshots identical except for `cost_usd` are NOT `same_content`.
- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement.** Update every existing `snapshot(...)` call site in the tests to pass
  `&PriceTable::defaults()`.
- [ ] **Step 4: Run to verify it passes** — 6 new tests, and every pre-existing collector test still green.
- [ ] **Step 5: Commit** — `git commit -m "feat(collector): price live sessions"`

---

### Task 2: Pass the price table into the live loop

**Files:** Modify `src-tauri/src/lib.rs`

**Interfaces:**
- Both the 1 s background loop and the `live_snapshot` command lock the existing price-table state,
  clone or borrow it, and pass it to `snapshot`.
- The price lock must NOT be held while emitting the event or sleeping. Take it, read what is
  needed, release it.
- After `pricing_set` succeeds, the next tick naturally reprices; no extra invalidation is needed.

- [ ] **Step 1:** Implement.
- [ ] **Step 2:** Verify — `cargo check --manifest-path src-tauri/Cargo.toml --workspace` exits 0 and the whole Rust suite passes.
- [ ] **Step 3: Commit** — `git commit -m "feat(app): feed the price table into the live loop"`

---

### Task 3: Show it

**Files:** Modify `src/lib/types.ts`, `src/components/KpiRow.tsx`, `src/components/SessionCard.tsx`, `src/lib/summarize.ts`, `src/lib/summarize.test.ts`

**Interfaces:**
- `Session` gains `costUsd: number` and `priced: boolean`; `LiveSnapshot` gains `costUsd: number` and
  `unpriced: number`.
- `summarize` gains `cost: number` (the sum of `costUsd`) and `unpriced: number` (count of
  `priced === false`). Extend the existing `summarize` test to assert both.
- `KpiRow`'s fourth KPI becomes the real value:
  - value: `formatUsd(cost)` from `src/lib/cost.ts`
  - subtitle when `unpriced === 0`: `semua model punya harga`
  - subtitle when `unpriced > 0`: `N model belum punya harga` in the `waiting` colour, because the
    total is then an undercount and the user should know
  - remove the stale `tersedia setelah tabel harga (P2)` string entirely.
- `SessionCard` shows the per-session cost next to the token count, in the same monospace style. A
  session with `priced === false` shows `-` there with the `title` attribute
  `Model ini belum ada di tabel harga.` rather than a misleading `$0,00`.

- [ ] **Step 1: Write the failing test** — extend `src/lib/summarize.test.ts` so the expected object
  includes `cost` and `unpriced`, with one unpriced session in the fixture.
- [ ] **Step 2: Run to verify it fails** — `bun test`.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes**, and confirm `bunx tsc --noEmit` and `bun run build` are clean.
- [ ] **Step 5: Commit** — `git commit -m "feat(ui): show live cost on the board"`

---

## Done when

- The whole Rust suite passes with 6 new tests; `bun test` passes with the extended summarize test.
- `bunx tsc --noEmit`, `bun run build`, `cargo check --workspace` all clean.
- No occurrence of `tersedia setelah tabel harga` remains in `src/`.
- One commit per task, no `Co-Authored-By`, nothing pushed.

## Self-Review

- **Coverage:** the placeholder is replaced by a real number, per session and in total.
- **Honesty:** an unknown model is reported as unpriced rather than counted as $0, both per card and
  as a count next to the KPI, so a partial total never looks complete. `PriceTable::matches` exists
  precisely so a legitimately free model is not mislabelled.
- **No duplicated math:** the frontend sums and formats only; `PriceTable::cost_usd` stays the one
  implementation, already covered by the P2 pricing tests.
