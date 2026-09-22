# Agent Deck P7: Iconify icons, bundled offline

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Real vendor marks for every agent, provider and model shown in the app, plus consistent tool icons — all bundled into the binary so the app works with no network.

**Architecture:** One `icons` module registers two Iconify collections at startup via `addCollection`, so no icon is ever fetched from a CDN. A single `AgentIcon` / `ProviderIcon` / `ModelIcon` component family resolves a name (agent, provider id, model id) to a verified icon, with a deterministic fallback. Existing `lucide-react` usage stays as it is; this phase does not rewrite it.

**Tech Stack:** `@iconify/react` 6.0.2, `@iconify-json/simple-icons` 1.2.97, `@iconify-json/lucide` 1.2.135. All MIT/CC0.

**Spec:** Extends `docs/superpowers/specs/2026-09-22-agent-deck-design.md`. Mockups: `docs/design/06-model-provider.png`, `07-kelola-provider.png`.

## Global Constraints

Same as previous phases. Plus, specific to this one:

- **No runtime network access for icons.** `@iconify/react` fetches from `api.iconify.design` by
  default; that is forbidden here. Both collections are registered with `addCollection` before the
  first render. A test proves the registration happens.
- Every icon name used MUST be one of the verified names in the table below. Do not invent a name,
  do not guess a variant, and do not add a name that is not listed. An unverified guess renders as
  an empty box at runtime, which the type system will not catch.
- Do not delete the existing `lucide-react` imports. Both libraries coexist: `lucide-react` for
  interface icons already in place, Iconify for vendor marks and any new icon.
- UI text Indonesian, code and commit messages English. No `Co-Authored-By`. Never push.

## Verified icon names

These were each verified against `https://api.iconify.design/<prefix>.json?icons=<name>`.

| Key | Icon | Notes |
|---|---|---|
| Claude / Anthropic agent | `simple-icons:claude` | |
| Anthropic (wordmark) | `simple-icons:anthropic` | |
| OpenAI / Codex | `simple-icons:openai` | marked `hidden` in the collection but resolves normally |
| DeepSeek | `simple-icons:deepseek` | |
| Moonshot AI | `simple-icons:moonshotai` | |
| Kimi | `simple-icons:kimi` | |
| Qwen | `simple-icons:qwen` | |
| MiniMax | `simple-icons:minimax` | |
| Mistral | `simple-icons:mistralai` | `mistral` alone does NOT exist |
| Google Gemini | `simple-icons:googlegemini` | `gemini` alone does NOT exist |
| Ollama | `simple-icons:ollama` | |
| opencode | `simple-icons:opencode` | a plain square mark; keep it but see Task 2 |

**No icon exists** for Zhipu / GLM / ChatGLM. Those must use the generic fallback, not a
look-alike. Do not substitute another vendor's mark.

## Icon mapping rules

- **Agent** (`claude` | `opencode` | `codex`) → `simple-icons:claude` | `simple-icons:opencode` |
  `simple-icons:openai`.
- **Model id** → matched by substring, first match wins, checked in this order so longer brands win:
  `claude` → claude; `gpt`/`codex`/`openai` → openai; `deepseek` → deepseek; `kimi` → kimi;
  `moonshot` → moonshotai; `qwen` → qwen; `minimax` → minimax; `mistral` → mistralai;
  `gemini` → googlegemini; `ollama` → ollama; `glm`/`zhipu`/`chatglm` → **fallback**.
- **Provider id** (opencode provider ids such as `aki`, `kn`, `oa`, `sumo`, `deepseek`, `openai`,
  `anthropic`) → `anthropic` → `simple-icons:anthropic`; `openai` → `simple-icons:openai`;
  `deepseek` → `simple-icons:deepseek`; everything else → **fallback**.
- **Fallback** is a lucide `cpu` glyph in a coloured tile whose colour is derived deterministically
  from the key (hash into the six palette colours), with the first two letters of the key. The same
  key always produces the same colour.

---

### Task 1: Offline icon registry

**Files:**
- Create: `src/lib/icons.ts`, `src/lib/icons.test.ts`
- Modify: `package.json`, `src/main.tsx`

**Interfaces:**
- `bun add @iconify/react @iconify-json/simple-icons @iconify-json/lucide`
- `src/lib/icons.ts` exports:
  - `registerIcons(): void` — calls `addCollection` for both collections; safe to call twice
    (guard with a module-level boolean).
  - `agentIcon(agent: Agent): string`
  - `modelIcon(model: string | null): string | null` — `null` means "use the fallback"
  - `providerIcon(providerId: string): string | null`
  - `fallbackTile(key: string): { color: string; initials: string }` — `color` is one of the six
    palette hex values, chosen by a stable hash of `key`; `initials` is the first two alphanumeric
    characters, uppercased (one character when only one exists, `?` when none).
- `src/main.tsx` calls `registerIcons()` once, before the React root renders.

- [ ] **Step 1: Write the failing tests** in `src/lib/icons.test.ts`:
1. `agent_icons_map_to_verified_names`: the three agents map to `simple-icons:claude`, `simple-icons:opencode`, `simple-icons:openai`.
2. `model_icon_matches_by_substring`: `claude-sonnet-5` → claude; `gpt-5.4` → openai; `kn/deepseek-v4-1-flash` → deepseek; `cbai/kimi-k2.6(high)` → kimi; `qwen3-8-max` → qwen.
3. `model_icon_returns_null_for_glm_family`: `glm-5.2`, `zhipu-x`, `chatglm-4` all return `null`, because no verified icon exists.
4. `model_icon_returns_null_for_unknown_and_null_input`: `"totally-unknown"` and `null` both return `null`.
5. `provider_icon_known_and_unknown`: `anthropic`/`openai`/`deepseek` map to their marks; `aki`, `kn`, `oa` return `null`.
6. `fallback_tile_is_deterministic_and_uses_palette`: the same key twice gives the same colour; the colour is one of the six palette values; `aki` → initials `AK`; `k` → `K`; `"-"` → `?`.
7. `every_mapped_name_is_in_the_verified_list`: collect every string the three mapping functions can return and assert each one is a member of a hardcoded `VERIFIED` array. This is the guard against a typo shipping as an empty box.
- [ ] **Step 2: Run to verify it fails** — `bun test`.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes** — 7 tests pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(ui): add offline iconify registry and icon mapping"`

---

### Task 2: Icon components

**Files:** Create `src/components/BrandIcon.tsx`

**Interfaces:**
- `BrandIcon` props: `{ name: string | null; fallbackKey: string; size?: number; className?: string }`.
  When `name` is non-null it renders `<Icon icon={name} width={size} height={size} />`; when it is
  null it renders the fallback tile (a rounded square with the deterministic colour at 18% opacity,
  a matching border, and the initials in that colour).
- `AgentIcon` props `{ agent: Agent; size?: number }`, `ModelIcon` props `{ model: string | null; size?: number }`,
  `ProviderIcon` props `{ providerId: string; size?: number }` — thin wrappers over `BrandIcon`.
- Default `size` is 16. Icons inherit `currentColor`, so the parent controls the colour; the caller
  sets the agent colour class as it does today.
- **Check `simple-icons:opencode` visually once** by rendering it at 64 px in a scratch page or by
  fetching its SVG path. If it is a plain unbranded square rather than the opencode mark, use the
  fallback tile for opencode instead and say so in the commit message. Do not ship a mark that is
  not actually the product's.

- [ ] **Step 1:** Implement.
- [ ] **Step 2:** Verify — `bunx tsc --noEmit`, `bun test`, `bun run build` all clean.
- [ ] **Step 3: Commit** — `git commit -m "feat(ui): add brand icon components"`

---

### Task 3: Use the icons across the screens

**Files:** Modify `src/components/SessionCard.tsx`, `src/components/ActivityFeed.tsx`, `src/components/ModelTable.tsx`, `src/components/TimelineLane.tsx`, `src/components/TerminalTabs.tsx`, `src/components/NewSessionDialog.tsx`, `src/components/ProviderTable.tsx`, `src/components/ModelChips.tsx`, `src/pages/ProviderOverviewPage.tsx`, `src/pages/ProviderEditPage.tsx`

**Interfaces:**
- Replace the plain coloured dot that currently marks an agent with `AgentIcon` **plus** the
  existing colour, in: session cards, the activity feed, timeline lane labels, terminal tabs, the
  model table's agent column, and the new-session dialog's profile list.
- The model table's model column gets a `ModelIcon` before the model id.
- The provider table and the provider editor header get a `ProviderIcon`.
- Model chips on the provider overview get a `ModelIcon`.
- Keep every existing colour, size and spacing. This task changes what is drawn, not the layout.
- All 23 existing tests plus the 7 new ones must still pass.

- [ ] **Step 1:** Implement.
- [ ] **Step 2:** Verify — `bunx tsc --noEmit`, `bun test`, `bun run build` all clean.
- [ ] **Step 3: Commit** — `git commit -m "feat(ui): show vendor icons across the screens"`

---

### Task 4: Prove the icons are offline

**Files:** Modify `src/lib/icons.test.ts`

**Interfaces:**
- Add a test that imports the two collection JSONs and asserts each verified name is present in its
  collection's `icons` (or `aliases`) object. If a name is missing from the bundled JSON, the app
  would silently fall back to a network fetch, which must fail the build instead.
- Add a test asserting `registerIcons()` is idempotent (calling it twice does not throw).

- [ ] **Step 1: Write the failing tests.**
- [ ] **Step 2: Run to verify they fail** (before the assertion logic exists).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify they pass** — `bun test` green.
- [ ] **Step 5: Commit** — `git commit -m "test(ui): assert every icon is bundled offline"`

---

## Done when

- `bun test` → 23 existing + 9 new tests pass.
- `bunx tsc --noEmit`, `bun run build`, `cargo check --workspace` all clean.
- No icon name outside the verified list appears anywhere in `src/`.
- One commit per task, no `Co-Authored-By`, nothing pushed.

## Self-Review

- **Coverage:** verified names only (the table), offline bundling (Task 1, 4), a deterministic
  fallback for vendors with no mark (Task 1, 2), and usage across all screens (Task 3).
- **Deliberate gap:** GLM / Zhipu has no icon in any Iconify collection, so it uses the fallback
  tile. Substituting a similar-looking mark would be wrong, so it is not done.
- **Licensing note for the user:** simple-icons SVGs are CC0, so free to use with no attribution.
  CC0 does not waive trademark rights, though — the shapes are free, the brands are still owned by
  their vendors. For a personal dashboard that shows which vendor a model came from this is
  ordinary nominative use, but it is worth knowing before the app is distributed.
