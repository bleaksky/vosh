# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Read This First

`prompt.md` is the master spec for this project. Read it before doing anything. It defines requirements, the locked stack, the full phase plan, allowed and forbidden actions, and writing style rules. This file summarizes the rules that govern how you work and the locked architectural decisions; `prompt.md` is the source of truth for what the project is.

## Locked Stack (Approved Phase 0)

- App shell. Tauri 2.
- Backend. Rust with Tokio for async. rustls for TLS.
- Frontend. TypeScript and React, bundled by Vite.
- Renderer. xterm.js with a custom addon for the trigger and highlight pipeline. Fall back to a custom canvas renderer only if profiling demands it.
- Map. HTML canvas with a layer abstraction. WebGL only if rooms-per-second profiling demands it.
- Scripting. Lua via mlua. QuickJS as a possible secondary later.
- IPC. Tauri commands and events between Rust and the web view, with a typed event bus on top.
- Storage. SQLite via rusqlite for logs, scrollback indexes, and map data. TOML for human edited profile config.
- License. GPL v3.
- CI. GitHub Actions matrix across macOS (arm64 and x86_64), Windows, and Linux.

## Repo Layout

```
mudclient/
  CLAUDE.md            Stack and workflow rules. This file.
  prompt.md            Master spec. Source of truth.
  README.md            Public scope and how to run.
  CONTRIBUTING.md      How to develop, lint, commit.
  LICENSE              GPL v3.
  Cargo.toml           Rust workspace root.
  package.json         Frontend dependencies and scripts.
  vite.config.ts       Frontend bundler config.
  tsconfig.json        TypeScript config.
  index.html           Vite entry point.
  src/                 React frontend.
  src-tauri/           Rust app crate, Tauri config, capabilities.
  crates/              Rust library crates added in later phases (telnet, ansi, gmcp, ...).
  fixtures/            Captured byte streams for parser tests.
  .github/workflows/   CI for the three platforms.
  .githooks/           Project pre-commit hook.
```

## Common Commands

These work once `npm install` and `cargo fetch` have run at least once.

- `npm run dev` runs Vite alone for frontend iteration.
- `npm run tauri dev` runs the full app (Rust backend plus web view).
- `npm run tauri build` builds release binaries for the host platform. Bundling stays disabled until icons land in Phase 11.
- `npm run lint`, `npm run format:check`, `npm run typecheck` cover frontend checks.
- `cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features -- -D warnings` cover Rust checks.
- `cargo test --workspace` runs Rust tests. Single test by name: `cargo test --workspace <test_name>`.

## Workflow Rules (Non-Negotiable)

This is a **phased, approval gated project**. Read, propose, wait for approval, then implement. Never skip phases.

1. Stop at every phase boundary. Each phase ends with a working build, a short demo, and an approval checkpoint. Do not start phase N+1 without explicit approval in the chat.
2. Surface architecture decisions. When two valid implementation paths exist and the choice affects architecture, stop and ask.
3. Stop conditions. Pause for human review when a file would be permanently deleted, a new external service or API needs integration, an error cannot be resolved in two attempts, the task requires changes outside the stated scope, or a phase boundary is reached.
4. Commits are small and single concern. Conventional Commits format.

## Forbidden Actions

- Modifying files outside the working directory.
- Pushing to a remote without explicit approval.
- Deleting files without showing a diff first.
- Adding dependencies that pull in trackers, telemetry, or required cloud services.
- Shipping features that require a server we do not control.
- Adding a feature outside the agreed phase scope for the current session.

## Quality Bars

- Protocol parsers (telnet, ANSI, GMCP) get unit tests against captured byte stream fixtures in `fixtures/`. Seed with samples from Aabahran.
- Integration tests use a fake MUD server fixture.
- Run `cargo clippy`, `cargo fmt`, `eslint`, and `prettier` on every commit. The pre-commit hook in `.githooks/pre-commit` enforces this. Enable it once with `git config core.hooksPath .githooks`.
- Crash reports are local only and opt in.

## Writing Style for User-Visible Text

Applies to README, CONTRIBUTING, settings labels, error messages, commit messages, and any prose the user will read.

- Active voice. Address the user as "you".
- No dashes of any kind in prose, no semicolons, no colons in body sentences, no asterisks for emphasis, no emojis.
- Direct and concise. Vary sentence length for rhythm.
- Concrete and specific over abstract. Definitive statements over conditionals.
- No filler phrases ("it's important to note", "let's explore", "streamline", and similar).

## Phase Status

Phase 0 in progress. Scaffolding committed, CI in place, lint and format hooks wired. Phase 1 (telnet plus ANSI) starts only after explicit approval in chat.
