# Agent Deck P20: Micro-interactions — make it feel fast

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** The app stops feeling stiff. Every interactive surface answers the pointer, presses feel pressed, and things that appear do so with a short, purposeful motion.

**Architecture:** Motion tokens in `src/index.css` first, then a small set of utility classes applied consistently, then entrance motion for the surfaces that currently pop in. No animation library.

## The audit, measured

Counted across `src/`, excluding tests:

| Measure | Count |
|---|---|
| Buttons | 69 |
| `hover:` states | 43 |
| **Hover states with a transition** | **0** |
| Press (`active:`) feedback | 2 |
| Motion tokens (`--ease-*`, `--duration-*`) in CSS | 0 |

That is the whole diagnosis: **every hover in the app is an instant snap.** Colour jumps with no
transition read as cheap and unresponsive, which is exactly the "kaku" the user described. The
files with the most unanimated interactive elements are `ModelList.tsx` (13), `ProviderEditPage.tsx`
(10), `NewSessionDialog.tsx` (7) and `ModelPickerDialog.tsx` (7).

## Findings, by leverage

| # | Severity | Finding | Fix |
|---|---|---|---|
| 1 | HIGH | 43 hover states, none animated | One transition utility applied to every interactive element |
| 2 | HIGH | No motion tokens, so every future addition invents its own | Define `--ease-out-quart` and three durations |
| 3 | MEDIUM | 67 of 69 buttons have no press feedback | A shared `:active` scale |
| 4 | MEDIUM | Five modals/dialogs mount instantly | Overlay fade + panel fade-and-rise |
| 5 | MEDIUM | Session cards and list rows pop in | A short fade-and-rise on mount |
| 6 | LOW | Switching pages is an instant swap | A 120 ms fade on the page container |

## Global Constraints

- **`ease-out`, never `ease-in`, for anything entering or responding to the pointer.** `ease-in`
  starts slow, which is what makes UI feel sluggish. `ease-in` is only correct for something
  leaving.
- **Durations stay short.** Hover and press are 120-150 ms. A modal is 200 ms. Nothing in this app
  earns more than 250 ms. Long durations are the other half of feeling laggy.
- **Animate only `transform`, `opacity`, `color`, `background-color`, `border-color`.** Never
  `transition: all` — it animates layout properties and drops frames.
- **Respect `prefers-reduced-motion`** with a single global rule, not per component.
- No new dependency. No layout changes: this phase changes how things move, not where they are.
- UI text Indonesian, code and commit messages English. No `Co-Authored-By`. Never push.

---

### Task 1: Motion tokens and shared utilities

**Files:** Modify `src/index.css`

Add to the `@theme` block:

```css
  --ease-out-quart: cubic-bezier(0.25, 1, 0.5, 1);
  --duration-fast: 120ms;
  --duration-base: 160ms;
  --duration-slow: 220ms;
```

Then, after the theme block:

```css
/* Every interactive surface answers the pointer in the same way. */
.ad-interactive {
  transition-property: color, background-color, border-color, opacity, box-shadow;
  transition-duration: var(--duration-base);
  transition-timing-function: var(--ease-out-quart);
}

/* Presses read as pressed. Transform only, so it never triggers layout. */
.ad-press {
  transition: transform var(--duration-fast) var(--ease-out-quart);
}
.ad-press:active:not(:disabled) {
  transform: scale(0.97);
}

@keyframes ad-rise {
  from { opacity: 0; transform: translateY(4px); }
  to   { opacity: 1; transform: none; }
}
@keyframes ad-fade {
  from { opacity: 0; }
  to   { opacity: 1; }
}
.ad-rise  { animation: ad-rise var(--duration-slow) var(--ease-out-quart) both; }
.ad-fade  { animation: ad-fade var(--duration-base) var(--ease-out-quart) both; }

/* One global opt-out rather than a check in every component. */
@media (prefers-reduced-motion: reduce) {
  .ad-interactive, .ad-press { transition: none; }
  .ad-press:active:not(:disabled) { transform: none; }
  .ad-rise, .ad-fade { animation: none; }
}
```

- [ ] **Step 1:** Add the tokens and utilities.
- [ ] **Step 2:** Verify — `bun run build` exits 0 and the classes appear in the built CSS
      (`grep ad-interactive dist/assets/*.css`).
- [ ] **Step 3: Commit** — `git commit -m "feat(ui): add motion tokens and interaction utilities"`

---

### Task 2: Every interactive element answers the pointer

**Files:** every file under `src/components/` and `src/pages/` that contains `cursor-pointer` or `hover:`

For each `<button>`, clickable row, nav item, tab, chip and segmented option:

- add `ad-interactive` to its `className`
- add `ad-press` to anything that is a real button the user clicks to act (not to a whole row that
  merely opens a panel)
- where an element has `cursor-pointer` but **no** `hover:` state at all, add one: a button gains
  `hover:text-fg` or `hover:border-fg-3`, a row gains `hover:bg-surface-2`. An interactive thing
  that does not respond to the pointer is the same defect as one that responds instantly.

Do NOT change colours, spacing, sizes or layout. This task only adds motion and the missing hover
state.

- [ ] **Step 1:** Apply across all files. Work file by file; `ModelList.tsx`, `ProviderEditPage.tsx`,
      `NewSessionDialog.tsx` and `ModelPickerDialog.tsx` hold the most.
- [ ] **Step 2:** Verify — `bunx tsc --noEmit`, `bun test`, `bun run build` all clean, and
      `grep -rn "hover:" src/ | grep -vc "ad-interactive"` returns 0.
- [ ] **Step 3: Commit** — `git commit -m "feat(ui): animate every hover and press"`

---

### Task 3: Entrances

**Files:** `src/components/GroupModal.tsx`, `PricingDialog.tsx`, `BillingDialog.tsx`, `ModelPickerDialog.tsx`, `SettingsPopover.tsx`, `SessionCard.tsx`, `src/pages/ActivityPage.tsx`, `src/App.tsx`

- **Modals and dialogs**: the scrim gets `ad-fade`; the panel gets `ad-rise`. Nothing else changes.
- **Session cards**: the card root gets `ad-rise`, so a session that appears eases in rather than
  popping. Because React keys them by session id, an existing card does not re-run the animation.
- **Activity rows**: the newest row gets `ad-rise`; do NOT stagger the whole list, which would make
  every poll look like a reload.
- **Page container** in `App.tsx`: the element wrapping the current page gets `ad-fade`, keyed by
  the page key so switching re-runs it.

- [ ] **Step 1:** Apply.
- [ ] **Step 2:** Verify — `bunx tsc --noEmit`, `bun test`, `bun run build` all clean.
- [ ] **Step 3: Commit** — `git commit -m "feat(ui): ease in modals, cards and page switches"`

---

## Done when

- `bunx tsc --noEmit`, `bun test`, `bun run build`, `cargo check --workspace` all clean.
- `grep -rn "hover:" src/ | grep -vc "ad-interactive"` returns 0.
- `grep -rn "transition: all\|transition-all" src/` returns nothing.
- Three commits exist, no `Co-Authored-By`, nothing pushed.

## Self-Review

- **The diagnosis is measured, not felt:** 43 hover states with 0 transitions is the finding, and
  Task 2's verification command proves it was fixed rather than partially applied.
- **Why `ease-out` everywhere:** it starts fast and settles, which reads as responsive.
  `ease-in` on a hover is the classic cause of "sluggish", and the user asked for the opposite.
- **Why so short:** 120-160 ms is below the threshold where motion starts to feel like waiting.
  Making a dashboard feel fast means getting out of the way, not putting on a show.
- **Uncertainty stated honestly:** feel cannot be fully judged from code. The reviewer must run the
  app, hover a few buttons, open a modal and switch pages before calling this done.
