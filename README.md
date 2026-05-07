# mudclient

A desktop MUD client for macOS, Windows, and Linux. Built for power users who want a connected map window, clean split panes, a tick timer, and the full alias and trigger toolkit they expect from TinTin++.

The client targets [Aabahran](https://theforsakenlands.com), a ROM 2.4 MUD, but speaks standard telnet, GMCP, MSDP, MCCP, MXP, and the rest of the modern MUD protocol stack. It works with any compliant server.

## Status

Phase 6 complete. The status pane now shows a tick timer at the top with the remaining seconds before the next MUD tick. Configure it with `#tick interval <secs>`, set a reset pattern with `#tick on {regex}`, set an auto-fire command with `#tick fire <command>`, and toggle the beep with `#tick sound on|off`. The countdown flashes red below 5 seconds and plays a short beep when the timer fires. Type `#tick` to see the current state. Phase 7 lands the map window.

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
