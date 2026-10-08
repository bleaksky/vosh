# Vosh

A desktop MUD client for macOS, Windows, and Linux. Built for power users who want clean split panes, a map pane, a tick timer, and the full alias and trigger toolkit they expect from TinTin++.

The client targets [Aabahran](https://theforsakenlands.com), a ROM 2.4 MUD, and works with any server that speaks the same protocols.

## Protocols

Vosh speaks telnet and negotiates each option with the RFC 1143 state machine. It answers these options.

- TTYPE, with the MTTS flags on the last answer.
- NAWS, which sends your window size and sends it again when the window changes.
- NEW-ENVIRON.
- CHARSET, which agrees on UTF-8.
- EOR, so the server marks the end of each prompt.
- ECHO, which the server turns on while you type a password, so Vosh masks that line.
- SGA, so neither side sends the telnet go ahead.
- GMCP.

Vosh turns down every other option, MSDP, MCCP, and MXP among them.

## Status

Vosh 0.9.0 is out, and the refactor before 1.0 is under way. `docs/requirements.md` says what 1.0 must do, and `docs/refactor-plan.md` holds the milestone plan with the status of each phase. `CHANGES.md` lists what each release changed.

## Goals

- Equal TinTin++ for power users.
- Add a map pane driven by GMCP.
- Add clean split panes per session, with a chat capture pane and a status pane.
- Add a configurable tick timer with reset on detected events.
- Ship native binaries on macOS, Windows, and Linux.
- No required cloud accounts. No required login. No telemetry. No bundled trackers.

## Stack

Tauri 2 shell. Rust backend with Tokio for async. TypeScript and React frontend. xterm.js for terminal rendering, with a native surface on macOS. Lua via mlua for scripting. SQLite for logs. TOML for human edited profile config.

## Run It

You need rustup, Node 20 or newer, and the Tauri 2 system prerequisites for your platform. See the Tauri 2 prerequisites page. `rust-toolchain.toml` pins the Rust version, and rustup installs it on your first build.

On Linux you also need the WebKitGTK and related dev packages. On Debian and Ubuntu these are `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `libssl-dev`, and `patchelf`.

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

This builds the release binary and the installers for your platform. Add `-- --no-bundle` to build the binary alone.

## Tests

The app crate embeds the built page from `dist`, so build the page before you run the Rust tests.

```
npm install
npm run build
npm test
npm run typecheck
cargo test --workspace
```

## License

GPL v3. See `LICENSE`.

The built in themes come from many authors. `public/theme-credits.txt` names the source, the author and the license of each one and keeps in full every notice their licenses ask for, MIT and Apache alike. Vite copies it into each build, so the notices ship with every copy of Vosh.

## Contributing

See `CONTRIBUTING.md` for development setup, the checks CI runs, commit message format, and where tests and fixtures go.
