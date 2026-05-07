# mudclient

A desktop MUD client for macOS, Windows, and Linux. Built for power users who want a connected map window, clean split panes, a tick timer, and the full alias and trigger toolkit they expect from TinTin++.

The client targets [Aabahran](https://theforsakenlands.com), a ROM 2.4 MUD, but speaks standard telnet, GMCP, MSDP, MCCP, MXP, and the rest of the modern MUD protocol stack. It works with any compliant server.

## Status

Phase 9 complete. The active profile (aliases, profile-scoped variables, triggers, tick config) saves to `<app_data_dir>/profile.toml` and auto-loads on startup. Use `#profile save`, `#profile load`, `#profile reset` to manage it from the input box. The new **profile** button in the header opens a settings drawer that edits the TOML directly with apply/copy/paste/download/upload. `#import-tintin <path>` reads a TinTin++ `.tin` file, imports its `#alias` and `#variable` lines, and reports any directives it skipped. Phase 10 lands logging, scrollback, and search.

See `prompt.md` for the full phase plan and `CLAUDE.md` for stack and workflow rules.

## Goals

- Equal TinTin++ for power users.
- Add a connected map window driven by GMCP Room.Info.
- Add clean split panes per session, with a chat capture pane and a status pane.
- Add a configurable tick timer with reset on detected events.
- Ship signed native binaries on macOS, Windows, and Linux.
- No required cloud accounts. No required login. No telemetry. No bundled trackers.

## Stack

Tauri 2 shell. Rust backend with Tokio for async. TypeScript and React frontend. xterm.js for terminal rendering. Lua via mlua for scripting. SQLite for logs and map storage. TOML for human edited profile config.

## Run It

You need Rust (stable), Node 20 or newer, and the Tauri 2 system prerequisites for your platform. See the Tauri 2 prerequisites page.

```
npm install
npm run tauri dev
```

Frontend only iteration without the Rust backend.

```
npm run dev
```

## Build It

```
npm run tauri build
```

Bundling stays disabled until icons land in Phase 11. The Rust binary still builds.

## Tests

```
cargo test --workspace
npm run typecheck
```

## License

GPL v3. See `LICENSE`.

## Contributing

See `CONTRIBUTING.md` for development setup, lint and format expectations, commit message format, and the phased delivery rules.
