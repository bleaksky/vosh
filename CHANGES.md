# Changes

All notable changes to Vosh. Newest first.

## v0.3.1 - 2026-05-31

- Connect form now takes a character. The hardcoded `null` in earlier builds meant character-pinned auto-match profiles could never fire by design, so the resolver soft-skipped them every time. The connect row carries an optional "char:" text input that flows into `profileResolveMatch`, and the last character per host is remembered in `localStorage` so reconnects do not require re-typing.
- A profile can claim multiple characters via `characters = ["Erelei", "Lustig"]` in `profile.toml`. Any-of resolver semantics, useful for class-based bundles where one profile covers every warrior the user plays. The legacy single-string `character = "Name"` form still loads through a custom Deserialize impl, and round-trip tests cover legacy-only, new-only, both-present-with-dedup, and the canonical serialize then parse path. Settings · Profiles · auto-match takes a comma-separated list in its "characters" field.
- Every `profile.toml` and `global.toml` save now goes through an atomic write with rotating backups. The existing file is renamed to a timestamped sibling `<file>.bak.<unix-ms>` before the new content lands, the write happens via a `.tmp` rename, and the ten most recent generations are retained. A crashed save, a slow IPC, or any other one-bad-write failure can no longer wipe a populated `custom_themes` or `tracked_affects` list. (A user reported losing both after v0.3.0; the snapshot-protection lives forward of this release.)

## v0.3.0 - 2026-05-31

- Vitals row is a template you author. `%hp / %mana / %sp` style tokens, `%{Char.Worth.gold}` long form, any GMCP `Char.Vitals` or `Char.Worth` field passes through. Per-bar `track` style with a CSS-tinted track replaces the density-ramped glyph bar, percent gradient toggle, inline vs stacked layout. The bar clips at its own bounds so a wide stacked row with `bar_width=60` and parallelogram glyphs cannot bleed into the right sidebar under sidebar-fill-height layout.
- Combat chip stacks vertically to the right of mana with target name and condition wrapping.
- MUD time tick. `World.Time` GMCP feed drives a `formatMudTime` helper with time-of-day color tinting, always visible as a centered tick plus MUD time chip in the status bar.
- Kanso Zen theme accent moves from pink to a cool blue that matches the Aabahran site palette. Accent `b0c8d4`, sage `87a987`, gold `c4b28a`, muted red `c4746e`.
- Multi-pattern triggers, Mudlet-style. Each trigger holds many pattern rows with per-row enable toggles. The backend serde accepts the legacy single-pattern shape and promotes it on first save, and `TriggerForm` exposes per-row add, remove, toggle.
- Aliases gain `%N-` (N from 0 to 9) for "word N and the rest of the input, with original whitespace preserved". `%1-` is equivalent to `%0`, and `%N-` with fewer than N words present expands to empty.
- Folder-style grouping for aliases, triggers, and macros. Each item carries an optional `group` tag, and a single Settings checkbox enables or disables the whole group without losing the per-row `enabled` flag. Disabled groups are skipped at the `iter_compiled` layer for triggers, so they cost nothing at match time. Aliases in a disabled group pass through as typed; macros in a disabled group fall through to alias or to the MUD. Groups are independent per type, matching the existing tab-per-type Settings layout.
- Affects bar has up and down reorder arrows on each tracked affect chip. Custom display labels via a `name` plus `label` pair so e.g. Field of Discord can be shown as Shroud. Backend serde accepts both the legacy bare-string list and the new table-with-label shape.
- Settings forms fade a saved indicator after 1.5 seconds on every form. Manual-save forms (triggers, aliases, JSON tabs) show an unsaved dot plus a `beforeunload` guard so closing with dirty edits prompts.
- Per-panel placement controls in Settings · Panels. Zone plus align for each panel, with a live preview at the top of the tab showing chip-style labels in their assigned zones. Map label moves from a dedicated header strip into the existing subhead row, reclaiming a row of vertical space. Moon phase placement setting adds right-edge, before-time, and after-time options.
- `[duplicate]` on a profile row now uses an inline text input. `window.prompt()` is disabled in Tauri webviews (WKWebView on macOS and WebView2 on Windows both return null synchronously with no UI), so the duplicate handler used to bail out silently with no error, no dialog, no new profile. The button matches the rename flow now.
- Dragging the split-scrollback divider no longer leaves the live pane stuck above the tail. xterm preserved the buffer position across the resize, so server output kept arriving below the visible region. The divider drag now re-anchors the live pane to the bottom on release.
- Scrollbars are themed on Windows. WebView2 was rendering the chunky white system default across every scrollable surface. A global `::-webkit-scrollbar` rule plus the modern `scrollbar-width` and `scrollbar-color` shorthand paints a thin 10px scrollbar with a `--c-border-strong` thumb on a transparent track, applying to the xterm viewport, the panels, the settings forms.
- Frameless transparent corners no longer leak white on Windows. WebView2 cannot paint alpha behind a rounded mask, so the border-radius area showed the white window backing. A `[data-platform=windows]` override drops `border-radius` to 0 on Windows; macOS keeps its rounded corners.
- NAWS no longer wraps the initial `who` and motd at 80 columns. The latest reported window size is cached in `AppState` across the no-session window, so a fresh `session_connect` seeds the Negotiator with the real cols and rows instead of the 80x24 default.
- SQLite log store moves to WAL with `synchronous=NORMAL` and `wal_autocheckpoint=100`. Synthetic bench shows 204 us to 11 us per append (18x), live capture against a populated indexed DB shows about 3x. Append-time spikes under bursty load flatten with the autocheckpoint cap.
- SQLite map store gets the same WAL plus NORMAL pragmas. 432 us to 20 us per `Room.Info` push (21x). Regression test added.
- GMCP fanout is per-package. The backend emits `session://gmcp/<package>` with `.` encoded as `-` to satisfy Tauri event-name rules, and an `onGmcpPackage` helper subscribes per package. The 12 frontend listeners that woke on every GMCP packet now wake only the consumers that need it. Twelve handlers per packet drops to one through three.
- Tick-reset `TickPayload` build folds under the existing profile mutex acquisition, eliminating the second `profile.lock().await` per tick-reset firing in both the per-line and per-GMCP paths.
- `useTickState` rewritten as a `useSyncExternalStore`-backed singleton with snapshot caching. Four to five redundant `setInterval`s collapse to one. `TemplateVitalsRow` no longer subscribes to tick state; `%tick` renders as a self-subscribing `TickSecondsToken` leaf so the template tree stops re-rendering four times per second.
- `setUiConfig` consolidates all ten to eleven cross-window emits with a field-level diff against a `lastSentConfig` snapshot. Empty per-keystroke saves now fire zero emits, one-field changes fire one. `SettingsApp` drops four manual emits plus `broadcastTrackedAffects`.
- `vosh_ansi::plain_text` short-circuits with `String::from_utf8_lossy` when the input has no `0x1B` byte, about 7x faster on ANSI-clean lines. `script_state::snapshot_vars` early-returns when `ScriptEngine::has_handlers` is false, eliminating the per-line var-map clone for users without Lua.
- A `PerfCounters` struct now emits a 1 Hz rollup tracing line for socket reads, lines, profile-lock wait, trigger and Lua time, SQLite append, scrollback push, output emits, GMCP packets, tick emits, routed emits. Debug level by default, the instrument the above measurements run against.

## v0.2.11 - 2026-05-30

- Settings · General opens instantly. The system font enumeration (the slow `font-kit` pass that reads every installed font file from disk) now runs lazily — only after you focus the font filter or hover the list — and caches its result in a process-lifetime `OnceLock`, so every later Settings open in the same session is free.

## v0.2.10 - 2026-05-30

- Server output no longer reads as a typewriter. Each TCP read's worth of MUD lines now lands in a single `session://output` event, so a 50-line response paints in one xterm.write instead of one-per-line. Notably more obvious on Windows where Tauri's WebView2 IPC adds higher per-event overhead than macOS WebKit.
- Map glyph mode rewritten as an HTML monospace overlay. Each cell now tiles at real terminal-cell pitch (1ch × 1em) using the app font, matching tintin's character-grid map instead of the loose square-pixel cells the canvas version produced. Same sector glyphs, same colors, same dim ladder, same `@` player marker.

## v0.2.9 - 2026-05-30

- Pasting multi-line text into the input now sends each line as its own command instead of collapsing the whole paste onto one line. Single-line pastes still insert at the cursor as before. Password prompts are exempt.
- Paste pacing avoids MUD flood kicks. Settings · General · paste pacing sets the delay between lines (default 500 ms, range 0–10000). A `[paste 7/50  esc cancels]` chip shows progress next to the prompt; Esc aborts the queue and leaves any unsent lines unsent.
- Vitals panel is configurable. Settings · Panels · vitals adds independent toggles for the bar, percent, numeric, and per-tick delta columns; pick custom filled/empty glyphs from a row of quick-pick chips (parallelogram, block, heavy/light, circle, square, vertical bar) or type your own Unicode characters; tune the bar width (4–60 cells). A live preview shows the result. Preset chips snap the whole layout to `bars`, `compact`, `numeric`, or `percent` in one click.

## v0.2.7 - 2026-05-30

- Settings · Panels lets you reorder panels inside a zone. Up/down arrows on each chip in the live preview shuffle the stack and persist through `dock_layout`.
- Tick countdown can now embed at the right edge of the vitals bar, room strip, affects bar, or the statusbar instead of taking its own panel. New `in:vitals` / `in:roomstrip` / `in:affects` / `in:statusbar` zones in the tick's zone dropdown. The preview shows the embedded chip as `+ tick` on the host.
- Unset quick keys silently fall through to aliases or pass to the MUD instead of erroring with `no verb is set`.

## v0.2.6 - 2026-05-30

- Closing the main window now closes the Settings popup with it instead of leaving it orphaned.
- Middle-click (scroll-wheel click) on the terminal removes the scrollback break and snaps back to the live tail.

## v0.2.5 - 2026-05-30

- Split-scrollback history pane is now resizable. Drag the divider down to grow the history view, drag up to shrink it. Size persists across sessions.
- Chat panel shorter (160 px default) so it does not eat the window when pinned to bottom.
- Chat scrolls to the most recent message on open instead of showing the oldest.
- Wheel gestures over chat stay inside the chat panel and no longer trigger the terminal scroll.

## v0.2.4 - 2026-05-29

- Tab completes the current word. First Tab fills the most recent match from typed history; subsequent Tabs cycle, Shift+Tab cycles back. Room characters (combat targets) are a secondary source so `ere` completes to `Erelei`.
- Client-side word wrap so long lines (tells, comm channels) stop splitting words mid-character. NAWS still handles most lines server-side; this catches the rest.
- Status bar no longer truncates the target name or quick keys. Wraps to a second row when there are too many to fit.
- Chat panel now has a bounded default height of 240 px (50vh max), so it stops eating the window and the body scrolls properly. Auto-scroll-to-bottom on new messages works again.
- Terminal refits when the panel layout changes, so toggling chat hidden no longer leaves a stripe of padding at the bottom.

## v0.2.3 - 2026-05-29

- Mouse wheel and trackpad scroll now open the split-scrollback view and page the history pane, same as PageUp / Fn+Up. The live pane stays anchored at the bottom regardless of where the cursor hovers.
- Esc snaps the live pane back to the bottom in addition to closing the split.
- Scroll-back divider made thicker and more visible.
- Tick countdown is its own movable panel so hiding vitals no longer hides the tick.
- Settings General gains a [check now] button for updates with inline status (checking, up to date, install + restart, or the error).

## v0.2.2 - 2026-05-29

- Word wrap now happens server-side. The client advertises the live terminal cols and rows to the MUD via telnet NAWS, and the MUD wraps its own output at word boundaries before sending. No mid-word breaks, no client preprocessing, no latency.
- Window resize broadcasts the new size to the MUD immediately so output rewraps live as you drag.

## v0.2.1 - 2026-05-29

- Ctrl+C / Ctrl+X copy the xterm selection on Windows and Linux. Cmd+C / Cmd+X on macOS.
- Shift+Home and Shift+End extend the selection to the start or end of the input line.
- Home and End on long inputs scroll the caret into view.
- Input bar stays editable when disconnected so you can compose commands ahead of a reconnect.

## v0.2.0 - 2026-05-28

- New "panels" tab in Settings with a visual layout map. Move each panel to any side (top, bottom, left, right, hidden) and align top or bottom within left/right zones.
- Six movable panels: map, group, vitals, roomstrip, chat, affects.
- Optional setting to make left and right zones span the full window height with input + status bar under the terminal only.
- Resize handles render on the side facing the terminal.
- Map fills its column; other panels stack above or below at natural size and never overlap.
- Chat panel decoupled from group. Chat header gained a close button.
- Settings tab switch clears stuck error banners.

## v0.1.0 - 2026-05-28

- Split-scrollback view on PageUp. Read older output above while combat keeps streaming below. PageDown or Esc closes it.
- Scroll-depth indicator on the history pane.
- Customizable split-scrollback divider color.
- CMUD / zMUD XML importer added with wildcard translation.
- 256-color and truecolor support advertised via MTTS. MUDs that gated full color now serve it.
- Bare printable keys like `\` can be bound as macros.

## v0.0.9 - 2026-05-28

- App version shows as a tooltip on the brand pill and in the Settings footer.
- Cmd+C copies the xterm selection even when the input box has focus.
- Macro changes from Settings reach the main window without a relaunch.
- Shift-only macro combos (Shift+F1 etc) fire from the input.
- Updater bundles and `latest.json` get signed and uploaded on every release.

## v0.0.8 - 2026-05-28

- Custom theme delete works. Creating a theme from "active" picks up the just-clicked one.
- Custom themes sync across Settings and main windows.
- Theme changes auto-save so the live preview becomes the saved state.
- Brand pill in the topbar shrunk to fit the chrome.

## v0.0.7 - 2026-05-28

- Custom theme editor in Settings. Fork any theme, edit every color slot.
- Fn+Up/Down scrolls the xterm scrollback by a page.
- Fn+Left/Right (or Home/End) move the input cursor to start/end of line.
- Cmd+C copies the xterm selection.

## v0.0.6 - 2026-05-28

- Server output uses the canonical xterm-256 palette by default. Theme tint is opt-in.

## v0.0.5 - 2026-05-28

- Maximize button enters native full-screen on macOS.
- macOS builds are signed and notarized.

## v0.0.4 - 2026-05-28

- Named profile catalog. Each character or MUD gets its own aliases, triggers, macros, and variables.
- Connecting auto-switches to the matching profile.
- Per-category scope toggles, so settings can be global or per-profile.
- Keep last command setting. Press Enter to repeat your last submitted line.

## v0.0.3 - 2026-05-27

- Built-in themes catalog and runtime theme switching.
- Map zoom controls.
- Opt-in auto-updater.

## v0.0.2 - 2026-05-06

Initial tagged release. Covers the full Phase 1 through Phase 12 build:

- Telnet and ANSI parsers with fixture tests.
- TCP/TLS transport, xterm terminal, command input with history.
- Alias engine, variable store (profile + session scopes), regex trigger engine, macro bindings.
- Embedded Lua scripting and plugin manager.
- Auto-mapping rendered to a canvas with camera follow.
- SQLite logging and regex search over scrollback.
- Importers for MUSHclient, Mudlet, GMUD, and TinTin++.
- TinTin-style status bar with hp/sp/mv vitals, tick countdown, and tracked affects.
- Chat pane fed by Comm.Channel GMCP.
- Settings drawer for triggers, aliases, profiles, plugins, themes, fonts.
- Frameless tmux-style chrome with Kanso Zen default theme.
- Cross-platform CI for macOS, Linux, and Windows.
