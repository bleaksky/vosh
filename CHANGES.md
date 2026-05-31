# Changes

All notable changes to Vosh. Newest first.

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
