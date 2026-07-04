# Changes

All notable changes to Vosh. Newest first.

## v0.6.2 - 2026-07-04

- Typing a color in Settings no longer blanks the window mid-keystroke. The theme editor, split divider, and sent command color fields used to apply every character as you typed, so a half-finished hex like #010e0 made surfaces turn transparent. They now apply only complete colors and snap back to the last good value if you leave a typo behind.
- Macro and trigger groups stay collapsed the way you left them, across tab switches and restarts.
- The new-macro row now sits at the top of the macros panel, so a long list never makes you scroll to add a binding.
- Preset triggers collect in their own collapsible section, and you can now file a preset into one of your own groups by typing in its group field. That group finally sticks across restarts too. Setting it used to be silently undone at the next launch.
- The tick warning color accepts hex like #ff8800, short hex like #f80, and 256-palette numbers like 196, alongside the existing names.

## v0.6.1 - 2026-07-02

- You can now try the native GPU renderer on Windows and Linux. Type `#nativesurface on` in the command line and restart Vosh to switch the terminal to the native surface, `#nativesurface off` to force the previous renderer anywhere, and `#nativesurface default` to go back to the platform default. The default is unchanged, native on macOS and the previous renderer elsewhere, because the Windows and Linux surfaces have not had a hardware shakedown yet. If you turn it on and something looks wrong, `#nativesurface off` and a restart puts everything back, and a report of what you saw helps a lot.
- `#help` and the in-app help list the new command.

## v0.6.0 - 2026-07-02

- The terminal now draws on a native GPU surface on macOS. Text renders through the system rasterizer at the exact same size and weight as before, but scrolling and heavy combat output are far smoother because server output never crosses into the web layer to reach your screen. This is on by default. If anything looks wrong, set vosh.nativesurface to 0 in DevTools localStorage and reload to fall back to the previous renderer.
- Everything you rely on carries over to the new surface. Scrollback with the split view and its draggable divider, drag to select with copy on release and Cmd+C, find with regex and match highlighting, theme colors and the tint toggle, your font at its exact size, the echo of sent commands, and your saved scrollback from last session are all there.
- Links light up. Hover a URL in the terminal and it turns blue and underlines. Cmd+click opens it in your browser.
- The custom prompt grows a full formatting grammar. Color anything with named colors, 256-palette indexes, RGB triples, or hex codes, and color by a stat percent so your hp number shifts green to yellow to red on its own. Backgrounds take the same color forms. Bold, italic, underline, inverse, and strikethrough styles work inline, and time, date, and percent tokens round it out. The Settings hint lists the syntax.
- A new terminal setting draws bright colored text with the heavier bold font. Most MUDs mark bright text as bold, so turn this on if you like that weight. Off keeps bright text at the normal weight.
- Menus and dialogs that open over the terminal now show correctly instead of clipping behind it, and the content underneath no longer shifts while they are open.
- Selecting text in the live half of an open scrollback split grabs the right characters now.
- Groundwork for the native renderer on Windows and Linux is in place. Both compile and wait on hardware testing, so those platforms keep the previous renderer for now.

## v0.5.3 - 2026-06-25

- The split-scrollback history pane shows your scrollback reliably again, the instant you scroll. A rendering race could leave the top pane blank when the split opened, and the earlier attempt to fix it added a visible pause before the content appeared. The pane now reveals and repaints the moment its scrollback loads, so it is never blank and never lags, with GPU rendering on by default.

## v0.5.2 - 2026-06-23

- The split-scrollback history pane shows content on the very first scroll again. After v0.5.0, opening the split with your first wheel or PageUp gesture could leave the top pane blank until you scrolled a second time, because the pane was positioned before it had finished sizing to its height. The history pane now resizes continuously while the split is open, so its first view always lands on real content.

## v0.5.1 - 2026-06-23

- The split-scrollback history pane no longer comes up blank. With GPU rendering on by default in v0.5.0, the top pane could fail to paint the scrollback it receives all at once the moment the split opens, so it showed empty even though the content was there and the scroll-depth counter kept climbing. The history pane now draws with the standard renderer and paints every time, while the live pane keeps GPU rendering.

## v0.5.0 - 2026-06-23

- The command line is now multi-line. Press Shift+Enter to start another line and the prompt numbers each line down the left edge. When you press Enter, Vosh sends every line as its own command, so you can stack a whole sequence and fire it at once. A single line behaves exactly as before.
- Pasting several lines no longer clears what you already typed. The paste still sends each line, and whatever sat in the command line, selected or not, stays put.
- The MUD's parting text when you quit now lands in the terminal, the scrollback, and the session log. The last lines a server prints as it closes the connection used to vanish before Vosh captured them. Now they read like every other line and survive into your next launch.
- GPU rendering is on by default. The terminal draws through WebGL for smoother scrolling and burst output, and falls back to software rendering on its own if the graphics context is ever lost, so the pane never goes blank. Turn it off under Settings and reload if you prefer the old renderer.
- The Settings window is reorganized. Tabs sit under clear section headers, and the vitals readout and the tick timer each get their own top-level tab. The vitals tab carries a live preview you can drag to scrub the fill and watch your glyph, color, layout, and percent choices update. Many labels and hints across Settings are shorter and plainer.
- The window stays smooth during heavy output. The terminal no longer measures layout on every animation frame, which used to stack a reflow onto each combat round and steal frames from the renderer.
- The find toolbar match counter holds steady while output streams. The position no longer jumps around as new lines arrive in the middle of a search.
- Faster under load. Server output reaches the screen in a more compact form, log writes batch into one transaction per network read, and the trigger engine computes each line's capture groups once instead of several times.
- New profiles start on the Kanso Zen theme.

## v0.4.3 - 2026-06-07

- Split-scrollback divider now drags as a true CMUD-style curtain. As you drag the line, the top pane grows or shrinks over the live pane in real time and content stays anchored — the new rows the top pane exposes match what was at the top of the live pane, because both panes share the same scrollback. The earlier per-snap jitter and post-release refresh are gone, the wrapper resize and the xterm fit and the scroll-anchor restore all commit in one synchronous paint per snap, and closing the split (auto-close on scroll-bottom, ESC, middle-click) no longer triggers any visible re-fit.

## v0.4.2 - 2026-06-07

- Split-scrollback divider now drags smoothly and feels like CMUD's split. The live pane sits behind as an always-present background and the history pane is laid on top with its height controlled by the divider. Dragging slides the divider over the live tail in real time, both panes stay anchored, and on release nothing reflows. The divider snaps to terminal row heights so it always lands on a row boundary and never clips a half-line of content at the bottom of the history pane. The previous build's row-by-row jitter and post-release resize jump are gone.

## v0.4.1 - 2026-06-06

- Five new built-in themes. Dracula at Night carries the iconic Dracula ANSI palette (purple #bd93f9, green #50fa7b, pink #ff79c6) on a deeper chrome for late-session reading. Monokai is the classic Sublime palette with the signature magenta accent. One Dark is the Atom editor cool slate with a blue accent and soft pastel semantics. One Half Dark is the brighter foreground variant of One Dark with cooler surfaces. Tango Dark is the GNOME Terminal classic with saturated CGA-style primaries on a warm dark background. Pick any from Settings · General · Theme.
- New `${target}` session variable that resolves to whoever the `tar` command set most recently. Use it in alias templates (`kill ${target}`), trigger Send templates (`bash ${target}`), or Lua bodies via `mud.var("target")`. Clears on disconnect or `tarclear`. Does not affect trigger patterns or highlight matching, which compile once.
- New `#group <name> on|off` slash command flips a group's enabled state across triggers, aliases, and macros in one shot. Useful for turning a highlight or gag group on around a single fight and back off afterward. `#group <name>` with no arg shows the current state per store. `#groups` lists every group across the profile. The Lua counterpart is `mud.set_group_enabled(name, enabled)`.
- Vitals percent chip in the template layout sits on the surrounding text's baseline. Before this, the bare `inline-block` chip adopted the row's normal line-height computed against its smaller font and the chip box floated above the line, with the surrounding `750(`, `)h`, `(19s)`, `3PM` characters looking taller and looser than they did in the inline plain layout. A flex baseline host span around each `%pct_*` token render gives the chip the same baseline behavior the inline layout already had.
- Terminal cursor pads to the bottom of the viewport after every initial-mount path, not only when the scrollback restore comes back empty. Any saved scrollback that was shorter than the viewport used to leave the cursor mid-screen and the next chunk from the MUD piled into the middle, surfacing the big gap on connect. The pad now runs after the post-restore writes flush, so a short or empty scrollback both anchor cleanly.
- Typed commands mirror into the split-scrollback history pane so scrolling up shows your own commands alongside server output. No backend round-trip and no added latency. Persisted scrollback restored on next launch is still server-only.

## v0.4.0 - 2026-06-05

- Lua scripting for aliases (mode toggle template / lua) and trigger actions (Script effect), with a CodeMirror 6 editor in the Settings drawers. The backend evaluates the body in a sandboxed engine with capture groups bound to a `captures` table.
- Multi-command sends now split on `;` and `\n` into separate wire lines, with `\\`, `\;`, and `\\\n` escapes preserved. Fixes triggers like a disarm chain that used to send `get 1.;wield 1.` as one mangled line.
- Hidden doors render with pink dashed lines on the map, closed doors with amber, closed and locked with solid red. Door state comes from GMCP `Map.Tiles` payloads. A hidden door without a neighbor draws a half-pitch stub.
- Player input lands in session log files with a `>` prefix, so `#log` search returns both directions of the transcript.
- macOS spell-check works in the input prompt. It is enabled at the NSResponder level via `toggleContinuousSpellChecking`, the same path the right-click menu uses. The frontend gates the `spellcheck` attribute to chat-prefixed lines (`say`, `tell`, `gossip`, `emote`, and so on) so MUD verbs do not light up red on every line, and a Settings toggle disables it entirely.
- Trackpad wheel sensitivity in the terminal pane drops to a calmer level and switches to a pixel accumulator so per-event jitter no longer triggers a runaway scroll.
- The vitals bar gains two new inline layout styles. `drain` renders each vital as a detailed chip with the caption and percent in the top corners, a numeric value below, an inset drain background that fills proportional to the value, and a glowing leading edge in the vital color. `badge` renders a compact chip with the value and a small percent pill hanging off the upper-right corner. Pick from the new `inline style` dropdown in Settings · Panels · panes · vitals. Plain (the historical `cur(pct%)L` look) stays the default.
- New `% mode` dropdown controls how the percent value gets colored in the non-plain inline styles. `drain to red as low` (the default) ramps from the stat color through warn gold and orange to danger red as the value drops. `match stat color` keeps it tied to the vital. `accent` paints it in the brand pink.
- New tintin-style `#prompt {regex}` slash command turns any server prompt into a vitals source. Every named capture group like `(?<hp>\d+)` gets bound to a prompt var of the same name that the vitals bar reads with priority over GMCP, so the same chip styling you pick in Settings drives the values from your parsed prompt text. The matching line gets gagged. Run `#unprompt` to remove it. Works without GMCP, so MUDs that do not push `Char.Vitals` can still drive a bar from prompt text alone.
- Triggers gain a `target` dropdown in the editor (line or prompt) so any trigger can be tagged to fire only on the partial-prompt buffer the server flushes at GA / EOR instead of completed lines. The line option (the historical behavior) is the default.
- A trigger that gags the matched line AND echoes via Lua now renders the echo in the gagged-line row instead of below it with extra spacing. Before this, a gag plus an echo read as a blank row followed by the echo on a new line.
- Percent values in the inline and template vitals layouts can now wrap in a styled chip. Pick `pill`, `soft tint`, `glow ring`, or `drain fill` from the new `% chip` dropdown in Settings · Panels · panes · vitals. The chip applies to the plain inline layout and to the `%pct_hp`, `%pct_mn`, `%pct_mv` template tokens alike. Default stays plain so existing layouts render unchanged.
- Fresh sessions with no scrollback now anchor MUD output at the bottom of the terminal pane instead of the top. The cursor pads to the last row on mount and re-pads on resize, so subsequent writes scroll content up from a full buffer and the giant gap between the latest prompt and the room strip and vitals chip is gone.
- `#profile load` and `#profile save` now route to the active profile under `profiles/<active>.toml` instead of the legacy single-file path they were stuck on after the multi-profile migration. The legacy path is still consulted as a fallback for pre-migration layouts.
- Per-profile triggers, aliases, and macros overlay on top of the shared catalog (Path B mode) instead of getting silently replaced by it. A trigger you author in your per-profile file or via `#prompt` survives the next launch. Same-name entries from your per-profile file win, new per-profile names are added.
- Typed commands now mirror to the split-scrollback history pane so scrolling up shows your own commands alongside server output. Persisted scrollback restored on next launch is still server-only.

## v0.3.15 - 2026-06-04

- New vitals layout option stacks a braille trend grid underneath your bar. Each cell carries two samples across four dot rows of vertical fill, drawn in the vital color over a dim base. Recent hp / mn / mv drops read as a falling shape next to the current bar fill. Pick it from the new `layout` dropdown in Settings · Panels · panes · vitals. The bar style (solid / track / ramped) and the layout (plain / with history) are now independent dropdowns so you can pair any bar with the trend grid. The live preview in Settings reflects both selections.
- The combat target chip can now live in its own panel. Settings · Panels · combat lets you place it in the top, bottom, left, or right zone, with `hidden` (the default) keeping the legacy "inline next to vitals" behavior. When combat AND vitals both land in the bottom zone the combat chip renders flush against the vitals bar with no border between them, so the two read as one block. In any other zone combat gets the full pane treatment and vertically centers in its own surface so a single-line chip never sits glued to the top edge.
- The combat target's hp bar is redesigned. It now uses whichever bar style you picked for vitals (solid glyphs, track, or ramped) with a single-hue drain-red palette that runs bright red at full hp through to near-black as the opponent dies, and adopts your bar font so combat and vitals render consistently. The chip itself collapses to a single line of swords plus target name plus bar plus percent. The condition word is dropped.
- The "one with erelei" toggle is renamed "low hp vignette" so the name actually describes what it does. Existing profiles preserve your choice across the rename.
- Trackpad scrolling on macOS is much calmer. The terminal pane no longer runs away when you swipe. Both the live terminal and the split-scrollback history pane now scale scroll speed by accumulated gesture distance instead of per-event count, so a touchpad swipe scrolls proportional to how far you actually moved rather than how many tiny events the OS fired.
- Selection ghosts in the terminal pane are fixed. Selecting text in the live pane then watching MUD output scroll your selection off the top of the viewport used to leave a permanent gray highlight ghost in the spot where the selection used to live. The selection now auto-clears the moment it leaves the viewport.
- Selections across the split-scrollback are exclusive. Selecting in the history pane clears any selection in the live pane (and vice versa), so the copy shortcut always grabs the most recent selection rather than competing across two panes.
- The chat panel is resizable from whichever edge faces the main content. Drag the divider on top when chat sits at the bottom, or on the bottom when chat sits at the top. The new height persists across launches per zone.
- Switching between chat channels (all / chat / tell / etc.) now scrolls the body to the latest message instead of preserving the previous channel's scroll position.

## v0.3.14 - 2026-06-03

- The vitals settings panel is restructured as a two-column console. A dense control rack on the left (mode · show · appearance · layout · colors · template) and a live monitor on the right showing your real bar preview at full width. The monitor footer surfaces the current layout, style, width, and font so you can confirm state at a glance without scrolling. Collapses to a single column under 820px so a half-width Settings window still works.
- New "one with erelei" mode in the vitals console. Toggles a soft red peripheral vignette that pulses on the window edges when hp drops below 30 percent. Additive — your bar, template, or inline layout continues rendering normally and the vignette sits on top of the chrome as a fixed-position overlay.
- The bar font picker grows MonoLisa, Menlo, Consolas, and Courier New presets. MonoLisa ships under several family names depending on the build (regular, Variable, Trial); the picker probes for each and surfaces a small status line under the dropdown noting which variant your system actually carries, or a warning if none was found.

## v0.3.12 - 2026-06-03

- The vitals settings top is redesigned. The crowded row of `style | filled | empty | width` is replaced by a single `style` dropdown that carries both the bar style and the glyph pair as one choice (e.g. `solid · block · █ ░`, `ramped · braille full · ⣿ ⣀`, `track · smooth CSS bar, no glyphs`), and a small `width` field next to it. Hand-edited custom glyphs surface a `custom` entry in the dropdown and reveal the filled / empty text inputs on demand.
- New `bar font` picker that applies only to the bar glyphs, leaving labels, percent, numeric, delta, and panels chrome in your app font. Pick `Berkeley Mono (bundled)` or `JetBrains Mono (bundled)` for clean partial-block and braille rendering. Pick any locally-installed monospace by name and Vosh registers a runtime `@font-face` for it so the font actually resolves on macOS WKWebView (where bare CSS family names are refused by anti-fingerprinting). An always-visible `CSS font-family` input below the dropdown accepts any custom stack.

## v0.3.11 - 2026-06-03

- Five new vitals bar glyph presets in the btop family. dark / light shade (▓ ░), block / medium shade (█ ▒), braille full (⣿ ⣀), braille mid (⠿ ⠤), and braille thin (⠶ ␣).
- The ramped bar style is back. Pick `ramped` in the style dropdown next to solid and track. Ramped uses 1/8-step partial-block characters (▏▎▍▌▋▊▉) on the boundary cell so the bar moves at sub-character resolution instead of snapping to whole-cell increments. At bar width 20 a solid bar moves in 5% jumps; the ramped variant moves in 0.625% jumps. Looks best in the bundled Berkeley Mono or JetBrains Mono.

## v0.3.10 - 2026-06-02

- The vitals preview in Settings is now interactive. Click and drag any of the hp / mn / mv bars to scrub through the 0 to 100 percent range and watch the colors transition through the ramp live. The header above the preview reads the current values so you can see exactly which percent each bar is at. Useful for verifying a custom color renders the way you want at every fill level before you commit to it.

## v0.3.7 - 2026-06-02

- The vitals bar no longer hides your current/max or your delta column when the panel is narrow. The bar now gives up width first so the percent, the numeric, and the delta stay visible at the panel edge. The bar_width setting in Settings becomes a maximum width rather than a hard pin. For the smooth track style the fill stays proportionally correct at the shrunk width. For the solid glyph style the filled and empty character counts are recomputed from the rendered width so a 50 percent bar always shows as half full no matter how narrow the panel is.
- Custom vitals templates that contain bar tokens now wrap cleanly at newlines you typed instead of mid-line wherever the browser felt like breaking. Each line of your template becomes its own row, and inside the row the bar token absorbs the leftover width while the text and other tokens hold their natural width. Templates without any bar tokens keep the original inline rendering so users who depend on exact whitespace are not disturbed.
- New per-vital color overrides. Settings · Panels · panes · vitals now has a colors section with a swatch per vital and a "drain through red as the bar empties" toggle. Leave a color blank to keep the built-in green / blue / orange ramps. Pick a color and keep the toggle on to make that color the high end of a drain-to-red ramp; the drain compresses into the bottom half of the bar so a 75 percent bar stays your picked color flat and dark picks like `#102000` render as green, not muddy olive. Pick a color and turn the toggle off to make the bar that color flat at every fill percentage. The preview in Settings reflects per-vital overrides too, so what you see while configuring is what you get on the live bar.

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
