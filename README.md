# mudclient

A desktop MUD client for macOS, Windows, and Linux. Built for power users who want a connected map window, clean split panes, a tick timer, and the full alias and trigger toolkit they expect from TinTin++.

The client targets [Aabahran](https://theforsakenlands.com), a ROM 2.4 MUD, but speaks standard telnet, GMCP, MSDP, MCCP, MXP, and the rest of the modern MUD protocol stack. It works with any compliant server.

## Status

Phase 1 complete. The app connects to a MUD over TCP or TLS, negotiates the core telnet options, refuses what it does not yet understand, and renders the full ANSI repertoire through xterm.js. Input round trips as UTF-8 with command history. Phase 2 lands aliases, variables, and prefix history search.

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
