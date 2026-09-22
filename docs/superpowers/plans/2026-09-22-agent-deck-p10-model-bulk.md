# Agent Deck P10: Model picker and bulk editing

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Fetching from `/v1/models` becomes "pick the ones you want", and editing many models at once becomes possible: multi-select, bulk limits, bulk delete — with saving blocked while any model would be written broken.

**Architecture:** Pure helpers in `src/lib/models.ts` carry all the logic and all the tests. `ModelList` gains selection state and a bulk action bar. A new `ModelPickerDialog` sits between the fetch call and the list. `ProviderEditPage` gates its save button on the helpers' verdict. Frontend only — no Rust changes.

**Tech Stack:** Same as P7. No new dependency.

**Spec:** Fixes the flow described in `docs/superpowers/plans/2026-09-22-agent-deck-p5-provider-admin.md` Task 7. Mockup reference: `docs/design/07-kelola-provider.png`.

## Why this phase exists

The current flow has three real defects, found by using it:

1. `Ambil dari /v1/models` dumps **every** returned model into the list with no way to choose.
2. Fetched models arrive with `contextLimit: null` and `outputLimit: null`. opencode defaults a
   missing limit to **0**, which silently breaks the model. The UI warns in text but still lets you
   save.
3. Filling limits one model at a time is unusable when the endpoint returns dozens.

## Global Constraints

Same as previous phases. Plus:

- **Never present a guessed limit as fact.** The default limits below are conservative starting
  points, not vendor-verified numbers. Every place they appear must say so, and every value must
  stay editable.
- Saving a model with a null limit must be impossible, not merely discouraged.
- UI text Indonesian, code and commit messages English. No `Co-Authored-By`. Never push.

---

### Task 1: Pure helpers

**Files:** Create `src/lib/models.ts`, `src/lib/models.test.ts`

**Interfaces:**

```ts
interface LimitPair { context: number; output: number }

// Conservative defaults, matched by substring on a lowercased model id, longest brand first.
// These are STARTING POINTS, not vendor-verified figures.
export const defaultLimits = (modelId: string): LimitPair => ...
export const DEFAULT_LIMITS_NOTE = "Angka ini perkiraan. Sesuaikan dengan dokumentasi provider.";

export const toggleOne = (selected: string[], id: string): string[] => ...
export const toggleAll = (selected: string[], visibleIds: string[]): string[] => ...
export const applyLimits = (models: OcModel[], ids: string[], limits: LimitPair): OcModel[] => ...
export const applyDefaultLimits = (models: OcModel[], ids: string[]): OcModel[] => ...
export const removeSelected = (models: OcModel[], ids: string[]): OcModel[] => ...
export const mergeFetched = (models: OcModel[], chosenIds: string[]): OcModel[] => ...
export const incompleteModels = (models: OcModel[]): OcModel[] => ...
export const canSave = (models: OcModel[]): boolean => ...
export const filterModels = (models: OcModel[], query: string): OcModel[] => ...
```

Rules:
- `defaultLimits` table, by substring on the lowercased id, first match wins in this order:
  `claude` → `{context: 200000, output: 64000}`; `gpt`/`codex`/`openai` → `{128000, 16384}`;
  `deepseek` → `{65536, 8192}`; `kimi`/`moonshot` → `{200000, 8192}`; `qwen` → `{131072, 8192}`;
  `glm`/`zhipu` → `{128000, 8192}`; `minimax` → `{192000, 8192}`; `gemini` → `{1000000, 8192}`;
  anything else → `{32768, 4096}`.
- `toggleAll`: when every visible id is already selected, remove them all; otherwise add the missing
  ones. Selected ids that are not currently visible are preserved either way, so a search filter
  cannot silently drop a selection.
- `applyLimits` / `applyDefaultLimits` / `removeSelected` return NEW arrays and leave models whose
  id is not in `ids` untouched. `applyDefaultLimits` uses `defaultLimits(model.id)` per model.
- `mergeFetched` appends only ids not already present, with `name: null` and both limits taken from
  `defaultLimits`, so a freshly picked model is never null-limited. Order: existing first, then new
  in the order given.
- `incompleteModels` returns models whose `contextLimit` or `outputLimit` is `null`, or is `<= 0`.
  Zero counts as incomplete, because opencode treats a missing limit as 0.
- `canSave` is `incompleteModels(models).length === 0`.
- `filterModels` matches the trimmed, lowercased query against `id` and `name`; an empty query
  returns everything.

- [ ] **Step 1: Write the failing tests** in `src/lib/models.test.ts`:
1. `default_limits_match_known_families`: `claude-sonnet-5` → 200000/64000; `kn/deepseek-v4-1-flash` → 65536/8192; `cx/gpt-5.4` → 128000/16384; `cbai/kimi-k2.6(high)` → 200000/8192.
2. `default_limits_fall_back_for_unknown`: `"totally-unknown"` → 32768/4096.
3. `toggle_one_adds_then_removes`.
4. `toggle_all_selects_all_visible_then_clears_them`.
5. `toggle_all_preserves_selection_outside_the_filter`: with `["a","z"]` selected and only `["a","b"]` visible, toggling on keeps `z`, and toggling off still keeps `z`.
6. `apply_limits_only_touches_selected`: two models, one selected; the other is unchanged and the array is a new reference.
7. `apply_default_limits_uses_per_model_family`: a claude and a deepseek selected together get different values.
8. `remove_selected_keeps_the_rest_in_order`.
9. `merge_fetched_skips_duplicates_and_sets_default_limits`: fetching `["a","b"]` when `a` exists adds only `b`, with non-null limits.
10. `incomplete_models_flags_null_and_zero`: a model with `contextLimit: 0` and one with `outputLimit: null` are both flagged; a complete one is not.
11. `can_save_is_false_with_any_incomplete_model`, and true when all are complete.
12. `filter_models_matches_id_and_name_case_insensitively`, and an empty query returns all.
- [ ] **Step 2: Run to verify it fails** — `bun test`.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes** — 12 tests pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(ui): add model selection and limit helpers"`

---

### Task 2: Model picker dialog

**Files:** Create `src/components/ModelPickerDialog.tsx`; modify `src/components/ModelList.tsx`

**Interfaces:**
- `ModelPickerDialog` props:
  `{ fetched: string[]; existing: OcModel[]; onCancel: () => void; onAdd: (ids: string[]) => void }`.
- It lists every fetched id with a checkbox. Ids already in `existing` are shown checked, disabled,
  and labelled `sudah ada`. A search box filters the list. A header checkbox selects or clears all
  currently visible, addable ids (via `toggleAll`).
- The footer shows `N dipilih dari M` and a primary button `Tambah N model`, disabled when none are
  selected. Cancel closes without changing anything.
- Below the footer, one line: `Limit diisi otomatis dengan perkiraan dan bisa diubah setelah ditambahkan.`
- `ModelList`'s `Ambil dari /v1/models` no longer merges directly. It opens this dialog with the
  fetched ids, and `onAdd` calls `mergeFetched`. A fetch that returns an empty list shows
  `Endpoint tidak mengembalikan model.` and opens nothing.

- [ ] **Step 1:** Implement.
- [ ] **Step 2:** Verify — `bunx tsc --noEmit`, `bun test`, `bun run build` all clean.
- [ ] **Step 3: Commit** — `git commit -m "feat(ui): pick which fetched models to add"`

---

### Task 3: Multi-select and bulk actions in the model list

**Files:** Modify `src/components/ModelList.tsx`

**Interfaces:**
- Each row gets a checkbox; the list header gets a select-all checkbox operating on the currently
  filtered rows, in the indeterminate visual state when only some are selected.
- When at least one model is selected, a bulk bar appears above the list showing `N dipilih` and:
  - `Set limit` — opens a small inline form with `Context` and `Output` number inputs, prefilled
    from the first selected model's values when they agree, otherwise empty. `Terapkan` calls
    `applyLimits`. Non-numeric or `<= 0` input is rejected inline with `Harus angka lebih dari 0`.
  - `Isi default` — calls `applyDefaultLimits`, with `DEFAULT_LIMITS_NOTE` shown next to it.
  - `Hapus` — asks `Hapus N model dari daftar?` then calls `removeSelected`. The note
    `Model hanya dihapus dari config, bukan dari provider.` is shown in the confirmation.
  - `Batal pilih` — clears the selection.
- Selection is cleared after any bulk action completes, and whenever `providerId` changes.
- The existing per-row `Tes` button and its result display stay exactly as they are.
- The existing incomplete-limit warning stays, but now reads
  `N model belum punya limit yang sah. Pilih lalu "Isi default" atau "Set limit".`

- [ ] **Step 1:** Implement.
- [ ] **Step 2:** Verify — `bunx tsc --noEmit`, `bun test`, `bun run build` all clean.
- [ ] **Step 3: Commit** — `git commit -m "feat(ui): multi select with bulk limits and delete"`

---

### Task 4: Block saving a broken config

**Files:** Modify `src/pages/ProviderEditPage.tsx`

**Interfaces:**
- The save button is `disabled` when `canSave(models)` is false.
- Next to it, when disabled for that reason, show
  `Tidak bisa simpan: N model tanpa limit akan rusak di opencode (limit dianggap 0).`
- The existing behaviour after a successful save is unchanged: show the backup path and
  `Sesi opencode yang sedang jalan perlu direstart.`

- [ ] **Step 1:** Implement.
- [ ] **Step 2:** Verify — `bunx tsc --noEmit`, `bun test`, `bun run build` all clean.
- [ ] **Step 3: Commit** — `git commit -m "feat(ui): block saving models without limits"`

---

## Done when

- `bun test` passes with 12 new tests.
- `bunx tsc --noEmit`, `bun run build`, `cargo check --workspace` all clean.
- One commit per task, no `Co-Authored-By`, nothing pushed.

## Self-Review

- **Coverage:** the three defects listed at the top map to Task 2 (choose what to add), Task 1 + 4
  (limits are never null and saving is blocked), and Task 3 (bulk editing).
- **Honesty:** the default limits are guesses. They are conservative, labelled as estimates in the
  UI through `DEFAULT_LIMITS_NOTE`, and always editable. The alternative — leaving limits null —
  produces a silently broken model, which is worse.
- **Logic lives in pure helpers**, so the behaviour is tested without rendering; the components stay
  thin. That is why Task 1 carries all 12 tests and Tasks 2-4 carry none.
