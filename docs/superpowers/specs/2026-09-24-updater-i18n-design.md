# Auto-updater + i18n (EN/ID) — Design

Date: 2026-09-24

## Goals

1. Agent Deck updates itself from GitHub Releases: checks automatically, prompts the user, downloads, relaunches.
2. UI is available in English and Indonesian. English is always the default; the user switches manually.

## Non-goals

- Silent/background auto-install.
- OS-locale detection.
- Languages beyond `en` and `id` (the structure allows adding more later).
- Translating data coming from agents/sessions (model names, log text, file paths).

---

## Part 1 — i18n

### Approach

Custom typed dictionary, no new dependency.

```
src/i18n/
  en/<area>.ts   // source of truth per area: export const settings = { "settings.title": "Settings", ... } as const
  en/index.ts    // export const en = { ...common, ...settings, ... } as const
  id/<area>.ts   // export const settings: Record<keyof typeof enSettings, string> = { ... }
  id/index.ts    // export const id: Record<keyof typeof en, string> = { ...common, ... }
  index.ts       // store, useT(), t(), locale(), Lang type
```

Per-area files let parallel workers edit disjoint files.

- Keys are flat dotted strings grouped by area: `common.*`, `settings.*`, `project.*`, `session.*`, `terminal.*`, `provider.*`, `pricing.*`, `billing.*`, `live.*`, `timeline.*`, `token.*`, `savings.*`, `activity.*`, `notify.*`, `update.*`.
- Each `id/<area>.ts` typed as `Record<keyof typeof en, string>` → a missing or extra key is a compile error (`tsc` in `bun run build`).
- Interpolation: `{name}` placeholders, `t("update.available", { version: "0.3.0" })`. Simple regex replace; missing param leaves the placeholder visible.
- Plurals: no plural engine. Where needed, use two keys (`*.one` / `*.other`) and pick in code. Both EN and ID only need one/other.

### Runtime

- `useLang` Zustand store: `{ lang: Lang, setLang(lang) }`.
- Initial value read synchronously from `localStorage["agent-deck.lang"]`; anything other than `"en"`/`"id"` → `"en"`. Reads/writes wrapped in try/catch.
- `setLang` writes localStorage and sets `document.documentElement.lang`.
- `useT()` returns `t(key, params?)` bound to the current lang; components re-render on change.
- Non-React code (e.g. `src/lib/notify.ts`) uses `t()` that reads `useLang.getState()`.
- Date/number formatting that is locale-dependent uses `lang === "id" ? "id-ID" : "en-US"` via a helper `locale()` exported from `src/i18n/index.ts`.
- Unit formatters in `src/lib/format.ts` and `src/lib/perf.ts` follow the active language: EN `18.4M`, `448.7K`, `26 min`, `1 h 4 min`, `12 s`; ID keeps today's output `18,4 jt`, `448,7 rb`, `26 mnt`, `1 j 4 mnt`, `12 dtk`. Hardcoded `"id-ID"` in `toLocale*`/`Intl` calls is replaced by `locale()`.

### Settings UI

`SettingsPopover.tsx` gets a **Language** row: segmented control `English | Bahasa Indonesia`. Language names are shown in their own language, not translated.

### String migration

- Every user-facing string in `src/**/*.tsx|ts` (JSX text, `title`, `placeholder`, `aria-label`, toast/notification text, confirm dialog text) moves to `en.ts`.
- Strings currently written in Indonesian (~35 files across `src/components`, `src/pages`, `src/lib`) get an English version in `en.ts` and the original meaning in `id.ts`.
- Not translated: brand/product names, model ids, CLI commands, keyboard shortcut labels, units like `tok/s`.
- Tests that assert on visible text keep working because the default lang in tests is `en`; existing tests asserting Indonesian text set `useLang.setState({ lang: "id" })` in `beforeEach` and gain matching English cases.

---

## Part 2 — Auto-updater

### Dependencies

- Rust: `tauri-plugin-updater`, `tauri-plugin-process` (desktop only, `#[cfg(desktop)]`).
- JS: `@tauri-apps/plugin-updater`, `@tauri-apps/plugin-process`.
- Registered in `src-tauri/src/lib.rs` next to the existing plugins.
- Capability `src-tauri/capabilities/default.json`: add `updater:default`, `process:allow-restart`.

### Config (`tauri.conf.json`)

```json
"bundle": { "createUpdaterArtifacts": true, ... },
"plugins": {
  "updater": {
    "pubkey": "<public key from tauri signer>",
    "endpoints": [
      "https://github.com/yolkmonday/agent-deck/releases/latest/download/latest.json"
    ]
  }
}
```

### Signing key

- Generated with `bunx tauri signer generate -w ~/.tauri/agent-deck.key` using a random password.
- Private key + password → GitHub secrets `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (via `gh secret set`).
- The password is saved next to the key (`~/.tauri/agent-deck.key.password`, mode 600) so future local builds can sign. The key never enters the repo.
- If the key is lost, installed apps can never verify a new update → keep a backup.

### CI (`.github/workflows/release.yml`)

- Uncomment the two `TAURI_SIGNING_*` env lines.
- tauri-action (v1) with `createUpdaterArtifacts` uploads per-platform signed bundles + `latest.json` to the release.
- `releaseDraft: true` stays. `releases/latest` only resolves published releases, so users get an update only after the draft is published manually. This is intended: publishing is the release gate.

### Frontend

```
src/lib/updater.ts          // checkForUpdate(), installUpdate(onProgress)
src/store/updater.ts        // Zustand: status, version, notes, progress, error, lastChecked
src/components/UpdateBanner.tsx
```

State machine (`status`):

```
idle → checking → (none | available | error)
available → downloading(progress 0..1) → installing → relaunch
available → dismissed (Later)
```

- **Auto-check:** 10 s after app start, then every 6 h while the app runs. Not in `import.meta.env.DEV` (dev builds have no updater artifacts).
- **Errors from auto-check** (offline, 404 before the first release with `latest.json`) are silent: logged with `console.warn`, status goes back to `idle`. Errors from a manual check are shown in Settings.
- **UpdateBanner:** thin bar at the top of the app shell when `status === "available"`: "Version {version} is available" + **Install** + **Later**. During download it shows a progress bar and hides the buttons. After install it calls `relaunch()`.
- **Later** hides the banner for that version until the next app start. The periodic check does not re-show a dismissed version in the same session.
- **SettingsPopover:** shows `Version {current}` (from `getVersion()`), a **Check for updates** button showing the result inline ("You're up to date" / "Version X available" + Install / error text), and the Language row.
- All banner/settings strings go through i18n (`update.*`).

### Rollout caveat

v0.2.1 and older have no updater. The first release that includes this feature (v0.3.0) must be installed manually once. Every release after that updates automatically.

---

## Testing

- **i18n unit tests (`bun test`):** interpolation, fallback to `en` for an invalid stored value, `setLang` persists, and a parity test that `Object.keys(id)` equals `Object.keys(en)` (runtime backup of the type check). Also a test that no value in `en.ts` is an empty string.
- **Updater store tests:** state transitions with `check()` mocked: none, available, error on auto-check (silent), error on manual check (shown), and dismissed version is not re-shown.
- **Build:** `bun run build` (tsc) and `cargo check` pass.
- **Manual E2E:** after v0.3.0 is installed, publish v0.3.1 → the app shows the banner → Install → relaunches on v0.3.1.

## Files touched (summary)

- New: `src/i18n/**`, `src/lib/updater.ts`, `src/store/updater.ts`, `src/components/UpdateBanner.tsx`, tests.
- Modified: ~50 TS/TSX files (string extraction), `SettingsPopover.tsx`, app shell (banner mount), `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `package.json`, `.github/workflows/release.yml`.
