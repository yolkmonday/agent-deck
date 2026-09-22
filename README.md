# Agent Deck

A local desktop dashboard (Tauri) that shows what every coding agent on your machine is doing right now, and how much it costs — in one place, instead of 3-5 terminals.

Covers **Claude Code**, **opencode**, and **Codex CLI**. Also tracks **rtk** and **lean-ctx** token savings.

All agent data is read-only — Agent Deck never writes to another agent's session, settings, or transcripts.

## Features

- **Live Board** — session cards per agent (status: busy/waiting/idle, current tool, tokens, cost), KPI row, activity feed
- **Token & Cost** — per-day stacked bars by agent, per-model breakdown
- **Timeline** — per-session lanes of tool spans with a detail panel
- **Token Savings** — rtk / lean-ctx savings per day, top commands
- **Terminal** — embedded terminal tabs to jump into any session
- **Model & Provider** — opencode provider/model management (add provider, mask keys, test models)
- **Recover** — nudge, restart, or kill a stuck agent process

Mockups are in [`docs/design/`](docs/design). Architecture notes: [`docs/superpowers/specs`](docs/superpowers/specs).

## Prerequisites

- [Bun](https://bun.sh) 1.x
- [Rust](https://www.rust-lang.org/tools/install) (stable toolchain, for the Tauri backend)
- Tauri's platform dependencies — see the [Tauri prerequisites guide](https://v2.tauri.app/start/prerequisites/) for your OS (Xcode CLT on macOS, WebView2 on Windows, `libwebkit2gtk` etc. on Linux)

## Running

```bash
bun install
bun run tauri dev
```

This launches the desktop app with hot reload. On first run Tauri compiles the Rust backend, which takes a while.

To run only the frontend in a browser (no Tauri backend, limited functionality):

```bash
bun run dev
```

## Building

```bash
bun run tauri build
```

Produces a platform-native installer/bundle under `src-tauri/target/release/bundle/`.

## Tests

```bash
bun test              # frontend
cd src-tauri && cargo test   # backend (collector + commands)
```

## Project structure

```
agent-deck/
  src-tauri/
    crates/collector/   # lib: reads agent session data, pricing, storage (no Tauri dependency)
    src/                 # Tauri app: commands, events, terminal, opencode admin
  src/                   # React + Vite + TypeScript + Tailwind + shadcn/ui
  docs/                  # design mockups and specs
```

## Tech stack

React 19, Vite, TypeScript, Tailwind CSS v4, Zustand, TanStack Query, xterm.js, recharts — on a Tauri v2 (Rust) backend.

## License

[MIT](LICENSE)
