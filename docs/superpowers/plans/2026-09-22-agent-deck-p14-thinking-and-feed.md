# Agent Deck P14: Animated thinking indicator and a hideable activity feed

> Executed by the controller (Claude), because motion has to be watched to be judged and a delegated agent cannot see its own output.

**Goal:** A session that is thinking looks alive and says how long it has been thinking. The activity feed can be collapsed when the cards need the room.

**Spec:** Two requests from the user on the running app, 2026-09-22. Reference for the thinking line: `brainless.swerdlow.dev`, a shadcn registry that mirrors the Claude Code / Codex / Grok interfaces. Its thinking row pairs a marker with elapsed time (`Thinking… (0s · ↑ 0 tokens · esc to interrupt)`) rather than showing a bare spinner — the useful part is the elapsed time, and that is what we copy.

**Depends on:** P12, which adds `quietMs` and `toolRunningMs` to `Session`. Durations are read from those, never from a component-local stopwatch.

## Constraints

- No new npm dependency. The animation comes from the brainless component, which ships its own CSS.
- **Respect `prefers-reduced-motion`.** The brainless component already does; keep that intact
  rather than overriding its styles. Motion that cannot be turned off is an accessibility failure.

- Animate only what is genuinely in progress: `thinking` and a running `tool`. A card that is
  `Diam` or `Selesai` stays still, or the board becomes a christmas tree.
- Keep the existing colour tokens and spacing. This is a small addition, not a redesign.
- Feed visibility is a per-viewer UI preference, so it lives in `localStorage`, not in the settings
  table. Every read and write is wrapped in try/catch, because storage can throw or come back empty.
- UI text Indonesian, code and commit messages English. No `Co-Authored-By`. Never push.

## Changes

### 1. The thinking component comes from brainless — DONE

`bunx shadcn@latest add @brainless/claude-thinking` installed
`src/components/brainless/claude/claude-thinking.tsx`. It is self-contained: no npm dependency, no
`cn()`, its own `<style>` block, `role="status"` + `aria-live="polite"`, and it already honours
`prefers-reduced-motion`.

Two local additions were made to it, because the upstream defaults are dishonest **in a dashboard**
(they are perfectly fine in the terminal replica it was written for):

- The token counter is **estimated**, not measured: upstream computes `secs * 137`. Agent Deck shows
  real token counts two lines below, so a fabricated number next to a real one would poison trust in
  both. Pass `showTokens={false}`.
- `esc to interrupt` is hardcoded upstream and is **false here** — pressing esc in the dashboard does
  nothing to a session it does not own. A new `hint?: string | null` prop was added; pass
  `hint={null}` on the card.
- A new `elapsedMs?: number` prop was added so the measured duration from the backend replaces the
  component's own stopwatch, which would otherwise restart on every re-render.

### 2. Activity line — `src/components/SessionCard.tsx`

- `thinking`: render `<ClaudeThinking verbs={["Berpikir"]} showTokens={false} hint={null} elapsedMs={s.quietMs} />`.
  A single verb, not the upstream rotation (`Levitating`, `Schlepping`, `Percolating`): whimsy reads
  well in a terminal, but on a status board it implies the dashboard knows what the agent is doing
  when it does not.
- `tool` while running: keep the current styling, add the elapsed tool time from P12's
  `toolRunningMs` via `formatShort`. No extra spinner — the status pill already says `Sibuk`.
- `done`, `waiting`, `idle`: unchanged, no motion.
- Colour: the component hardcodes terracotta `#cd694a`, which is close to our `--color-claude`
  (`#E8825C`). Leave it; it reads as the Claude brand colour and matches the agent icon beside it.

### 3. Collapsible feed — `src/pages/LivePage.tsx`, `src/components/ActivityFeed.tsx`

- `LivePage` owns `const [feedOpen, setFeedOpen] = useState(readFeedOpen())`, where `readFeedOpen`
  reads `localStorage["ad.feedOpen"]` inside try/catch and defaults to `true`.
- `ActivityFeed` gains `onClose: () => void` and renders a `panel-right-close` icon button in its
  header, with `title="Sembunyikan aktivitas"`.
- When `feedOpen` is false the aside is not rendered, and a slim vertical strip button appears on the
  right edge of the content area: a `panel-right-open` icon plus the count of events, with
  `title="Tampilkan aktivitas"`. Clicking it restores the feed.
- Every write to `localStorage` is wrapped in try/catch and failure is ignored — the toggle must
  still work for the session even if storage is blocked.

### 4. `formatShort` — `src/lib/format.ts` and its test

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
