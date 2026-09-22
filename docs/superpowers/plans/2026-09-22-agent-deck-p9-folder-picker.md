# Agent Deck P9: Native folder picker

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Choose a project folder by browsing in Finder, not by typing a path.

**Architecture:** Add the Tauri dialog plugin and expose a thin `pickFolder()` wrapper. Every place that currently asks for a directory gets a `Pilih folder…` button that opens the native picker; the text field stays as an editable fallback.

**Tech Stack:** `tauri-plugin-dialog` (Rust, version resolved by `cargo add`) and `@tauri-apps/plugin-dialog` 2.7.3 (JS).

**Spec:** Extends `docs/superpowers/specs/2026-09-22-agent-deck-design.md`.

## Global Constraints

Same as previous phases. Plus:

- The dialog opens only when the user clicks. Never open it automatically on mount.
- A cancelled dialog must leave the existing value untouched — no clearing the field, no error.
- The typed field stays. Some people paste a path; removing that would be a regression.
- UI text Indonesian, code and commit messages English. No `Co-Authored-By`. Never push.

---

### Task 1: Dialog plugin

**Files:** Modify `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs`, `src-tauri/capabilities/default.json`, `package.json`

**Interfaces:**
- `cargo add tauri-plugin-dialog` inside `src-tauri`; register with
  `.plugin(tauri_plugin_dialog::init())` before `.setup(...)`, alongside the other plugins.
- `bun add @tauri-apps/plugin-dialog`.
- Add the `dialog:allow-open` permission (or `dialog:default`) to the capability file. Without it the
  picker fails silently at runtime, so this step is not optional.
- Record the resolved Rust crate version in the commit message.

- [ ] **Step 1:** Implement.
- [ ] **Step 2:** Verify — `cargo check --manifest-path src-tauri/Cargo.toml --workspace` exits 0 and the full Rust suite still passes.
- [ ] **Step 3: Commit** — `git commit -m "feat(app): add tauri dialog plugin for folder picking"`

---

### Task 2: `pickFolder` helper

**Files:** Create `src/lib/pick.ts`, `src/lib/pick.test.ts`

**Interfaces:**
- `pickFolder(opts?: { title?: string; defaultPath?: string }): Promise<string | null>` — wraps
  `open({ directory: true, multiple: false, title, defaultPath })` from `@tauri-apps/plugin-dialog`.
  It returns the chosen absolute path, or `null` when the user cancels.
- It must normalise the result: the plugin can return `string`, `string[]` or `null` depending on
  options and version. Take the first entry of an array, and treat an empty array or an empty string
  as a cancel.
- Errors from the plugin are caught and surfaced as a rejected promise with a readable message; they
  must not crash the calling component.

- [ ] **Step 1: Write the failing tests** in `src/lib/pick.test.ts`, mocking the plugin module with
  `mock.module("@tauri-apps/plugin-dialog", ...)` from `bun:test`:
1. `returns_the_selected_path`: the mock resolves `"/Users/yolk/Dev/noor"`; the helper returns it.
2. `returns_null_on_cancel`: the mock resolves `null`; the helper returns `null`.
3. `takes_the_first_entry_of_an_array`: the mock resolves `["/a", "/b"]`; the helper returns `"/a"`.
4. `treats_an_empty_array_as_cancel`: the mock resolves `[]`; the helper returns `null`.
5. `treats_an_empty_string_as_cancel`: the mock resolves `""`; the helper returns `null`.
6. `passes_title_and_default_path_through`: assert the mock received the options, including
   `directory: true` and `multiple: false`.
- [ ] **Step 2: Run to verify it fails** — `bun test`.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes** — 6 tests pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(ui): add native folder picker helper"`

---

### Task 3: Use the picker everywhere a folder is entered

**Files:** Modify `src/components/ProjectForm.tsx`, `src/components/NewSessionDialog.tsx`

**Interfaces:**
- **Project form:** the path field becomes a row: the text input plus a `Pilih folder…` button with
  the lucide `folder-open` icon. Clicking it opens the picker with
  `title: "Pilih folder project"` and `defaultPath` set to the current field value when it is a
  non-empty absolute path, otherwise the user's home. On a result, the field is filled AND, when the
  name field is still empty or was auto-filled from a previous pick, the name is set to the folder's
  last segment. On cancel, nothing changes.
- **New session dialog:** the free-text `Folder bebas` field gets the same button, titled
  `Pilih folder kerja`.
- The buttons are disabled while a pick is in flight, so a double click cannot open two dialogs.
- A picker error renders inline as `Gagal membuka Finder` plus the message; it never blanks the form.
- Keep layout, spacing and colours as they are; this adds a button, it does not restyle the form.

- [ ] **Step 1:** Implement.
- [ ] **Step 2:** Verify — `bunx tsc --noEmit`, `bun test`, `bun run build` all clean.
- [ ] **Step 3: Commit** — `git commit -m "feat(ui): browse for a folder instead of typing a path"`

---

## Done when

- The whole Rust suite still passes; `bun test` passes with 6 new tests.
- `bunx tsc --noEmit`, `bun run build`, `cargo check --workspace` all clean.
- One commit per task, no `Co-Authored-By`, nothing pushed.
- Manual check by the reviewer: the button really opens Finder and the chosen path lands in the field.

## Self-Review

- **Coverage:** both places that take a directory (project form, new-session dialog) get the native
  picker, with the typed field kept as a fallback.
- **Risk covered by Task 1:** forgetting the capability permission makes the picker fail silently.
  That is called out explicitly rather than left to be discovered at runtime.
- **Not covered:** the auto-index and the provider editor do not take directories, so they are
  unchanged.
