# Agent Deck P14: Animated thinking indicator and a hideable activity feed

> Executed by the controller (Claude), because motion has to be watched to be judged and a delegated agent cannot see its own output.

**Goal:** A session that is thinking looks alive and says how long it has been thinking. The activity feed can be collapsed when the cards need the room.

**Spec:** Two requests from the user on the running app, 2026-09-22. Reference for the thinking line: `brainless.swerdlow.dev`, a shadcn registry that mirrors the Claude Code / Codex / Grok interfaces. Its thinking row pairs a marker with elapsed time (`Thinking… (0s · ↑ 0 tokens · esc to interrupt)`) rather than showing a bare spinner — the useful part is the elapsed time, and that is what we copy.

**Depends on:** P12, which adds `quietMs` to `Session`. The thinking duration is read from it.

## Constraints

- No new dependency. Animation is CSS keyframes in `src/index.css`, not a library.
- **Respect `prefers-reduced-motion`.** Under that setting the dots hold at a steady opacity
  instead of pulsing. Motion that cannot be turned off is an accessibility failure, and it costs
  four lines to do right.
- Animate only what is genuinely in progress: `thinking` and a running `tool`. A card that is
  `Diam` or `Selesai` stays still, or the board becomes a christmas tree.
- Keep the existing colour tokens and spacing. This is a small addition, not a redesign.
- Feed visibility is a per-viewer UI preference, so it lives in `localStorage`, not in the settings
  table. Every read and write is wrapped in try/catch, because storage can throw or come back empty.
- UI text Indonesian, code and commit messages English. No `Co-Authored-By`. Never push.

## Changes

### 1. Keyframes — `src/index.css`

```css
@keyframes ad-dot {
  0%, 80%, 100% { opacity: 0.2; }
  40% { opacity: 1; }
}

.ad-dot { animation: ad-dot 1.4s ease-in-out infinite; }
.ad-dot:nth-child(2) { animation-delay: 0.16s; }
.ad-dot:nth-child(3) { animation-delay: 0.32s; }

@media (prefers-reduced-motion: reduce) {
  .ad-dot { animation: none; opacity: 0.6; }
}
```

### 2. `ThinkingDots` — `src/components/ThinkingDots.tsx`

A three-dot row: `<span className="ad-dot size-1 rounded-full bg-current" />` three times, wrapped in
`inline-flex items-center gap-1`, inheriting `currentColor` so the caller picks the colour.

### 3. Activity line — `src/components/SessionCard.tsx`

- `thinking`: label becomes `Berpikir` + `ThinkingDots` + the elapsed time from `quietMs`, rendered
  with the existing `formatDuration`, e.g. `Berpikir ··· 1 mnt`. Under a minute, show seconds:
  add `formatShort(ms)` to `src/lib/format.ts` returning `"12 dtk"` below 60 s and delegating to
  `formatDuration` above it.
- `tool` while running: the tool name keeps its current styling but gains a small `Loader`
  (lucide `loader-circle`) with `animate-spin` at 12 px, plus the elapsed tool time when P12's
  `toolRunningMs` is non-null.
- `done`, `waiting`, `idle`: unchanged, no motion.
- The busy colour (`--color-busy`) carries the thinking state; the waiting colour stays reserved for
  sessions that need a human.

### 4. Collapsible feed — `src/pages/LivePage.tsx`, `src/components/ActivityFeed.tsx`

- `LivePage` owns `const [feedOpen, setFeedOpen] = useState(readFeedOpen())`, where `readFeedOpen`
  reads `localStorage["ad.feedOpen"]` inside try/catch and defaults to `true`.
- `ActivityFeed` gains `onClose: () => void` and renders a `panel-right-close` icon button in its
  header, with `title="Sembunyikan aktivitas"`.
- When `feedOpen` is false the aside is not rendered, and a slim vertical strip button appears on the
  right edge of the content area: a `panel-right-open` icon plus the count of events, with
  `title="Tampilkan aktivitas"`. Clicking it restores the feed.
- Every write to `localStorage` is wrapped in try/catch and failure is ignored — the toggle must
  still work for the session even if storage is blocked.

### 5. `formatShort` — `src/lib/format.ts` and its test

```ts
export const formatShort = (ms: number): string =>
  ms < 60_000 ? `${Math.max(0, Math.floor(ms / 1000))} dtk` : formatDuration(ms);
```

Tests to add in `src/lib/format.test.ts`:
1. `formatShort` returns `"12 dtk"` for 12_300, `"0 dtk"` for 400, and `"0 dtk"` for a negative input.
2. `formatShort` delegates above a minute: 90_000 gives the same string as `formatDuration(90_000)`.

## Verification

- [ ] `bunx tsc --noEmit`, `bun test`, `bun run build` all clean.
- [ ] Run the app and watch a real thinking session: the dots pulse, the elapsed time counts up.
- [ ] Collapse the feed, confirm the cards reflow to more columns, reopen it, restart the app and
      confirm the choice stuck.
- [ ] Toggle macOS "Reduce motion" and confirm the dots stop animating.
- [ ] Commit as `feat(ui): animate the thinking state and let the activity feed collapse`.
