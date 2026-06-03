# Changes

All notable changes to Vosh. Newest first.

## v0.3.9 - 2026-06-02

- The "drain through red as the bar empties" toggle no longer washes out dark per-vital colors. Before, the bar lerped from your picked color toward red across the whole 0 to 100 percent range, so even a 75 percent full bar mixed in 25 percent red. A dark green like `#102000` rendered as muddy olive instead of green. The drain now compresses into the bottom half of the bar. Above 50 percent the bar is your picked color flat. Below 50 percent it transitions toward red so the warning cue still fires when the bar empties.

## v0.3.8 - 2026-06-02

- The vitals preview in Settings now reflects the per-vital color overrides you pick. Previously the bars in the preview pane always used the theme accent (pink) and never changed when you set a custom hp / mn / mv color, so you had to save and look at the live bar to see the result. Both the preview and the live bar now run through the same color resolver, including the drain-through-red ramp toggle.

## v0.3.7 - 2026-06-02

- The vitals bar no longer hides your current/max or your delta column when the panel is narrow. The bar now gives up width first so the percent, the numeric, and the delta stay visible at the panel edge. The bar_width setting in Settings becomes a maximum width rather than a hard pin. For the smooth track style the fill stays proportionally correct at the shrunk width. For the solid glyph style the filled and empty character counts are recomputed from the rendered width so a 50 percent bar always shows as half full no matter how narrow the panel is.
- Custom vitals templates that contain bar tokens now wrap cleanly at newlines you typed instead of mid-line wherever the browser felt like breaking. Each line of your template becomes its own row, and inside the row the bar token absorbs the leftover width while the text and other tokens hold their natural width. The classic three-line template `HP %bar_hp %pct_hp %dhp` / `Mana %bar_mn %pct_mn %dmn` / `Move %bar_mv %pct_mv %dmv` lays out correctly even at panel widths the old renderer would have wrapped through the middle of. Templates without any bar tokens keep the original inline rendering so users who depend on exact whitespace are not disturbed.
- New per-vital color overrides. Settings · Panels · panes · vitals now has a colors section with a swatch per vital and a "drain through red as the bar empties" toggle. Leave a color blank to keep the built-in green / blue / orange ramps. Pick a color and keep the toggle on to make that color the high end of a drain-to-red ramp. Pick a color and turn the toggle off to make the bar that color flat at every fill percentage.

## v0.3.6 - 2026-06-02

- Per-profile settings now actually save when you are using the global catalog (Path B) mode. Tracked affects, the "tint server output with theme palette" toggle, panel layout when its scope is profile, the vitals shape, custom themes, paste pacing, moon-glyph position, chip style, and the enabled-presets list all stayed in memory and silently dropped on quit. Switching profiles also lost them. Each loadout now writes its own per-profile file, and switching loadouts re-applies the shared catalog on top so your aliases and triggers stay in sync.
- New scrollback search. Press Cmd+F on macOS or Ctrl+F on Windows and Linux to open a search bar at the top of the terminal. Type a query and press Enter to find matches across the whole live session, not just what you can see right now. A match count shows `3 / 12` next to the input so you can see how many hits exist. Shift+Enter or the up arrow walks backward, the case / word / regex toggles work as labeled, and Esc closes the bar. When a match lives in scrollback above the live tail, the split-scrollback view opens automatically and highlights the match in the history pane while live keeps streaming below.
- Quit is no longer ugly. Some MUDs (Forsaken Lands among them) slam the connection closed when you type `quit`, which used to surface as `[read failed: Connection reset by peer (os error 54)]` and could swallow the goodbye line the server tried to send. Vosh now drains any pending bytes from the kernel after a read error so the goodbye text has a chance to land, and the disconnect message now reads `[connection reset by server]` or `[server closed connection]` instead of the raw os-error noise.
- 256-color detection should now stick on Forsaken Lands and other servers that gate color on the first TTYPE response. The client now identifies as `VOSH-xterm-256color X.Y.Z` in slot 1 so substring scans for `xterm` or `256` match. MTTS-aware servers still get the full three-slot cycle with the capability bitmask in slot 2. NEW-ENVIRON is also now answered with `TERM=xterm-256color` and `COLORTERM=truecolor` for servers that check environment variables instead of (or in addition to) TTYPE.
- Settings · panels · chips drops the moons-position dropdown. The "left of tick / time chip" and "right of tick / time chip" options anchored the moons block to a chip that no longer lives in the status bar (the tick + MUD time chip moved to the input row's top border in v0.3.5). Moons always render on the right side of the status bar now. Existing profiles with the old setting are coerced to the right-edge position automatically.

## v0.3.5 - 2026-06-02

- New in-client help. Click `[help]` in the top bar to open a full help window. Read about every feature in plain language, with a search bar that filters topics live and highlights matches as you type. Press Esc or click the backdrop to close. The same content is mirrored as `HELP.md` at the repo root for offline reading.
- The tick countdown and MUD time clock are now one chip riding on the top border of the input row. Pick its appearance (value only, caption plus value, or icon plus value) in Settings · Panels · chips. The chips sub-view also brings tick interval, auto-fire command, regex reset, sound, and warning settings into the same place, so you no longer have to drop to the command line to dial in the tick.
- Settings · Panels gains a sub-toggle for layout, panes, and chips. Layout still shows zone placement. Panes is where you configure individual panel content like vitals shape and tracked affects. Chips is where the tick, MUD time, and moons chips live. The three sub-views read as distinct chapters instead of one long scrolling tab.
- Click anywhere in the main window that is not a button or an active text selection and the command input refocuses. A quick way back to typing after reading is just to click anywhere in the terminal area. The same focus pull fires when you bring Vosh back to the front from another app.
- The chat pane sticks to the bottom while you are reading the latest messages and pauses autoscroll when you scroll up to read back. Scroll back within twenty-four pixels of the bottom and autoscroll resumes. Opening the chat pane jumps to the latest message instead of the oldest.
- The room strip wraps onto multiple lines when you place it in a left or right side panel, and the panel becomes resizable. A wide strip in a side zone now reads top to bottom instead of being clipped.
- Deleting a profile, alias, or trigger uses an inline two-step confirm (`[delete]` then `[confirm delete]`) instead of the browser confirm dialog that the Tauri webview silently rejected. The destructive action lives next to its target row.
- The top bar drops the `chat` toggle button. The chat pane now opens from Settings · Panels by setting its zone to top, bottom, left, or right, matching how every other panel is shown or hidden.
- The group pane drops the unpin button. The pane is always pinned when its zone is not hidden, so its visibility is controlled the same way every other panel is.

## v0.3.1 - 2026-05-31

- Profile auto-match now reads your character name from the MUD itself. As soon as you log in, the MUD sends Vosh your character name over GMCP, and Vosh swaps to the matching profile silently. No more typing your character name into the Connect form, no more wrong profile loaded because you forgot.
- A profile can now claim more than one character. List them in Settings · Profiles · auto-match as a comma-separated string ("Erelei, Lustig, Carmen") and the profile autoloads for any of those characters. Useful when you want one profile to cover every warrior you play, or every alt on a single shared host.
- Your settings are now protected against bad writes. Every save of `profile.toml` and `global.toml` is atomic, and the previous version is kept as a timestamped backup. Up to ten backup generations are retained, so a crashed save or a stale-state overwrite can no longer wipe your custom themes or tracked affects. (One user reported losing both after installing v0.3.0; this protection lives forward of this release.)

## v0.3.0 - 2026-05-31

- The vitals row is a template you write. Use `%hp`, `%mana`, `%sp` style tokens for the common stats, or pull any GMCP `Char.Vitals` or `Char.Worth` field with the long form `%{Char.Worth.gold}`. A new per-bar "track" style replaces the old density-ramped glyph bars, optional percent gradient, inline or stacked layout. The bar can no longer bleed into the right sidebar at wide widths.
- The combat target chip stacks vertically to the right of mana, showing target name and condition with wrap.
- MUD time tick. The MUD clock drives an always-visible centered tick countdown and a MUD time chip in the status bar, color-tinted by time of day.
- Kanso Zen theme accent shifts from pink to a cool blue that matches the Aabahran site palette.
- Each trigger can now hold multiple pattern rows, in the style of Mudlet. Each row has its own enable toggle, so you can switch mob names or events on and off without editing a long pipe-delimited regex. Old single-pattern triggers keep loading.
- Aliases gain `%N-` (for N from 0 through 9): "word N and the rest of the input, with original whitespace preserved." `%1-` is equivalent to `%0`; `%N-` with fewer than N words present expands to empty.
- Aliases, triggers, and macros can be grouped into folders. Each item carries an optional group tag, and a single Settings checkbox turns the whole group on or off without losing the individual enable flags. Aliases in a disabled group pass through as typed, triggers in a disabled group do not fire, macros in a disabled group fall through to alias or to the MUD.
- Tracked affects gain up and down reorder arrows on every chip. You can also set a custom display label per affect so "Field of Discord" can be shown as "Shroud".
- Every Settings form fades a saved indicator after 1.5 seconds when changes land. Manual-save forms (triggers, aliases, JSON tabs) show an unsaved dot, and closing the window with dirty edits prompts you first.
- Settings · Panels lets you place each panel by zone and alignment, with a live preview at the top of the tab. The map label moves into the existing subhead row, freeing a row of vertical space. Moon phase placement gains right-edge, before-time, and after-time options.
- The Duplicate button on a profile row works again. It previously relied on a browser prompt dialog that the Tauri webview silently rejects, so clicking did nothing. It now opens an inline text input like the Rename flow.
- Dragging the split-scrollback divider no longer leaves the live pane stuck above the tail. The drag now re-anchors the live pane to the bottom on release.
- Scrollbars on Windows match the rest of the app. The chunky white system default is replaced by a thin, themed scrollbar on every scrollable surface (terminal viewport, panels, settings forms).
- Window corners on Windows no longer leak white at the rounded edges. Windows users get flat rectangle corners; macOS keeps its rounded ones.
- The initial `who` and motd no longer wrap at 80 columns. The client now caches your real window size between connects so the MUD knows it from the first byte.
- Performance pass. Log writes are about 18x faster in benchmarks and around 3x faster against a populated indexed database. Map updates are about 21x faster. GMCP packets are routed per package, so background panels stop redrawing on every event. The tick countdown stops re-rendering the vitals template four times per second. Settings auto-save fires zero events for empty changes and one event per real field change instead of ten at a time. Output lines that contain no ANSI escape codes take a fast path that is about 7x faster than the parsed path.

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
