# Agent Deck P13: Live board layout polish

> Executed by the controller (Claude) rather than delegated, because it needs a look-run-adjust loop against the real window. A delegated agent here cannot see its own output.

**Goal:** The Live board reads as a tidy grid: nothing wraps that should not, cards in a row share a height, and the card count per row follows the window width.

**Spec:** Fixes defects found by the user on the running app, 2026-09-22.

## Defects (observed, not hypothetical)

Measured from a screenshot of the running app at the default window size, with the activity feed
open (which leaves roughly 560 px for the card column):

1. `LivePage` hardcodes `grid grid-cols-3`, so at that width each card is about 180 px. Everything
   inside is then forced to wrap.
2. KPI value `686,0 jt` breaks across two lines.
3. KPI subtitle `1 sibuk · 0 nunggu · 7 diam` breaks across three lines.
4. The card footer (`flex items-center gap-3`) crams tokens, duration and the action button onto one
   row with no room; the duration `29 j 56 mnt` wraps to three lines.
5. Cards in the same row have different heights, because a two-line title or a present activity
   detail makes one taller.
6. A long project name such as `agent-deck-oc-p11` wraps to two lines.
7. The activity box reserves two lines even when the label is a single word like `Selesai`, wasting
   vertical space on most cards.

## Constraints

- Keep the existing colour tokens and the visual language. This is layout, not a redesign.
- Do not change any logic, any prop shape, or any test.
- Verify by running the app and looking at it, at two window widths.
- No `Co-Authored-By`. Never push.

## Changes

### 1. Responsive grid — `src/pages/LivePage.tsx`

Replace `grid grid-cols-3 gap-4` with a width-driven grid so the column count follows the space:

```tsx
<div className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] items-stretch gap-4">
```

`items-stretch` is what makes every card in a row share the tallest card's height.

### 2. Card structure — `src/components/SessionCard.tsx`

- Root gains `h-full` so the stretched grid cell is actually filled.
- Title: `truncate` with a `title={s.project}` attribute, so a long name shortens with an ellipsis
  instead of wrapping and changing the card's height.
- Activity box: `min-h-0`, and the detail line gets `line-clamp-2` so a long command cannot push the
  card taller than its neighbours.
- Push the footer to the bottom with `mt-auto` on it, so footers line up across a row even when the
  bodies differ.
- Footer becomes two rows instead of one cramped row:
  - top: tokens and cost, `whitespace-nowrap`
  - bottom: duration on the left, the action button on the right, via `justify-between`
- `whitespace-nowrap` on every number so `148,9 jt` and `29 j 56 mnt` never break.

### 3. KPI row — `src/components/KpiRow.tsx`

- Value: `whitespace-nowrap` plus a smaller clamp, `text-[clamp(20px,2.2vw,30px)]`, so a large
  number shrinks rather than wraps.
- Subtitle: `whitespace-nowrap` with `truncate` and a `title` attribute carrying the full text.
- Each KPI column gets `min-w-0`, without which `truncate` does nothing inside a flex row.

### 4. Feed width — `src/components/ActivityFeed.tsx`

The feed is a fixed `w-82.5` (330 px). Keep that at wide sizes but let the cards win when the window
is narrow: `w-[330px] shrink-0 max-[1200px]:w-[260px]`.

## Verification

- [ ] `bunx tsc --noEmit`, `bun test`, `bun run build` all clean.
- [ ] Run the app, screenshot, and confirm: no wrapped KPI number, no wrapped duration, equal card
      heights within each row, long titles ellipsised.
- [ ] Resize narrower and confirm the grid drops to two columns and then one, with nothing clipped.
- [ ] Commit as `fix(ui): tidy the live board grid and card layout`.
