# Agent Deck P19: Billing accounts — subscriptions, prepaid credit, and pay-as-you-go

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Know what a month actually costs. A subscription is a flat fee, prepaid credit drains, and only pay-as-you-go grows with every token — the dashboard must stop pretending all three are the same thing.

**Architecture:** A `billing` module owns a list of accounts, each matching some models and declaring how it is paid for. Cost becomes two numbers everywhere: **spend** (money that actually moves) and **notional** (what the same tokens would cost at API rates). The Token & Biaya page gains a per-account monthly summary keyed to each account's own billing cycle.

**Depends on:** P2 (price table, history index), P11 (live cost).

## The problem with what exists

P2 and P11 assume every token costs money at a per-model rate. That is only true for pay-as-you-go.
Right now a Claude Max subscription shows as **$806 of spend** when the real incremental cost of
those tokens is **zero** — the month costs whatever the plan costs, no matter how much is used. The
figure is not just imprecise, it is the wrong kind of number.

## Decisions taken with the user

- **A subscription session still shows a dollar figure**, prefixed `≈` and rendered in the muted
  colour, meaning "this is what it would have cost at API rates". It answers "is the plan worth it",
  as long as it can never be mistaken for a bill. It is never added to spend.
- **The month runs on each account's own billing cycle**, anchored to its renewal day — a plan that
  renews on the 14th has a period of the 14th to the 13th. Calendar months would not match any real
  invoice.

## Global Constraints

Same as previous phases. Plus these, which are the point of the phase:

- **Never add subscription usage to spend.** Spend is money that moves: pay-as-you-go charges and
  prepaid credit consumed. A test pins this.
- **Never show a bare dollar amount for a subscription.** It always carries the `≈` marker and the
  muted tone, and its tooltip says `Perkiraan kalau dibayar per token. Tidak menambah tagihan.`
- A model that matches no account falls back to pay-as-you-go with the existing price table, and is
  counted as `unpriced` when the table does not know it either — the P11 honesty rule stays.
- Dates are stored as `YYYY-MM-DD` strings and compared in local time.
- UI text Indonesian, code and commit messages English. No `Co-Authored-By`. Never push.

## Command API (the contract)

```ts
// invoke("billing_accounts") -> BillingAccount[]
// invoke("billing_save", { accounts: BillingAccount[] }) -> BillingAccount[]
interface BillingAccount {
  id: string;
  label: string;              // "Claude Max", "Kenari topup", ...
  mode: "subscription" | "prepaid" | "payg";
  matches: string[];          // model-id prefixes, e.g. ["claude-"], ["kn/"]
  monthlyUsd: number | null;  // subscription only
  renewalDay: number | null;  // subscription only, 1..28
  creditUsd: number | null;   // prepaid only: the amount topped up
  startedOn: string | null;   // prepaid only, YYYY-MM-DD
  expiresOn: string | null;   // subscription or prepaid, YYYY-MM-DD
}

// invoke("billing_summary", { asOf: string }) -> BillingSummary
interface BillingSummary {
  periods: AccountPeriod[];
  totalSpendUsd: number;      // money that actually moved this period
  totalNotionalUsd: number;   // what all usage would have cost at API rates
  warnings: string[];         // e.g. "Kenari topup habis dalam 6 hari"
}
interface AccountPeriod {
  accountId: string; label: string; mode: BillingAccount["mode"];
  fromDate: string; toDate: string;        // this account's current cycle
  tokens: TokenUsage;
  spendUsd: number;                         // 0 for subscription
  notionalUsd: number;
  committedUsd: number | null;              // subscription: monthlyUsd
  creditLeftUsd: number | null;             // prepaid: creditUsd - spend since startedOn
  daysLeft: number | null;                  // until renewal or expiry
  expired: boolean;
}
```

`Session` gains `billingMode: "subscription" | "prepaid" | "payg"`. `costUsd` keeps its meaning of
"cost of these tokens at API rates"; the UI decides how to label it based on `billingMode`.

---

### Task 1: Billing model and period maths

**Files:** Create `src-tauri/crates/collector/src/billing.rs`; modify `src-tauri/crates/collector/src/lib.rs`; add `time` feature `local-offset` if needed

**Interfaces:**
- `struct BillingAccount { ... }` exactly as the contract, serde camelCase.
- `struct BillingTable { accounts: Vec<BillingAccount> }` with `load(path)`, `save(path)`,
  `defaults()` (an empty list — do NOT invent the user's plans), and
  `fn match_account(&self, model: &str) -> Option<&BillingAccount>`.
- Matching: longest matching `matches` prefix wins, after the same provider-prefix stripping the
  price table already does, so `kn/deepseek-v4-1-flash` can match either `kn/` or `deepseek-`.
- `fn cycle_for(account: &BillingAccount, as_of: Date) -> (Date, Date)`:
  - `subscription` with `renewal_day` d: the period starts on the most recent occurrence of day d at
    or before `as_of`, and ends the day before the next occurrence. `renewal_day` is capped at 28 so
    no month is skipped.
  - `prepaid`: the period runs from `started_on` to `expires_on`, or to `as_of` when there is no
    expiry.
  - `payg`: the calendar month containing `as_of`.
- `fn days_left(account, as_of) -> Option<i64>` — to renewal for a subscription, to expiry for
  prepaid, `None` for payg. Negative means expired.

- [ ] **Step 1: Write the failing tests:**
1. `match_picks_the_longest_prefix`: with `["claude-"]` and `["claude-opus"]`, `claude-opus-5` picks the longer one.
2. `match_strips_the_provider_prefix`: `kn/deepseek-v4-1-flash` matches an account declaring `deepseek-`.
3. `match_returns_none_for_an_unknown_model`.
4. `cycle_for_subscription_mid_month`: renewal day 14, as_of the 20th → period the 14th to the 13th of next month.
5. `cycle_for_subscription_before_the_renewal_day`: renewal day 14, as_of the 3rd → period the 14th of LAST month to the 13th of this one.
6. `cycle_for_subscription_handles_year_rollover`: renewal day 14, as_of 5 January → starts 14 December.
7. `cycle_for_prepaid_uses_started_and_expires`.
8. `cycle_for_payg_is_the_calendar_month`.
9. `days_left_counts_down_and_goes_negative_when_expired`.
10. `renewal_day_is_capped_at_28`: an account saved with day 31 behaves as 28.
- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes** — 10 tests pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(collector): add billing accounts and cycle maths"`

---

### Task 2: Spend versus notional

**Files:** Modify `src-tauri/crates/collector/src/store.rs`, `src-tauri/crates/collector/src/billing.rs`

**Interfaces:**
- `fn split_cost(account: Option<&BillingAccount>, notional: f64) -> (f64 /*spend*/, f64 /*notional*/)`:
  - `None` or `payg` → `(notional, notional)`
  - `subscription` → `(0.0, notional)`
  - `prepaid` → `(notional, notional)` — prepaid credit is consumed at API rates
- `Store::by_account(&self, from_ms, to_ms, table: &BillingTable, prices: &PriceTable) -> Vec<AccountAgg>`
  aggregating the `message` table per matched account, returning tokens, spend and notional.
  Messages matching no account are grouped under a synthetic `payg` account with id `""` and label
  `Tanpa akun`.

- [ ] **Step 1: Write the failing tests:**
1. `split_cost_subscription_has_zero_spend_and_full_notional`.
2. `split_cost_payg_and_prepaid_spend_the_full_amount`.
3. `split_cost_without_an_account_is_payg`.
4. `by_account_groups_messages_and_sums_both_numbers`: three messages across two accounts produce the right tokens, spend and notional for each.
5. `by_account_puts_unmatched_models_in_the_fallback_group`.
6. `by_account_respects_the_time_window`.
- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes** — 6 tests pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(collector): separate spend from notional cost"`

---

### Task 3: Commands

**Files:** Modify `src-tauri/src/lib.rs`

**Interfaces:**
- `billing_accounts`, `billing_save` (validating: `label` non-empty, `matches` non-empty,
  `renewal_day` in 1..=28, `monthly_usd >= 0`, dates parseable — each with an Indonesian error),
  and `billing_summary` building `AccountPeriod` rows via `cycle_for` and `by_account`.
- Warnings in the summary: `<label> habis dalam N hari` when prepaid credit at the current burn rate
  runs out before `expires_on`, and `<label> sudah lewat tanggal` for anything expired.
- The live loop sets `Session.billing_mode` from `match_account`.
- The accounts file lives at `<app data dir>/billing.json`.

- [ ] **Step 1: Write the failing tests** for the validation errors and the default empty list.
- [ ] **Step 2: Run to verify they fail.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify they pass**, and `cargo check --workspace` exits 0.
- [ ] **Step 5: Commit** — `git commit -m "feat(app): expose billing commands"`

---

### Task 4: The UI

**Files:** Create `src/components/BillingDialog.tsx`, `src/components/MonthlySummary.tsx`; modify `src/pages/TokenPage.tsx`, `src/components/SessionCard.tsx`, `src/components/KpiRow.tsx`, `src/lib/api.ts`, `src/lib/types.ts`

**Interfaces:**
- `BillingDialog`, opened from a `Langganan & saldo` button on the Token page, edits the account
  list: label, mode, the model prefixes it covers, and the fields that mode needs. Mode changes show
  and hide fields rather than leaving irrelevant ones enabled. Include the note
  `Kosongkan kalau semua model dibayar per token.`
- `MonthlySummary` sits at the top of the Token page: one row per account with its cycle dates, the
  tokens used, and the number that matters for that mode:
  - subscription → `$X/bln · sisa N hari`, plus `≈ $Y setara API` in muted text
  - prepaid → `sisa $X dari $Y · habis N hari lagi` with a thin progress bar
  - payg → `$X bulan ini`
  An expired account shows a red `Sudah lewat tanggal` chip.
- `KpiRow`'s `Estimasi biaya` splits into the real figure and the notional one:
  value = `formatUsd(spend)`, subtitle = `≈ $Y setara API` when notional differs from spend.
- `SessionCard`: when `billingMode` is `subscription`, the cost renders as `≈ $X` in `text-fg-3`
  with `title="Perkiraan kalau dibayar per token. Tidak menambah tagihan."`. Other modes are
  unchanged.

- [ ] **Step 1:** Implement.
- [ ] **Step 2:** Verify — `bunx tsc --noEmit`, `bun test`, `bun run build` all clean.
- [ ] **Step 3: Commit** — `git commit -m "feat(ui): show subscriptions, credit and monthly usage"`

---

## Done when

- The whole Rust suite passes with 22 new tests.
- `bunx tsc --noEmit`, `bun test`, `bun run build`, `cargo check --workspace` all clean.
- One commit per task, no `Co-Authored-By`, nothing pushed.

## Self-Review

- **Coverage:** subscriptions with a price and a renewal day, prepaid credit with an expiry and a
  remaining balance, pay-as-you-go unchanged, and a monthly view on each account's real cycle.
- **The honesty problem this phase exists to fix:** the dashboard currently reports roughly $800 of
  "cost" for tokens that cost nothing extra. Spend and notional are separated everywhere, and the
  `≈` marker plus the muted tone make the notional figure unmistakable. Task 2 test 1 pins the
  zero-spend rule.
- **Deliberate gap:** no provider APIs are called to fetch a real balance. Everything is what the
  user typed, so the numbers are as good as the input and never pretend to be an authoritative
  invoice.
