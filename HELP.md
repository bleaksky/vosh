# Vosh help

This file is the same content the in-client Help view shows. The Help view (top bar `[help]` button) is the primary place to read it. This file mirrors the same topics for offline reading and copy-out.

The source of truth for both is `src/lib/helpContent.ts`.

---

## Start here

### 1.1 What Vosh is

Vosh is a desktop MUD client. It connects you to a text-based multiplayer world over the internet, shows that world's output in a terminal pane, and lets you type commands back. It runs on macOS, Windows, and Linux. There is no required account, no cloud sync, no telemetry.

Vosh ships with a connected map, configurable panels for vitals, channel chat, and party roster, a tick timer, an alias and trigger system, and per-profile settings so different characters can carry their own commands. The whole thing is built for power users who want TinTin++-grade tooling inside a modern desktop window.

### 1.2 What a MUD is

A MUD is a text-based multiplayer game world. People log in, walk between rooms, talk on channels, fight monsters, and build characters together. There are no graphics. Everything happens through text the server sends you, and commands you type back. The acronym stands for Multi-User Dungeon, though most worlds today have moved well beyond dungeons.

A MUD client is the program you use to talk to a MUD server. The server runs the world. The client renders the text and forwards your keystrokes. Without a client you would be staring at a raw telnet stream. With Vosh, the same stream becomes a colored terminal, a live map, and a dashboard of bars and pills that tell you what is happening at a glance.

### 1.3 Your first connection

The connect bar sits across the top of the window. It has three fields. Host is the address Vosh dials. Port is the TCP port the MUD listens on. TLS is a checkbox that asks Vosh to wrap the connection in encryption.

Type the address and port, leave TLS off unless the MUD specifically supports it, and click `[connect]`. The terminal pane echoes a status line as the connection opens. Once you see your login prompt, type your character name and password the same way you would type any other command. Vosh switches the input to password mode the moment the MUD asks for one, so the characters you type stay off screen.

The default values point at Aabahran on `play.theforsakenlands.com` port `1848`. Replace them with whatever world you are connecting to.

### 1.4 Reading the screen at a glance

Vosh has six places where information shows up.

- The terminal pane in the middle is the live MUD output.
- The room strip above it summarizes the room you are standing in.
- The vitals row near the bottom shows your hp, mana, and movement.
- The status bar across the very bottom shows your current target, your quick-keys, the moon phases, and your computer's clock.
- The map pane on the right (by default) shows the rooms around you.
- The command line at the bottom is where you type.

Each of these can be hidden, moved to another edge, or restyled. The Settings window has a `panels` tab that controls placement. The map panel can also be toggled from the top bar.

---

## The command line

### 2.1 Typing and sending commands

Type at the prompt, then press Enter. Vosh sends your line to the MUD as one command. The same line shows up at the bottom of the terminal pane so you can see what you sent.

Pressing Enter on an empty line sends a blank line, which most MUDs use to advance prompts or step through paginated output.

You can chain several commands by separating them with semicolons. `look;score;who` sends three commands in order.

Press Shift+Enter to add a second line instead of sending. The prompt grows and numbers each line down the left edge, and when you press Enter, Vosh sends every line as its own command. A single line behaves exactly as before.

Anything starting with `#` is a slash command. Slash commands talk to Vosh itself, not the MUD. They are how you create aliases and triggers, configure the tick timer, and save your profile. Type `#help` to see the full list.

### 2.2 Command history

Vosh remembers every command you send in the current session. Press Up to walk backward through that history. Press Down to walk forward.

If you start typing before pressing Up, Vosh treats what you typed as a prefix and only cycles through history entries that start with the same letters. So typing `cast` then pressing Up walks back through every spell you have cast, skipping unrelated commands.

Editing the line cancels the prefix filter so the next Up press starts a fresh search from whatever you typed last. The `keep last command` toggle in the General tab tells Vosh to leave your sent line in the input box, selected, so a second Enter resends it.

### 2.3 Tab completion

Press Tab to complete the word you are typing. Vosh fills in the first match from three sources, in order.

- Words from your typed-command history, most recent first.
- Names of characters currently in your room (from the Room.Chars GMCP push).
- Capitalized names Vosh has seen in MUD output over the last thirty minutes (who-lists, channel speakers, considers).

Press Tab again to cycle to the next match. Press Shift+Tab to walk the cycle backward. Typing anything else resets the cycle so the next Tab starts a fresh search.

### 2.4 Pasting many lines

Paste a block of text into the command line. Vosh splits it on newlines and sends each non-empty line as its own command. While the burst is in flight, a small `paste N/M esc cancels` indicator sits next to the prompt. Press Esc to drop any unsent lines.

A multi-line paste never clears what you already typed. It sends its lines, and whatever sat in the command line stays put.

The General tab has a paste-pacing field. Set it to the number of milliseconds you want Vosh to wait between lines. Zero means no pacing. A few hundred milliseconds dodges MUD flood filters that drop the connection when too many commands arrive too fast.

### 2.5 Password mode and line editing

When the MUD asks for a password, Vosh switches the input field to password style and replaces the typed characters with dots. Password lines never enter your command history, and the local echo to the terminal is blank.

Inside the input line, Home jumps to the start, End to the end. On macOS, Cmd+Left and Cmd+Right do the same. Hold Shift on any of these to extend a selection from the current caret. Standard text-editing keys (left, right, backspace, delete) work as you would expect.

---

## Reading output

### 3.1 The terminal pane

The terminal pane is where MUD output lives. It renders ANSI color exactly the way a stock xterm would. Long lines wrap at the terminal's column count, which Vosh advertises to the MUD via NAWS (the standard telnet sub-option for window size) on every connect and on every resize. Most modern MUDs honor NAWS and wrap server-side at the advertised width.

The terminal scrolls back automatically. Vosh keeps roughly ten thousand lines in memory plus a persistent copy on disk so the previous session's tail shows up when you launch Vosh again. A dim `[scrollback restored]` line marks where the old session ends and a new one begins.

### 3.2 Selecting and copying text

Click and drag in the terminal pane to select text. Release to lift the selection.

Press Ctrl+C (Cmd+C on macOS) to copy the selected text. The shortcut still works while focus is on the command input, which is the common case. You select with the mouse, then hit the shortcut without clicking back into the terminal. Ctrl+X also copies. Nothing is cut because the terminal is read-only.

Clicking anywhere in the window that is not a button or an active selection re-focuses the command input. So a quick way back to typing after reading is to click anywhere in the terminal area.

### 3.3 Split scrollback

Scroll up with the mouse wheel anywhere in the terminal area and a second pane opens above the live one. It shows earlier output. The live pane keeps streaming below so combat does not freeze while you read history. The top-right corner of the upper pane shows `↑ N / max`, where N is how many lines above the live tail you are looking, and max is how many lines of scrollback exist.

Drag the divider between the two panes to set the split ratio. Press PageUp and PageDown to scroll a page at a time. Scroll back to the live tail with the wheel, click the middle mouse button anywhere in the terminal, or press Esc, and the split closes.

### 3.4 The status bar and clock

The bar across the very bottom of the window carries small status chips. The left side shows your current target (when one is set) and your quick-key bindings. The right side shows the moon-phase glyphs the MUD pushes via GMCP, followed by your computer's local clock.

The chips are read-only. Quick-keys configure under `#qkey <name> <verb>`. The moon glyphs have tooltips that show each moon's name and phase when you hover.

---

## Profiles

### 4.1 What a profile is

A profile is a saved bundle of everything that is yours, character by character. Aliases, triggers, macros, quick-keys, variables, tick configuration, and (when scoped that way) theme, font, and panel layout. Each profile lives on disk. Switching profiles loads a different bundle into memory.

Out of the box you have one profile called `default`. You can stay on it forever if you only play one character. Most people make a profile per character or per MUD and let auto-match pick the right one when they connect.

### 4.2 Auto-matching a profile

The Profiles tab in Settings has an `[auto-match]` button on each profile. It opens an inline form with three fields.

- Host is the MUD address.
- Port is the MUD port. Leave blank to match any port.
- Characters is a comma-separated list of character names. Leave blank to match any character on the host.

When you click `[connect]`, Vosh resolves a profile by host and (if set) port. If that profile pins one or more characters, Vosh does not switch yet. After login, the MUD pushes Char.Status with your character name, and Vosh swaps to the profile that lists that character.

Click `[save]` to commit the match block. Future connects will pick the right profile automatically.

### 4.3 Saving and switching profiles

Type `#profile save` to write the current profile to disk. Vosh also saves on its own whenever you change configuration through Settings.

Type `#profile load` to discard in-memory state and reload from disk. Type `#profile reset` to wipe the current profile back to defaults.

Switch between profiles by clicking `[switch]` next to a profile name in the Profiles tab. The whole environment swaps at once. Aliases, triggers, macros, and any per-profile chrome (theme, font, dock layout) load fresh from disk.

### 4.4 Profile vs global scope

Each profile carries its own aliases and triggers. Some other settings can be shared across every profile or carried per-profile, your choice. The scope row at the top of the Profiles tab has five toggles. Theme. Font (family and size together). Dock layout. Keep-last-command. Auto-check updates.

Set each to `global` if you want every profile to share the same value, or `profile` if you want each profile to remember its own. Flipping a toggle from global to profile copies the current value into every profile so nothing visibly changes the first time you do it.

---

## Layout and panels

### 5.1 Panels overview

A panel is a piece of UI Vosh can dock at one of four edges of the main window (top, bottom, left, right) or hide entirely. The map, chat, group, vitals, affects, and room strip are all panels.

Each panel can sit in any zone its content can fit. The map only allows left, right, or hidden because a horizontal map at full window width looks like a strip. The other panels accept any edge.

The Settings window has a `panels` tab with three sub-views.

- `layout` shows your placement choices and a visual map of where each panel will land.
- `panes` dials in content for individual panels (vitals shape, affects to track).
- `chips` configures the small read-outs that ride along the chrome (tick, mud time, moons).

### 5.2 Choosing where each panel lives

Open Settings, click the `panels` tab, and stay on the `layout` sub-view. Each panel has a row with a zone dropdown and an alignment dropdown. Pick a zone. For left and right zones, the alignment dropdown chooses whether the panel hugs the top of the column or the bottom. Top and bottom zones ignore alignment.

Use the up and down arrows on each row to reorder panels within their shared zone.

Click `[reset to defaults]` if you want to start over.

### 5.3 Resizing and side-fill

Drag the inside edge of any side panel (the visible dividing line between the panel and the terminal) to resize that zone's width. Each side zone remembers its width on its own.

The Panels tab has a checkbox called `side panels span full height`. When it is off (the default), top and bottom zones stretch all the way across the window and the side panels sit between them, like a plus sign. When it is on, the side panels run the full height of the window, top to bottom, and the input row plus status bar live in a column under the terminal area only. Pick whichever shape fits your screen.

---

## The panels themselves

### 6.1 The map pane

Vosh draws a map of your surroundings from GMCP data the MUD pushes (the Map.Tiles package). The player sits at the center of the canvas. Same-floor rooms render in full color around you. Rooms one or more floors above or below render dimmer in the same positions. Lines connect rooms that share an exit.

The header bar above the canvas has a label, a radius badge, a style toggle, and a zoom control. The style toggle picks between three rendering modes.

- `squares` is the default. Filled sector-colored squares with corridor lines underneath. Vertical exits show as small `▲` and `▼` markers.
- `glyphs` renders each room as a character glyph in the terminal font, TinTin++ style. The player is an `@`.
- `tileset` lets you load a horizontal PNG of thirteen tiles in sector order and use those instead of solid squares.

Zoom with the `+` and `-` buttons. The percentage shows in the middle. The `⤺` button resets to 100%. Hold Ctrl or Cmd and roll the mouse wheel over the map to zoom in or out as you would in a map app.

Hide the map with the `[map]` button in the top bar. Click it again to bring the map back to its last zone.

### 6.2 The chat pane

The chat pane collects channel chatter so you can read it without watching the live scrollback. Public channels, tells, and any trigger line you route with a `route <pane>` action all land here.

The pane is hidden by default. Move it somewhere visible from Settings panels (set the chat panel's zone to top, bottom, left, or right) or pin it under the terminal and use it as a backlog.

The header has a tab strip showing every pane name Vosh has seen. Click a tab to filter to just that source. Click `all` to see everything.

The pane auto-scrolls to the newest message while you are looking at the bottom. If you scroll up to read back, autoscroll pauses. Scroll back within twenty-four pixels of the bottom and autoscroll resumes.

### 6.3 The group pane

The group pane shows your party. While you are grouped, each member appears on a row with their name, class, level, hp percent, mana percent, move percent, and points to next level (TNL). Numbers are colored by how full they are.

While you are solo, the pane swaps to your own Worth read-out. TNL, experience, gold, bank balance, trains, and practices. The data comes from GMCP (Group.Info and Char.Worth), so the pane updates whenever the MUD pushes a new snapshot.

### 6.4 The vitals bar

The vitals bar shows your hp (hit points), mn (mana), and mv (movement). Each row carries a color-coded bar, a percent, a current-over-maximum number, and the change since the last tick.

Open Settings, click `panels`, pick the `panes` sub-view, and expand the `vitals` row to configure the bar.

- The preset chips at the top (`bars`, `compact`, `numeric`, `percent`) flip every column toggle to a sensible combination.
- The toggles below let you turn each column on or off independently.
- The bar glyphs section sets which characters render the filled and empty cells, plus the bar width.
- The layout dropdown switches between stacked rows (one vital per row) and inline (all three on a single row in TinTin++ nprompt style).
- Percent color either matches the bar (per-vital color) or uses a red-to-green gradient against the percent.

If none of that fits, turn on `custom template` and write your own line. The textarea takes tokens like `%hp`, `%pct_mn`, `%bar_mv`, and the help block under it lists every supported token. Tokens can also reach into any field your MUD ships through `Char.Vitals` or `Char.Worth` (for example `%xp`, `%gold`).

### 6.5 Tracked affects

An affect is a temporary status the MUD applies to your character. Sanctuary, haste, poison, bless, anything timed.

The affects bar shows pills for the affects you have asked Vosh to track. Each pill carries the affect name and the remaining duration. The duration color shifts from green (plenty of time) to yellow, orange, and red as it counts down. A permanent affect renders cyan. A tracked affect that is not currently active renders as a dim pill with a `-` instead of a duration, so you can see at a glance which of your buffs are missing.

Pick which affects to track in Settings panels under the `affects` accordion. Add an affect by name (the server-side name from `Char.Affects`). Optionally set a custom label that displays instead of the server name. The tooltip still shows the real name so you cannot lose track of what a chip stands for.

### 6.6 The room strip

The room strip summarizes the room you are standing in on a single horizontal line. It reads `area · room name · #vnum · terrain · [exits] · here <chars> · items <items>` left to right.

The area name is colored by the area, which makes border crossings obvious. The terrain word is colored by its sector type. Exits show as their compass-direction initials (N, E, S, W, U, D) in square brackets. Characters and items group by name so a stack of twenty arrows reads as `(20) arrow` instead of twenty separate entries. Your current target shows up with a `▶` next to its chip in the `here` list.

The strip lives in the top zone by default. When you move it into a side panel, it wraps onto multiple lines and becomes resizable.

---

## Tick and target

### 7.1 The tick timer

A tick is a recurring server event most MUDs use to regenerate hp, mana, and movement. Knowing when the next tick lands is the difference between resting and dying.

Vosh has a tick countdown chip that rides on the top border of the input row. It shows `tick 14s` (or similar) and decrements every second. When it reaches zero, an optional sound plays and an optional auto-fire command sends.

Configure the tick in Settings panels, in the `chips` sub-view.

- The interval defaults to a common tick length. Set it to whatever your MUD uses (Aabahran is thirty seconds).
- The auto-fire field is a command Vosh sends on every tick. Useful for keeping a `consider` or `score` on a clock.
- The reset-on regex resets the countdown whenever a server line matches it. Useful for MUDs that re-anchor the tick to specific events.
- The warning section adds an optional pre-fire alert with custom text and color.

Every one of these is also drivable from the command line. `#tick` shows the current state. `#tick interval 30`, `#tick reset`, `#tick on {pattern}`, `#tick off`, `#tick fire score`, `#tick nofire`, `#tick sound on|off`, `#tick disable`, `#tick enable`, and the `#tick warn` family (`#tick warn at 5`, `#tick warn message your tick is up`, `#tick warn color bright-red`, `#tick warn off`) cover the lot.

### 7.2 Targets and quick-keys

A target in Vosh is whatever character you have most recently picked with the `tar` keyword. The current target shows in the status bar as `tar <name>` and gets a `▶` marker on its chip in the room strip.

Set a target by typing `tar` followed by a number or a partial name. `tar 2` picks the second character in the room. `tar helg` picks the first character whose name contains `helg`. With a substring you type, the target name stays exactly what you typed, so a command like `kill ${target}` sends the short keyword the MUD's own matcher already knows how to resolve. Cycle through everyone in the room with `tarn` (next) and `tarp` (previous). Clear the target with `tarclear`.

A quick-key is a short name (like `gg`) that expands to a verb plus your current target. Four slots come pre-defined. `gg`, `xx`, `zz`, `tt`. All start with empty verbs. Configure a verb with `#qkey gg kick`. Now typing `gg` while you have a target sends `kick <target>`. `#qkey clear gg` removes a binding. `#qkeys` lists every binding.

Quick-keys expand before alias expansion, so a quick-key always wins over an alias with the same name. They will not fire without a target set.

---

## Aliases, triggers, variables

### 8.1 Aliases

An alias is a short name that expands into a longer command (or several commands) when you type it. If you find yourself typing the same phrase many times, an alias replaces it with one word.

The simplest way to make one is from the command line. `#alias greet wave;bow` defines an alias named `greet` that runs `wave` then `bow`. Now typing `greet` sends both commands. `#unalias greet` removes it. `#aliases` lists every alias.

The Settings window has an `aliases` tab with a form editor. Each alias has a name, an expansion, an optional group (folder), and a toggle that controls whether typing the name expands it or leaves it alone.

Variables interpolate inside the expansion before it sends, so `${target}` and `$hp` work just like in any other typed command. The `%1`, `%2`, ... placeholders take positional arguments from the words you typed after the alias name. `%0` is the entire argument string. `%1-` is `argument 1 onward`.

Aliases can call other aliases. Vosh stops at a recursion limit so an alias that calls itself does not loop forever. The error echoes in brackets when it kicks in.

If you prefer raw JSON, the same tab has a mode toggle. Form mode is what most people want.

### 8.2 Triggers

A trigger watches the server's output for a pattern and runs an action on every match. Triggers are how you color a tell red, hide spam, send a follow-up command when you see a keyword, or push a line into the chat pane.

The pattern is a regular expression. Captures (`(\w+)`) become `$1`, `$2`, and so on inside the trigger's action. The whole match is `$0`. Named groups like `(?<who>\w+)` become `${who}`.

A trigger can do any combination of these actions.

- `highlight <color> [bold] [underline] [inverse] [bg:<color>]` colors the matched line.
- `gag` hides the matched line so it never reaches the terminal.
- `replace <template>` rewrites the line. The template can interpolate captures and variables.
- `send <template>` sends a command to the MUD. Same template substitution.
- `route <pane>` sends a copy of the matched line to a named chat pane.

Define a trigger from the command line with `#trigger <name> {pattern} <action>`. The braces around the pattern matter, so spaces inside the pattern do not split into argument boundaries. Backslash-escape a literal `}` inside the pattern as `\}`.

The Settings window has a `triggers` tab with the same form editor as aliases. Triggers belong to groups, have a priority that orders matching (lower runs first), and have an enable switch. Vosh ships a set of preset triggers, and the General tab toggles which presets are on.

### 8.3 Variables

A variable is a named value you can interpolate into any typed command, alias expansion, or trigger template. Vosh has two scopes.

- Profile variables live with the profile and survive across launches.
- Session variables live only as long as the current session and clear on disconnect.

Set one with `#var name value`. Show one with `#var name` (no value). Remove with `#unvar name`. List all with `#vars`.

Interpolate with `$name` or `${name}`. The braces are useful when the name runs into surrounding letters, as in `${name}ish`. A literal `$` escapes as `$$`.

Some variables update on their own. `${target}` always reflects the current target. Triggers and Lua scripts can set variables on a match.

---

## Macros and recording

### 9.1 Keyboard macros

A keyboard macro is a key on your keyboard that sends a command when you press it (while the command input has focus). Function keys, modifier combinations, and the numpad are all available.

Open Settings, click `macros`. Each row has three fields.

- The key field captures a key when you click it and press something. It accepts F-keys, Numpad keys, and modifier chords like `Ctrl+N` or `Shift+Alt+F3`.
- The command field is what gets sent on every press. Chain commands with semicolons.
- The group field is an optional folder so you can bulk-enable or disable related macros.

The bottom row is always empty so you can add a new binding. Fill in a key and a command, then click `[add]`. Delete a row with the red `[delete]` button.

Each named group has a header with a checkbox. Uncheck it to disable every macro in that group at once without losing the bindings. Re-check to bring them back.

### 9.2 Recording a multi-step alias

Recording is a faster way to build a complex alias. Instead of typing it out in the form editor, you tell Vosh to start recording, run through the sequence you want, then tell it to stop. The recorded commands save as a single alias that fires them in order when you type its name.

Type `#record buff` to start recording with the alias name `buff`. Every command you type from then on is captured. Slash commands are skipped, so `#aliases` or any other `#`-prefixed command will not land in the macro. Type `#endrec` when you are done. Vosh saves an alias called `buff` whose expansion is every recorded command joined with semicolons.

Type `#record` on its own to see the status of an in-progress recording. Type `#record cancel` to throw away the recording without saving.

---

## Appearance

### 10.1 Themes

A theme is a named bundle of colors. Surfaces, text, borders, accent, semantic colors (warn, danger, info, success), and the full ANSI 0 to 15 palette the terminal uses.

The Settings window has a `themes` tab. Built-in themes are listed first and are read-only. Click any row to make that theme active. Vosh applies the new colors immediately.

Click `[+ new from active]` under the custom themes section to fork the current theme. The new theme appears in the list with an `[edit]` button. Click `[edit]` to open the color grid. Each slot has a color picker and a hex or rgba text field. Change either and the running window redraws as you type. Click `[done]` when you are happy. Custom themes can be deleted, but only when they are not active.

The General tab has a checkbox called `tint server output with theme palette`. When it is off, server colors render with the canonical xterm palette regardless of theme. When it is on, the theme's palette tints server output too.

### 10.2 Fonts

The General tab in Settings sets the font Vosh uses for the terminal and the chrome. Type the family name directly into the font field. The five chips below the field are quick picks for fonts known to be present. Two ship bundled with Vosh (BerkeleyMono and JetBrainsMono). Three are system fonts always available on macOS.

The system fonts list lower on the page enumerates every monospace font installed on your computer. It loads lazily when you focus the filter input or hover the list. Filter by name. Click a row to pick. Uncheck `monospace only` to widen the list to every font. Proportional fonts will look strange in the terminal.

Set the font size in the `size` row. Nine to thirty-two pixels. The preview box at the bottom of the section shows the current font at the chosen size.

---

## Tools

### 11.1 Logs and search

Every connection Vosh makes gets logged. Each line, with its timestamp and ANSI colors, persists to a local SQLite database. The Logs tab in Settings is the only reader.

The left column lists sessions, newest first, with the host, port, start time, and line count. Click `all sessions` to search across everything. Click a specific session to scope the search to just that one. The small `copy` button next to each row copies the plain-text transcript of that session to your clipboard.

The right column is search. Type a pattern, choose case-sensitive or not, click `[search]`. Hits appear with their timestamp, the session they came from, and the original ANSI-colored text. The default cap is five hundred matches. Turn on `show all` if you want every match.

Search is plain substring (or case-sensitive substring), not regex. For complex queries, copy a session and search in your editor of choice.

### 11.2 Importing from another client

If you are coming from another MUD client, Vosh can ingest your aliases, triggers, macros, and variables. The Import tab supports MUSHclient (`.mcl` / `.xml`), Mudlet (`.xml`), GMUD (`.cfg`), and CMUD / zMUD (`.xml`). Auto-detect picks the format from the file contents.

Click `[pick file]` and choose your export, or paste the contents into the textarea directly. Click `[apply]`. Vosh parses the file, merges every entry into your current profile, and prints a summary. The summary lists how many of each kind landed, plus any rejected (broken), unsupported (no equivalent in Vosh), or unparsed (we did not understand the line) entries. Open each section to read the details.

Existing aliases and triggers with the same name are overwritten by the import. Save a profile first with `#profile save` if you want a backup before you do this.

For TinTin++, use the command line. `#import-tintin <path>` reads a `.tin` file and pulls in `#alias` and `#variable` directives. Other TinTin directives are counted in the summary so you can port them by hand.

### 11.3 Lua scripting

Vosh embeds Lua. Lua scripts can register triggers, react to GMCP, change variables, and send commands. Scripts live in your app data directory under a `scripts/` folder as `.lua` files.

Load a script with `#script load name`. Vosh reads `name.lua` from the scripts directory and runs it. Any triggers the script registers stay registered until the script is reloaded or the profile is reset.

Reload every loaded script with `#script reload`. List loaded scripts and any Lua triggers with `#scripts`.

Run a one-off snippet with `#lua <code>`. The snippet has access to the same Lua bindings the scripts use, so you can test something quickly without writing it to a file.

---

## Updates

### 12.1 Updates

The General tab has an `updates` row. The `auto-check on launch` checkbox controls whether Vosh checks for a new version at startup. When a newer version exists, the row turns into an install button like `[install vX.Y.Z + restart]`. Click it and Vosh downloads the update, applies it, and relaunches.

The `[check now]` button kicks off an out-of-cycle check. The status text to its right shows `checking…`, `up to date`, or an error.

Updates ship as signed releases. Vosh never installs an unsigned binary.

---

## Reference

### 13.1 Every slash command

Type any of these at the prompt. The text in `< >` is a placeholder you replace. `{pattern}` is a regex inside braces.

- `#help` shows the full slash list inline.
- `#alias <name> <expansion>` defines an alias.
- `#unalias <name>` removes an alias.
- `#aliases` lists every alias.
- `#var <name> <value>` sets a variable.
- `#var <name>` shows a variable.
- `#unvar <name>` removes a variable.
- `#vars` lists every variable with its scope.
- `#trigger <name> {pattern} <action>` defines a trigger.
- `#untrigger <name>` removes a trigger.
- `#triggers` lists every trigger by priority.
- `#tick` shows the tick timer's current state.
- `#tick interval <secs>` sets the tick interval.
- `#tick reset` resets the timer to a fresh interval.
- `#tick on {pattern}` adds a regex that resets the tick on every match.
- `#tick off` clears the regex reset pattern.
- `#tick fire <command>` runs a command on every tick fire.
- `#tick nofire` clears the auto-fire command.
- `#tick sound on|off` toggles the tick beep.
- `#tick disable` stops the tick timer.
- `#tick enable` starts the tick timer.
- `#tick warn` shows the warning settings.
- `#tick warn at <secs>` echoes a warning that many seconds before the tick fires.
- `#tick warn message <text>` customizes the warning text.
- `#tick warn color <name>` colors the warning (red, bright-red, yellow, ...).
- `#tick warn off` disables the warning.
- `#script load <name>` loads `name.lua` from the scripts directory.
- `#script reload` re-runs every loaded script.
- `#scripts` lists loaded scripts and Lua-registered triggers.
- `#lua <code>` evaluates a one-shot Lua snippet.
- `#profile save` saves the active profile to disk.
- `#profile load` replaces in-memory state with the saved profile.
- `#profile reset` wipes the profile back to defaults.
- `#import-tintin <path>` imports `#alias` and `#variable` from a TinTin++ `.tin` file.
- `#record <name>` starts recording typed commands into a macro alias.
- `#record` (no args) shows recording status.
- `#record cancel` discards the in-progress recording.
- `#endrec` stops recording and saves it as the named alias.
- `#qkey <name> <verb>` configures a quick-key.
- `#qkey clear <name>` removes a quick-key.
- `#qkeys` lists every quick-key binding.

The four target keywords are not slash commands. Type them bare.

- `tar` (no args) lists chars in the room.
- `tar <N>` picks the Nth char.
- `tar <substr>` picks the first char whose name contains the substring.
- `tarn` cycles to the next char in the room.
- `tarp` cycles to the previous char.
- `tarclear` clears the current target.

### 13.2 Keyboard shortcuts

Inside the command input.

- `Enter` sends the typed line.
- `Up` / `Down` walk command history. Type a prefix first to filter.
- `Tab` / `Shift+Tab` complete the current word (history, room chars, recent names).
- `Home` / `End` jump to the start or end of the input line. `Cmd+Left` / `Cmd+Right` on macOS.
- `Shift+Home` / `Shift+End` extend the selection.
- `Esc` cancels an in-flight paste burst or closes the split scrollback view.
- `PageUp` / `PageDown` scroll the terminal one page at a time. On macOS this is also `Fn+Up` / `Fn+Down`.

Inside the terminal pane.

- Click and drag selects text.
- `Ctrl+C` (`Cmd+C` on macOS) copies the selection.
- Mouse wheel scrolls up to open the split scrollback view.
- Middle-click anywhere closes the split scrollback and snaps to the live tail.
- Clicking anywhere that is not a button re-focuses the command input.

Map pane.

- `Ctrl+wheel` (or `Cmd+wheel`) zooms in and out.

Top bar.

- `[help]` opens this help window.
- `[settings]` opens the Settings window.
- `[map]` toggles the map pane.
