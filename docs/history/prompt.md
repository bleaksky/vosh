# Master Prompt. Custom MUD Client Project Kickoff

## Context (carry forward)

You are starting a new desktop MUD client project from zero. The author runs and develops Aabahran, a ROM 2.4 MUD at theforsakenlands.com, and currently uses TinTin++. The new client must equal TinTin++ for power users and add a connected map window, clean split panes, and a tick timer. Targets are macOS, Windows, and Linux with signed native binaries.

This is a phased, approval gated project. Read first, propose, wait for approval, then implement. Do not skip phases. Do not write code before stack approval.

## Objective

Phase 0 deliverable for this session. Read the working directory, propose a stack with reasoning and tradeoffs, scaffold the repo only after explicit approval, then implement Phase 1 (telnet plus ANSI display) and stop.

## Starting State

The working directory may be empty or may contain a CLAUDE.md, README, or partial scaffolding. You do not know which. Find out before doing anything.

## Target State (Phase 0 plus Phase 1)

After this session, the repo should have these things and nothing more.

1. A clean project skeleton for the agreed stack with CI for all three platforms.
2. A LICENSE file (ask which license), a README describing scope, and a CONTRIBUTING note.
3. A CLAUDE.md playbook in the repo root capturing stack decisions, phase plan, and style rules from this prompt.
4. A working Phase 1 build that opens a TCP or TLS connection, negotiates core telnet options, parses ANSI, displays output in a single pane, and sends typed input to the server.
5. A test target with at minimum unit tests for the telnet and ANSI parsers fed by captured byte fixtures.

Stop after Phase 1 demos cleanly against Aabahran. Do not start Phase 2.

## Hard Requirements (full project, version 1.0)

These are the capabilities the v1.0 client must support across all phases. Build the architecture so each one slots in cleanly.

### Connection and protocol

- TCP and TLS connections, with optional StartTLS.
- Telnet option negotiation including IAC, SB, SE, GA, EOR, DO, DONT, WILL, WONT.
- TTYPE (24), NAWS (31), CHARSET (42), MCCP2 (86), MCCP3 (87), MSSP (70), MSDP (69), GMCP (201), ATCP (200), MXP (91) optional.
- ANSI color, 256 color, and 24 bit truecolor.
- Configurable charset per profile, default UTF 8 with latin 1 fallback.
- Auto reconnect with backoff and a manual override.

### Input and output

- Command line input with multiline edit, history, and prefix search.
- Aliases with parameter substitution and recursion guards.
- Triggers using PCRE compatible regex with capture groups, ordered priority, and per pattern enable flags.
- Trigger actions of highlight, gag, replace, send, run script, and route to pane.
- Variables and lists with profile and session scope.
- Speedwalk parsing and execution.
- Macros bound to keys and key chords, with optional modal sets.
- Logging to file with rotation and per session toggles.
- Scrollback buffer sized in lines or memory, with regex search and timestamping.
- Command queue with throttle to respect server rate limits.

### Display and layout

- Tabbed sessions, each session bound to a profile and a server.
- Split pane output within a session, with at minimum a main pane, a chat capture pane, and a status pane.
- Resizable panes that persist per profile.
- Highlighting actions that change foreground, background, bold, underline, and inverse.
- Themeable via CSS variables or a small token system. Ship a dark theme and a high contrast theme.

### Text rendering

- ANSI 8 and 16 color, xterm 256 color palette, and 24 bit RGB truecolor. All three render correctly when servers mix them in one stream.
- Configurable color blend mode for trigger highlights so highlights compose cleanly with existing ANSI color rather than stomping it.
- Full UTF 8 input and output. Render the Basic Multilingual Plane and Supplementary Multilingual Plane, including emoji, CJK, Cyrillic, Greek, and combining characters.
- Correct grid handling for CJK double width characters, combining marks, and zero width joiner sequences. No column drift after wide glyphs.
- Font smoothing with grayscale and subpixel options. Per platform default that respects the OS setting, with a per profile override.
- High DPI and Retina rendering with crisp glyphs at any zoom level.
- Configurable monospace font with a fallback chain. Render a visible replacement glyph for missing characters, never silent drops.
- Per profile font size, line height, and letter spacing.
- Bold, italic, underline, strikethrough, and inverse rendering. Prefer real bold and italic font variants over synthetic styling.
- Ligatures off by default to keep grid alignment. Optional toggle for users who want them.
- Cursor style options of block, underline, and bar, with optional blink.
- Box drawing and block element characters render flush in the grid.
- Optional inline image rendering for MUDs that send them. Kitty graphics protocol and iTerm2 inline images first, sixel as a stretch.

### Out of band data

- GMCP message routing into a typed event bus that triggers, scripts, and the UI can subscribe to.
- Built in handlers for common GMCP packages including Char.Vitals, Room.Info, Comm.Channel.Text, Char.Items.List, Char.Skills.List.
- Variables auto populated from GMCP data, exposed to triggers and scripts under a stable namespace.
- MSDP fallback for servers that do not speak GMCP.

### Map

- Detachable window or floating pane.
- Driven primarily by GMCP Room.Info, with a manual fallback for MUDs that lack room data.
- Renders rooms and exits with configurable glyphs, colors, and labels. Supports user supplied glyph sets and bitmap tilesets.
- Click a room to issue a path command. Right click a room for notes, color, and tags.
- Stores maps per area in a portable on disk format (SQLite plus exportable JSON).
- Pathfinding across known rooms with avoidance flags (no trap rooms, no PK areas, etc.).

### Tick timer

- Configurable interval and offset.
- Reset on detected events using regex on input or output, or on a GMCP signal.
- Visible countdown in the status bar with optional sound and color flash near zero.
- Optional auto fire of an alias or script on tick.

### Scripting

- Embedded scripting language exposed to triggers, aliases, key bindings, and the GMCP event bus.
- Capability to load scripts from files in the profile directory.
- Sandboxed by default with explicit permissions for filesystem and network.
- A small standard library covering string and regex helpers, JSON, time, and a logger.

### Profiles and settings

- Per profile config in a human readable format (TOML preferred), versioned with a schema.
- Import path for TinTin++ aliases and triggers where possible, with a clear unsupported list.
- Settings UI plus a text editor fallback for power users.
- Backup, restore, and export of profiles.

### Distribution

- macOS universal binary (arm64 plus x86_64), signed and notarized.
- Windows installer (MSI or NSIS), signed.
- Linux AppImage and a deb plus rpm pair.
- Opt in auto update.

## Soft Preferences

- Native feel on each platform without sacrificing layout consistency.
- Cold start under 1 second on Apple Silicon.
- Memory footprint under 200 MB idle for one session.
- Persist scrollback and command history to recover sessions after a crash.
- Optional accessibility mode with screen reader hints and large font defaults.

## Anti Goals

- No required cloud account, no required login, no telemetry.
- No bundled adware, analytics, or tracking.
- No proprietary script format. Plain text, diffable, version controllable.
- Do not ship a feature that depends on a third party server we do not control.

## Recommended Stack (propose this, justify it, accept alternatives)

Open Phase 0 by proposing this stack. Lay out the reasoning and the tradeoffs. Wait for approval before scaffolding.

- App shell. Tauri 2 for the cross platform application shell. Native window management, small binary, real Rust backend, web frontend that renders MUD output well.
- Backend language. Rust. Solid async networking with Tokio, strong type system for protocol parsing, mature telnet and TLS crates.
- Frontend. TypeScript with React or Solid. Mature component model, good terminal and editor libraries.
- Terminal renderer. xterm.js with a custom addon for trigger and highlight pipelines. If xterm.js becomes a bottleneck, fall back to a custom canvas renderer.
- Map renderer. HTML canvas with a layer abstraction. WebGL only if performance demands it.
- Scripting. Lua via mlua. MUD convention, small footprint, easy to sandbox. Offer JavaScript via QuickJS as a secondary option.
- IPC. Tauri commands and events between Rust and the web view.
- Storage. SQLite via rusqlite for logs, scrollback indexes, and map data. TOML for human edited config.
- Build and CI. GitHub Actions matrix for macOS, Windows, Linux. Release artifacts uploaded on tagged commits.

Alternatives to consider and rule on.

- Electron with Node and TypeScript. Faster prototype, larger binary, weaker performance.
- Qt with C++ and Lua. Strong native feel, slower iteration, smaller contributor pool.
- Go with Wails or Fyne. Lean binary, smaller ecosystem for terminal rendering.

## Phase Plan

Each phase ends with a working build, a short demo, and an approval checkpoint. Do not start phase N+1 without explicit approval in the chat.

### Phase 0. Read, propose, scaffold

- Run a directory listing and read any CLAUDE.md, README, or notes.
- Propose stack with tradeoffs.
- After approval, create the repo skeleton, CI for all three platforms, license, README, CONTRIBUTING, and CLAUDE.md.
- Set up code style, linting, and pre commit checks.
- Commit and stop.

### Phase 1. Telnet and ANSI

- Implement raw TCP and TLS connections.
- Implement telnet option negotiation for the core options listed above.
- Stream incoming bytes through an ANSI parser into the renderer.
- Render 256 color and 24 bit truecolor correctly in the same stream.
- Render UTF 8 cleanly with default font smoothing and proper handling of CJK double width characters.
- Provide a minimal command input that sends to the server.
- Demo. Connect to Aabahran, log in, see colored output (including any 256 color or truecolor sequences the server sends), type CJK or emoji into the input field and see it round trip, send commands.

### Phase 2. Aliases, variables, history

- Add an alias engine with parameter substitution and recursion guards.
- Add a variable store with profile and session scope.
- Add command history with prefix search.
- Demo. Define an alias, use it across a reconnect.

### Phase 3. Triggers and highlights

- Add a regex trigger engine with capture groups and priority.
- Add highlight, gag, replace, send, and route actions.
- Add a trigger editor UI with import and export.
- Demo. Highlight tells in cyan, gag spam, replace text on match.

### Phase 4. GMCP and event bus

- Parse GMCP messages into typed events.
- Expose an event bus to triggers and the soon to come script engine.
- Auto bind common packages to variables.
- Demo. HP and MP populate from Char.Vitals, room name updates from Room.Info.

### Phase 5. Split panes and chat capture

- Add multi pane output within a session.
- Add a chat capture pane fed by triggers or GMCP.
- Add a status pane bound to GMCP variables.
- Persist pane layout per profile.
- Demo. Tells route to chat pane, status pane shows vitals.

### Phase 6. Tick timer

- Add a configurable tick timer with reset events.
- Display countdown in the status bar.
- Optional sound and color flash near zero.
- Demo. Tick fires every 30 seconds, resets on a chosen output line.

### Phase 7. Map window

- Add a detachable map window.
- Render rooms from GMCP Room.Info, with manual room creation as fallback.
- Add click to walk and right click for notes.
- Persist maps per area in SQLite.
- Add pathfinding with avoidance flags.
- Demo. Walk through a small area and see the map populate. Click a known room to walk there.

### Phase 8. Scripting engine

- Embed Lua via mlua.
- Expose triggers, aliases, variables, GMCP events, send, log, and timer functions to scripts.
- Add a sandbox with explicit permissions.
- Demo. A Lua script that builds a small combat assistant.

### Phase 9. Profiles and settings

- Add per profile TOML config, import and export.
- Add a settings UI with a text editor fallback.
- Add a TinTin++ script importer with a clear unsupported list.
- Demo. Import an existing TinTin++ alias and trigger file from Aabahran usage.

### Phase 10. Logging, scrollback, search

- Add per session log files with rotation.
- Add a search panel with regex.
- Add scrollback persistence between runs.
- Demo. Search a week of logs in under a second.

### Phase 11. Polish and packaging

- Add accessibility hints and a high contrast theme.
- Sign and notarize macOS builds. Sign Windows builds. Build AppImage and deb plus rpm.
- Add an opt in auto updater.
- Demo. Install from each platform's native package.

### Phase 12. Stretch

- Plugin system for community packages.
- Speech triggers via OS native TTS.
- Optional local LLM integration for chat classification and summary, off by default, runs through Apple Silicon native inference where available.
- Macro recorder.

## Allowed Actions

- Read, create, and edit files inside the working directory.
- Run package manager installs for the agreed stack only after approval.
- Create commits in small, single concern chunks.
- Run tests and the dev build locally to verify each phase.
- Add Rust crates and npm packages already common in the chosen stack.

## Forbidden Actions

- Do not write code before stack approval.
- Do not modify files outside the working directory.
- Do not push to a remote without explicit approval.
- Do not delete files without showing a diff first.
- Do not make architecture decisions silently. Surface them and wait for approval.
- Do not add dependencies that pull in trackers, telemetry, or required cloud services.
- Do not ship features that require a server we do not control.
- Do not add a feature that was not in the agreed phase scope for this session.

## Stop Conditions

Pause and ask for human review when any of these happen.

- A file would be permanently deleted.
- A new external service or API needs to be integrated.
- Two valid implementation paths exist and the choice affects architecture.
- An error cannot be resolved in two attempts.
- The task requires changes outside the stated scope.
- A phase boundary is reached.

## Checkpoints

After each major step, output a one line status of what was completed in plain text. At the end of each phase, output a full summary of every file changed plus a short demo script the human can run to verify.

## Quality Bars

- Tests for protocol parsers (telnet, ANSI, GMCP) must run against captured byte stream fixtures. Add a fixtures directory and seed it with samples from Aabahran.
- Integration tests use a fake MUD server fixture.
- Run cargo clippy, cargo fmt, eslint, and prettier on every commit.
- Crash reports are local only and opt in.
- Every commit message follows Conventional Commits.

## Writing Style for Any User Visible Text

Follow these rules for README, settings labels, error messages, and any prose the user will read.

- Active voice.
- Address the user as "you".
- Direct and concise.
- No dashes of any kind, no semicolons, no colons in body prose, no asterisks, no emojis.
- Vary sentence length for rhythm.
- Concrete and specific, not abstract.
- Definitive statements over conditionals.
- No filler phrases. No "it's important to note", "let's explore", "streamline", or similar.

## First Action

Run a directory listing. Read any CLAUDE.md, README, or notes you find. Summarize what exists. Then propose the stack with reasoning and tradeoffs. Wait for approval before any code is written.
