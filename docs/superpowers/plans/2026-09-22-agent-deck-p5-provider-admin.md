# Agent Deck P5: Model & Provider + opencode Admin

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** See every model available to each agent, and manage opencode providers, models and API keys from the dashboard — with keys stored in 0600 files instead of plaintext in the config.

**Architecture:** A `config` module edits `opencode.jsonc` while preserving comments, always via backup + temp file + atomic rename + re-parse. A `secrets` module owns the 0600 key files. A `probe` module tests a model with one tiny request. Tauri commands wrap all three. Two frontend pages render the overview and the editor.

**Tech Stack:** Same as P4, plus `jsonc-parser = "0.26"` (or an equivalent JSONC-preserving editor crate) and `reqwest = { version = "0.12", features = ["json", "rustls-tls"] }` on the Rust side.

**Spec:** `docs/superpowers/specs/2026-09-22-agent-deck-design.md` (section 3.4, 3.5). Mockups: `docs/design/06-model-provider.png`, `docs/design/07-kelola-provider.png`.

## Global Constraints

Everything from P4, plus the rules that make this phase safe. These are not optional.

- **A key value never reaches the frontend.** Commands return a masked form only
  (`sk-6a8b••••••••dc`: first 6 and last 2 characters, middle replaced by bullets; a key shorter
  than 12 characters is returned as `••••••••`). There is exactly one command that returns a
  plaintext key (`secret_reveal`), it requires an explicit user action, and its result is never
  logged, never cached and never written to disk by the dashboard.
- **Keys are never written into `opencode.jsonc`.** They go to
  `~/.config/opencode/secrets/<provider>.key` with mode 0600 (directory 0700), and the config
  references them as `{file:~/.config/opencode/secrets/<provider>.key}`.
- **Every config write is reversible**: copy to `opencode.jsonc.bak-<epoch_ms>` first (keep the 10
  newest, delete older ones), write to `opencode.jsonc.tmp-<pid>` in the same directory, `fsync`,
  atomically `rename`, then re-read and re-parse. If the re-parse fails, restore the backup and
  return an error.
- **Comments in the config survive every edit.** A test proves it.
- Never read or write `~/.local/share/opencode/auth.json`. OAuth credentials stay with the
  `opencode providers` CLI.
- Never log a key, a full config file, or a request body containing a key.
- No `Co-Authored-By`, never push, Bun only, `@/` alias, Indonesian UI text, English code.

## Command API (the contract)

```ts
// invoke("models_overview") -> ModelsOverview
interface ModelsOverview {
  claude: { models: string[]; recentlyUsed: string[] };
  opencode: OcProvider[];
  codex: { models: string[]; note: string };
}
interface OcProvider {
  id: string; name: string; npm: string; baseUrl: string;
  auth: "config" | "cli" | "none";      // config = key in our secrets file or inline; cli = credential from `opencode providers list`
  headerStyle: "bearer" | "custom";
  customHeaderName: string | null;
  enabled: boolean;                      // false when listed in disabled_providers
  keyMasked: string | null;
  keyInline: boolean;                    // true = still plaintext in the config, needs migration
  source: "global" | "project";
  models: OcModel[];
}
interface OcModel { id: string; name: string | null; contextLimit: number | null; outputLimit: number | null }

// invoke("provider_save", { provider: OcProviderInput }) -> OcProvider
interface OcProviderInput {
  id: string; name: string; npm: string; baseUrl: string;
  headerStyle: "bearer" | "custom"; customHeaderName: string | null;
  enabled: boolean; models: OcModel[];
}
// invoke("provider_delete", { id: string }) -> null
// invoke("secret_set", { providerId: string, key: string }) -> string      // returns the masked form
// invoke("secret_clear", { providerId: string }) -> null
// invoke("secret_reveal", { providerId: string }) -> string                // plaintext, explicit user action only
// invoke("secret_migrate_inline", { providerId: string }) -> string        // moves an inline key to a 0600 file, returns masked
// invoke("models_fetch", { providerId: string }) -> string[]               // GET <baseUrl>/models
// invoke("model_test", { providerId: string, model: string }) -> ModelTestResult
interface ModelTestResult { ok: boolean; status: number | null; latencyMs: number; reply: string | null; error: string | null; usedHeader: "bearer" | "custom" }
// invoke("config_backups") -> { path: string; atMs: number }[]
// invoke("config_restore", { path: string }) -> null
```

---

### Task 1: Secrets module

**Files:**
- Create: `src-tauri/src/secrets.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod secrets;`)

**Interfaces:**
- `fn secrets_dir(home: &str) -> PathBuf` → `<home>/.config/opencode/secrets`
- `fn key_path(home: &str, provider_id: &str) -> anyhow::Result<PathBuf>` — rejects a provider id
  that is not `^[A-Za-z0-9_-]{1,64}$`, so no path traversal is possible
- `fn write_key(home: &str, provider_id: &str, key: &str) -> anyhow::Result<()>` — creates the dir
  with mode 0700, writes the file with mode 0600 (set the mode at create time via
  `OpenOptions::mode`, not afterwards), trims the key, rejects an empty key
- `fn read_key(home: &str, provider_id: &str) -> anyhow::Result<String>` (trims the content)
- `fn clear_key(home: &str, provider_id: &str) -> anyhow::Result<()>` (missing file is not an error)
- `fn mask(key: &str) -> String` — first 6 + bullets + last 2; `••••••••` when shorter than 12
- `fn file_ref(home: &str, provider_id: &str) -> String` → `{file:~/.config/opencode/secrets/<id>.key}`
  (always the `~` form, never an absolute path, so the config stays portable)

- [ ] **Step 1: Write the failing tests** (temp HOME via `tempfile`):
1. `write_key_sets_0600_and_dir_0700`: assert the exact Unix modes with `std::os::unix::fs::PermissionsExt`.
2. `read_key_round_trips_and_trims`: write `"  abc\n"`, read `"abc"`.
3. `clear_key_is_idempotent`: clear twice, no error; the file is gone.
4. `key_path_rejects_traversal`: ids `"../evil"`, `"a/b"`, `""` and a 65-char id all error.
5. `mask_hides_the_middle`: `"sk-6a8b9f7e862e2668-e63vy0-0f8658dc"` masks to a string that starts `sk-6a8`, ends `dc`, contains `•`, and does NOT contain `9f7e862e2668`.
6. `mask_short_key_is_fully_hidden`: `"short"` → `"••••••••"`.
7. `write_key_rejects_empty`: `""` and `"   "` error.
8. `file_ref_uses_tilde_form`: exact expected string.
- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes** — 8 tests pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(app): add 0600 secrets store for provider keys"`

---

### Task 2: JSONC config editor

**Files:**
- Create: `src-tauri/src/config.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod config;`), `src-tauri/Cargo.toml`

**Interfaces:**
- `struct ConfigFile { path: PathBuf, text: String }` with:
  - `fn load(path: &Path) -> anyhow::Result<ConfigFile>`
  - `fn value(&self) -> anyhow::Result<serde_json::Value>` (JSONC parsed, comments stripped)
  - `fn set_path(&mut self, json_path: &[&str], value: serde_json::Value) -> anyhow::Result<()>` — an edit that preserves the rest of the file's formatting and comments
  - `fn remove_path(&mut self, json_path: &[&str]) -> anyhow::Result<()>`
  - `fn save_atomic(&self) -> anyhow::Result<PathBuf>` — backup, temp write, fsync, rename, re-parse; returns the backup path; restores the backup and errors if the re-parse fails
- `fn backups(dir: &Path) -> Vec<(PathBuf, i64)>` newest first; `fn prune_backups(dir: &Path, keep: usize)`
- `fn restore(backup: &Path, target: &Path) -> anyhow::Result<()>`

- [ ] **Step 1: Write the failing tests** (temp files):
1. `set_path_preserves_comments_and_other_keys`: start from a config containing `// a comment` and two providers; change one provider's `baseURL`; assert the comment text is still present, the other provider is untouched, and the new value parses back.
2. `set_path_creates_missing_objects`: setting `["provider","new","options","baseURL"]` on a config without `new` creates the nested objects.
3. `remove_path_deletes_only_the_target`: removing one provider leaves the others and the comments.
4. `save_atomic_creates_a_backup_and_leaves_no_temp_file`: after save, exactly one `.bak-*` exists, no `.tmp-*` remains, and the target parses.
5. `save_atomic_restores_backup_when_result_is_invalid`: force an invalid text (set `text` to `"{oops"` directly in the test) and assert `save_atomic` errors AND the original file content is intact.
6. `prune_backups_keeps_only_the_newest`: create 13 backups, prune to 10, assert the 10 newest remain.
7. `restore_puts_the_backup_back`: modify, restore, content matches the original.
- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement.** If `jsonc-parser` cannot do formatting-preserving edits, implement
  `set_path`/`remove_path` as a minimal targeted text edit using the parser's node ranges; do NOT
  fall back to reserialising the whole document, because that would destroy the comments.
- [ ] **Step 4: Run to verify it passes** — 7 tests pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(app): add comment preserving jsonc config editor"`

---

### Task 3: Provider model and overview

**Files:** Create `src-tauri/src/providers.rs`; modify `src-tauri/src/lib.rs`

**Interfaces:**
- The wire structs from the Command API, with serde camelCase.
- `fn read_overview(home: &str) -> anyhow::Result<ModelsOverview>`:
  - opencode providers come from the merged global config (`~/.config/opencode/opencode.jsonc`,
    falling back to `opencode.json`); `source` is `global` for now (project configs are listed but
    not edited in P5 — say so in the UI).
  - `auth` is `config` when the provider has an `apiKey` or a custom header, `cli` when
    `opencode providers list` shows a credential for it, else `none`. Parse the CLI output; if the
    command fails, treat every provider as `none` and continue.
  - `keyInline` is true when the config value is a literal key rather than a `{file:...}` or
    `{env:...}` reference. `keyMasked` is derived from the secrets file when referenced, or from
    the literal value when inline.
  - Claude models: the fixed list `claude-fable-5-1`, `claude-opus-5`, `claude-sonnet-5`,
    `claude-haiku-4-5`, plus `recentlyUsed` read from the dashboard's own index
    (`SELECT DISTINCT model FROM message WHERE agent='claude'`).
  - Codex models: `SELECT DISTINCT model FROM message WHERE agent='codex'`, with
    `note: "Dibaca dari riwayat sesi Codex."`
- `fn save_provider(home: &str, input: OcProviderInput) -> anyhow::Result<OcProvider>` writes the
  provider block through `ConfigFile`, sets `options.apiKey` to the `{file:...}` reference for
  `headerStyle: "bearer"`, or `options.headers.<name>` to it for `"custom"` (removing the other
  form), and adds or removes the id in `disabled_providers` per `enabled`.
- `fn delete_provider(home: &str, id: &str) -> anyhow::Result<()>` removes the block and the
  `disabled_providers` entry; it does NOT delete the key file (the user does that explicitly).

- [ ] **Step 1: Write the failing tests** against a temp HOME with a handcrafted `opencode.jsonc`
  containing comments, one provider with an inline key, one with a `{file:}` reference, and a
  `disabled_providers` array:
1. `overview_masks_inline_key_and_flags_it`: the inline provider has `keyInline: true` and a masked key; the plaintext never appears in the returned struct (assert with a substring check).
2. `overview_reads_file_referenced_key_as_not_inline`: `keyInline: false`.
3. `overview_marks_disabled_providers`: `enabled: false` for the listed id.
4. `overview_detects_custom_header_style`: a provider with `options.headers.x-api-key` reports `headerStyle: "custom"` and `customHeaderName: "x-api-key"`.
5. `save_provider_writes_file_reference_not_the_key`: after saving, the config text contains `{file:` and does NOT contain any raw key characters.
6. `save_provider_switching_to_custom_header_removes_apikey`: assert `options.apiKey` is gone and the header is present.
7. `save_provider_toggles_disabled_providers`: disabling adds the id, enabling removes it.
8. `delete_provider_keeps_other_providers_and_comments`.
- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes** — 8 tests pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(app): read and write opencode provider config"`

---

### Task 4: Model fetch and test probe

**Files:** Create `src-tauri/src/probe.rs`; modify `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml`

**Interfaces:**
- `async fn fetch_models(base_url: &str, key: &str, header: HeaderStyle) -> anyhow::Result<Vec<String>>`
  — `GET {base_url}/models`, parse `data[].id`.
- `async fn test_model(base_url: &str, key: &str, header: HeaderStyle, model: &str) -> ModelTestResult`
  — `POST {base_url}/chat/completions` with
  `{"model": <model>, "messages":[{"role":"user","content":"Reply with exactly: ok"}], "max_tokens": 512}`.
  **`max_tokens` must be at least 512**: a 50-token cap truncated a reasoning model during the
  design probe and produced a false failure.
  Timeout 60 s. On HTTP 401 with `bearer`, retry once with the custom `x-api-key` header and report
  which one worked in `usedHeader`.
  `reply` is the assistant `content`, trimmed to 200 chars; when `content` is empty but
  `reasoning_content` is present, `reply` is `null` and `error` is
  `"model menjawab hanya dengan reasoning (kemungkinan max_tokens terlalu kecil)"`.
- Errors never include the key. A redaction helper strips any substring equal to the key from the
  error text before returning it.

- [ ] **Step 1: Write the failing tests** against a local `tiny_http` (add as a dev-dependency) test
  server, so no real network call is made:
1. `fetch_models_parses_data_ids`.
2. `test_model_reports_ok_and_latency`: server returns a normal completion; `ok: true`, `reply == "ok"`, `latencyMs > 0`.
3. `test_model_retries_with_custom_header_on_401`: the server 401s on `Authorization` and 200s on `x-api-key`; assert `ok: true` and `usedHeader: "custom"`.
4. `test_model_reports_reasoning_only_reply`: a response with empty `content` and non-empty `reasoning_content` yields `ok: true`, `reply: null` and the reasoning error message.
5. `test_model_error_never_contains_the_key`: the server returns a 500 whose body echoes the key; assert the returned `error` does not contain it.
6. `test_model_reports_http_status_on_failure`: a 404 gives `ok: false` and `status: 404`.
- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes** — 6 tests pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(app): add provider model fetch and test probe"`

---

### Task 5: Commands

**Files:** Modify `src-tauri/src/lib.rs`

**Interfaces:** All commands in the Command API, returning `Result<T, String>`.
`secret_reveal` must carry a doc comment stating it is the only plaintext path and is triggered by
an explicit user action. `secret_migrate_inline` reads the inline key from the config, writes it to
the 0600 file, replaces the config value with the `{file:}` reference, and returns the masked form.

- [ ] **Step 1:** Implement.
- [ ] **Step 2:** Verify — `cargo check --manifest-path src-tauri/Cargo.toml --workspace` exits 0.
- [ ] **Step 3: Commit** — `git commit -m "feat(app): expose provider admin commands"`

---

### Task 6: Model & Provider page

**Files:** Create `src/pages/ProviderOverviewPage.tsx`, `src/components/ProviderTable.tsx`, `src/components/ModelChips.tsx`; modify `src/lib/api.ts`, `src/lib/nav.ts`, `src/App.tsx`

**Interfaces:**
- Matches `docs/design/06-model-provider.png`: a Claude section with model chips (available vs
  previously used), an opencode provider table (provider, auth, model count, example models,
  status), and a Codex section.
- A provider whose `keyInline` is true shows a warning chip `Key masih plaintext` with a
  `Pindahkan ke file 0600` action calling `secret_migrate_inline`.
- Clicking a provider row opens the editor page for it.
- App icons: use `lucide-react` icons, not bundled brand logos.

- [ ] **Step 1:** Implement, enable the nav item.
- [ ] **Step 2:** Verify — `bunx tsc --noEmit`, `bun test`, `bun run build` all clean.
- [ ] **Step 3: Commit** — `git commit -m "feat(ui): add model and provider overview page"`

---

### Task 7: Kelola Provider editor page

**Files:** Create `src/pages/ProviderEditPage.tsx`, `src/components/KeyField.tsx`, `src/components/ModelList.tsx`; modify `src/App.tsx`

**Interfaces:**
- Matches `docs/design/07-kelola-provider.png`: left form (name, id read-only when editing, adapter,
  base URL, enabled switch, header style segmented control, key field), right model list with
  search, `Ambil dari /v1/models`, `Model manual`, per-row `Tes` with its result, and delete.
- `KeyField` shows the masked key, an eye button calling `secret_reveal` that re-masks after 15
  seconds or on blur, plus `Ganti` and `Hapus`. The revealed value is held in component state only,
  never in the zustand store and never logged.
- Adding a model asks for id, display name, context limit and output limit, because a missing limit
  defaults to 0 in opencode.
- Saving shows the backup path that was created, and a line:
  `Sesi opencode yang sedang jalan perlu direstart.`
- A `Pulihkan` menu lists `config_backups` and can restore one after a confirmation.
- Header style help text: `Gateway aki menolak Bearer dan menerima x-api-key.`

- [ ] **Step 1:** Implement.
- [ ] **Step 2:** Verify — `bunx tsc --noEmit`, `bun test`, `bun run build` all clean.
- [ ] **Step 3: Commit** — `git commit -m "feat(ui): add provider editor page"`

---

## Done when

- `cd src-tauri && cargo test` passes, including the 29 new tests from Tasks 1-4.
- `bunx tsc --noEmit`, `bun test`, `bun run build`, `cargo check --workspace` all clean.
- A manual check on a COPY of the real config (never the live one during development) shows
  comments preserved and a backup created.
- One commit per task, no `Co-Authored-By`, nothing pushed.

## Self-Review

- **Spec coverage:** provider CRUD (Task 3, 7), model add/remove/fetch (Task 3, 4, 7), key
  management in 0600 files with a migration wizard (Task 1, 5, 6), connection tests (Task 4, 7),
  comment-preserving atomic config writes with backups (Task 2), the Bearer vs `x-api-key` finding
  (Task 4), the restart note and the limit-defaults-to-0 trap (Task 7).
- **Deliberate gap:** project-level `opencode.json` files are shown as a source but not edited in
  P5. Editing them is a later task.
- **Security review of this plan's own design:** the only plaintext key paths are
  `secret_set` (user typed it), `secret_reveal` (user asked), `secret_migrate_inline` (moving it),
  and the probe request itself. Tests 1.5, 3.5 and 4.5 each assert a key does not leak into a
  masked value, the config text, or an error message.
