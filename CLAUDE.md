# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Read This First

`docs/requirements.md` says what Vosh is and what 1.0 must do. `docs/refactor-plan.md` is the milestone plan. It holds every plan item, the status of each, and the decisions taken. Read both before doing anything. This file holds the stack, the rules that govern how you work, and where things live. `docs/architecture.md` maps the code as it stands.

## Stack

- App shell. Tauri 2.
- Backend. Rust with Tokio for async. rustls for TLS.
- Frontend. TypeScript and React, bundled by Vite.
- Renderer. A native Metal surface under the web view on macOS, described in `docs/renderer.md`. xterm.js on Windows and Linux.
- Map. HTML canvas with a layer abstraction. WebGL only if rooms-per-second profiling demands it.
- Scripting. Lua via mlua. QuickJS as a possible secondary later.
- IPC. Tauri commands and events between Rust and the web view, with a typed event bus on top.
- Storage. SQLite via rusqlite for logs. TOML for human edited profile config. `docs/data-files.md` lists every file Vosh writes.
- License. GPL v3.
- CI. GitHub Actions matrix across macOS (arm64 and x86_64), Windows, and Linux.

## Repo Layout

```
mudclient/
  CLAUDE.md            Stack and workflow rules. This file.
  README.md            What Vosh is, the protocols it speaks, how to build it.
  CONTRIBUTING.md      How to develop, lint, test, commit.
  CHANGES.md           Release notes.
  HELP.md              The one copy of the help text. The Help window reads it.
  LICENSE              GPL v3.
  Cargo.toml           Rust workspace root, with Cargo.lock.
  rust-toolchain.toml  The pinned Rust version.
  rustfmt.toml         Rust format settings.
  package.json         Frontend dependencies and scripts, with package-lock.json.
  vite.config.ts       Frontend bundler config.
  tsconfig.json        TypeScript config, with tsconfig.node.json.
  eslint.config.js     Lint config. Prettier reads .prettierrc.json.
  knip.ts              Knip config for the unused code check.
  index.html           Vite entry point.
  docs/                requirements.md, architecture.md, renderer.md,
                       data-files.md, refactor-plan.md, and history/ for
                       the kickoff prompt and the renderer milestones.
  src/                 React frontend, one folder per job.
    shell/             The main window, its title band, status line and overlays.
    settings/          The Settings window, one folder per page.
    help/              The Help window and the HELP.md reader.
    ipc/               Every call into Rust and every event, one file per topic.
    stores/            Live data in memory, in gmcp/, config/ and session/.
    terminal/          The game output, with xterm/ and native/.
    input/             The command line and its hooks.
    panel/             The right panel and each pane.
    prompt/            The pinned prompt dock and the prompt card.
    writing/           The writing card.
    theme/             Colors, built in themes and the theme runtime.
    automation/        Trigger, alias, macro, timer, preset and loadout logic.
    ui/                The kit every window shares.
    lib/               Small helpers used in several places.
    styles/            The sheets, loaded through styles/index.css.
    test/              Shared test fixtures and the Tauri mock.
  src-tauri/           Rust app crate (vosh-app), Tauri config, capabilities.
  crates/              Rust library crates.
    automation/        Aliases, variables and triggers.
    log/               logs.sqlite.
    prompt/            The prompt engine and the fake MUD test kit.
    protocol/          Telnet, ANSI and GMCP on the wire.
    script/            The sandboxed Lua engine.
  fixtures/            Byte streams, game lines and shared test cases.
  examples/lua/        A sample script.
  plugins/             A sample plugin.
  public/              Bundled fonts and the theme credits.
  scripts/             css-usage.mjs, the unused class check.
  .github/workflows/   CI for the three platforms and the release build.
  .githooks/           Project pre-commit hook.
```

## Common Commands

These work once `npm install` and `cargo fetch` have run at least once.

- `npm run dev` runs Vite alone for frontend iteration.
- `npm run tauri dev` runs the full app (Rust backend plus web view).
- `npm run tauri build` builds release binaries for the host platform.
- `npm run lint`, `npm run format:check`, `npm run typecheck` cover frontend checks. `npm test` runs the frontend tests.
- `npm run knip` and `npm run css:usage` find unused code, dependencies and CSS classes.
- `cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features -- -D warnings` cover Rust checks.
- `npm run build` builds the page, which the app crate embeds. Run it before the Rust tests.
- `cargo test --workspace` runs Rust tests. Single test by name: `cargo test --workspace <test_name>`.

## Workflow Rules (Non-Negotiable)

This is an **approval gated project**, built in milestones and plan items. `docs/refactor-plan.md` lists the items. Read, propose, wait for approval, then implement. Never skip an item.

1. Stop at every item close. Each item ends with a working build, a short demo, and an approval checkpoint. Do not start the next item without explicit approval in the chat.
2. Surface architecture decisions. When two valid implementation paths exist and the choice affects architecture, stop and ask.
3. Stop conditions. Pause for human review when a file would be permanently deleted, a new external service or API needs integration, an error cannot be resolved in two attempts, the task requires changes outside the stated scope, or an item closes.
4. Commits are small and single concern. Conventional Commits format.

## Forbidden Actions

- Modifying files outside the working directory.
- Pushing to a remote without explicit approval.
- Deleting files without showing a diff first.
- Adding dependencies that pull in trackers, telemetry, or required cloud services.
- Shipping features that require a server we do not control.
- Adding a feature outside the approved plan items for the current session.

## Quality Bars

- Protocol parsers (telnet, ANSI, GMCP) get unit tests against byte stream fixtures in `fixtures/`. The fixtures are synthetic, built from the lines the Aabahran server sends, and each says so. Real captures land only with your approval.
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

## Current State

Vosh 0.9.0 is out. The refactor before 1.0 follows `docs/refactor-plan.md`, which holds the status of every item and track. R23, docs, help and guards, is built except item 5, which marks the refactor done once the last item lands. R24, release groundwork, is built except the draft release build in CI, which waits for you to push. R25, accessibility and a high contrast theme, comes next. The last commit of each item updates this section.
