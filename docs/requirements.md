# Vosh 1.0 requirements

Vosh is a desktop MUD client for macOS, Windows and Linux. It targets Aabahran, a ROM 2.4 MUD at theforsakenlands.com, and works with any server that speaks the same protocols. It has to equal TinTin++ for power users and add a map pane, clean split panes and a tick timer.

This file says what 1.0 must do. It started as the kickoff prompt, which now lives in `docs/history/prompt.md`. Decisions taken since then changed some of its lines, and the last section says which ones and why. `CLAUDE.md` holds the stack and the rules for agents, and `docs/refactor-plan.md` holds the milestone plan and the status of each phase.

## Hard requirements

### Connection and protocol

- TCP and TLS connections.
- Telnet option negotiation with RFC 1143 state for each option, covering IAC, SB, SE, GA, EOR, DO, DONT, WILL and WONT.
- TTYPE answered with MTTS, NAWS, `NEW-ENVIRON`, CHARSET, EOR, ECHO for password lines, SGA and GMCP. Vosh turns down every other option, MSDP, MCCP and MXP among them.
- ANSI color, 256 color and 24 bit truecolor.
- Configurable charset per profile, UTF 8 by default with a latin 1 fallback.
- Auto reconnect with backoff and a manual override.

### Input and output

- Command line input with multiline edit, history and prefix search.
- Aliases with parameter substitution and recursion guards.
- Triggers using regular expressions with capture groups, ordered priority and an enable flag for each pattern. The syntax is that of the Rust regex crate, which matches in linear time, so no pattern can hang a session. It has no lookaround and no backreferences.
- Trigger actions of highlight, gag, replace, send, run script and route to pane.
- Variables and lists with profile and session scope.
- Speedwalk with `#walk`, which walks a route one step at a time and stops when the game stops you.
- Macros bound to keys with their modifiers, in groups you turn on and off.
- Every line logged with its colors to logs.sqlite, with a log switch for each profile. Save as file writes the logs you pick, one session or a span of days, as plain `.txt`, as ANSI `.log` or as one web page `.html`, with each line's time in front when you check Include times.
- A scrollback size in lines, set under Logs, from 1,000 to 100,000 lines with 10,000 by default. Regex search runs in the log view, and every line there shows its time.
- Command queue with a throttle that respects server rate limits.

### Display and layout

- Tabbed sessions, each bound to a profile and a server.
- Split pane output within a session, with at minimum a main pane, a chat capture pane and a status pane.
- Resizable panes that persist for each profile.
- Highlights that change foreground, background, bold, underline and inverse.
- Themes built on a small token system. Vosh ships a dark theme and a high contrast theme.

### Text rendering

- On macOS a native surface draws the terminal. Windows and Linux draw it with xterm.js.
- ANSI 8 and 16 color, the xterm 256 color palette and 24 bit RGB truecolor. All three render correctly when a server mixes them in one stream.
- Trigger highlights that compose with the game's color instead of stomping it. A base style colors only the text the game left in its default color, and a wash tints the whole line.
- Full UTF 8 input and output. Vosh renders the Basic Multilingual Plane and the Supplementary Multilingual Plane, emoji, CJK, Cyrillic, Greek and combining characters included.
- Correct grid handling for CJK double width characters, combining marks and zero width joiner sequences. No column drift after wide glyphs.
- Grayscale antialiased glyphs on the native surface. On Windows and Linux the web view draws glyphs as the system does.
- High DPI and Retina rendering with crisp glyphs at any zoom level.
- A configurable monospace font with a fallback chain. A missing character draws as a visible replacement glyph, never a silent drop.
- Font size, line height and letter spacing for each profile.
- Bold, italic, underline, strikethrough and inverse. Real bold and italic font variants win over synthetic styling.
- Ligatures off, to keep the grid aligned.
- A caret on the command line drawn as a block, an underline or a bar, with its own color and optional blink.
- Box drawing and block element characters render flush in the grid.

### Out of band data

- GMCP messages routed into a typed event bus that triggers, scripts and the page subscribe to.
- Built in handlers for the GMCP packages Aabahran sends, Char.Vitals, Char.Stats, Char.Affects, Char.Status, Room.Info and Comm.Channel among them.
- Variables filled from GMCP data and exposed to triggers and scripts under a stable namespace.

### Map

- A map pane inside the main window, drawn from the rooms and tiles the game sends.
- Rooms and exits drawn with the glyphs and colors of your theme, or with a bitmap tileset of your own.
- Click a room on the tiles the game sends to walk there with `#walk`.
- Vosh stores no map of its own.

### Tick timer

- Configurable interval and offset.
- A reset on events, from a regex on input or output or from a GMCP signal.
- A visible countdown in the status bar, with optional sound and a color flash near zero.
- Optional auto fire of an alias or a script on each tick.

### Scripting

- An embedded Lua engine exposed to triggers, aliases, key bindings and the GMCP event bus.
- Scripts loaded from Lua files in the scripts folder.
- Sandboxed, with no access to your files or the network.
- A small standard library covering string and regex helpers, JSON, time and a logger.
- Plugins, each a folder of Lua with a manifest. A Plugins page in Settings installs, writes, switches and removes them.

### Profiles and settings

- A config file for each profile in TOML. Vosh records each upgrade it makes to your files, so each one runs once.
- An import path for TinTin++ aliases and triggers where possible, with a clear list of what it cannot bring over.
- A Settings window, plus a text editor fallback for power users.
- Backup, restore and export of profiles.

### Distribution

- macOS builds for arm64 and x86_64, each signed and notarized.
- A Windows installer, MSI or NSIS. Windows builds of 1.0 ship unsigned for now, and README says how to get past SmartScreen.
- A Linux AppImage and a deb plus rpm pair.
- Opt in auto update.

## Soft preferences

- Native feel on each platform without giving up a consistent layout.
- Cold start under 1 second on Apple Silicon.
- Memory under 200 MB idle for one session.
- Scrollback and command history persist, so a session recovers after a crash.
- An optional accessibility mode with screen reader hints and large font defaults.

## Anti goals

- No required cloud account, no required login and no telemetry.
- No bundled adware, analytics or tracking.
- No proprietary script format. Scripts stay plain text, diffable and fit for version control.
- No feature that depends on a third party server we do not control.

## Writing style for user visible text

These rules cover README, CONTRIBUTING, settings labels, error messages, commit messages and any prose a player reads.

- Active voice. Address the reader as "you".
- No dashes of any kind in prose, no semicolons, no colons in body sentences, no asterisks for emphasis and no emojis.
- Direct and concise. Vary sentence length for rhythm.
- Concrete and specific over abstract. Definitive statements over conditionals.
- No filler phrases, such as "it's important to note", "let's explore" or "streamline".

## What changed since the kickoff

The kickoff prompt listed what 1.0 should do before any code existed. The refactor before 1.0 answered a set of decisions in October 2026, and the design reviews of October 3 reversed three of them. Each change below says what moved and why.

### Vosh keeps no map store

The kickoff asked for maps stored per area in SQLite with JSON export, pathfinding with avoidance flags, a manual fallback for MUDs without room data, and a right click on a room for notes, colors and tags. Vosh wrote every room to maps.sqlite, but nothing ever read the file back, while the pane drew from what the game sends. So the store retired. Vosh stores no map of its own, and maps.sqlite stays on disk unread, so nothing you had is lost. Room notes, pathfinding and a mapper of Vosh's own wait until after 1.0 and would build on what the game sends. The map is a pane in the main window rather than a window of its own, so the main window reads as one terminal.

### Click to walk and #walk come before 1.0

Retiring the map store first pushed speedwalk and click to walk past 1.0. The Scripts and Panels review of October 3 brought both forward, built on the tiles the game sends instead of a stored map. `#walk` walks a route one step at a time and stops when the game stops you, and a click on a room in the map pane walks there the same way.

### One log store and Save as file

The kickoff asked for log files with rotation and a toggle for each session. Vosh already logs every line with its colors to logs.sqlite, and search, the prompt lookup and the password wipe all read it. A running text file would add a second write to every line on the session loop the latency work trimmed, and it would keep the password lines the wipe only cleans in the database. So logs.sqlite is the one store. Save as file writes the logs you pick, one session or a span of days, as plain `.txt`, as ANSI `.log` or as one web page `.html`, each line with its time when you check Include times. A password line is hidden in every format and in Copy as text. A switch that also writes a running text log for a profile can come after 1.0, off by default.

### Scrollback in lines, with times in the log view

The kickoff asked for scrollback sized in lines or memory, with timestamps. Both renderers kept a fixed 10,000 lines. Now Scrollback size under Logs sets it, from 1,000 to 100,000 lines with 10,000 by default. Times stay in the log view, where every logged line already carries its time, because times in the terminal would need new work in both renderers.

### Plugins get a page in Settings

The refactor first dropped the plugin commands nothing called and left plugins as something you turn on in the profile file, since the kickoff put a plugin system among its stretch goals. The Scripts and Panels review reversed that. Plugins now have a page in Settings, where you install, write, switch and remove them, each with its own Lua environment and limits.

### Auto reconnect joins 1.0

The kickoff promised auto reconnect, and the Alerts and Scenes review of October 3 confirmed it for 1.0. After a drop while you play, Vosh dials the same world again with growing waits, from 3 seconds up to a minute, for 8 tries. You log in yourself, since Vosh stores no password. It never dials after your own Disconnect or quit, after a closing line from the game, or while Reconnect when the link drops is off.

### The protocols Vosh speaks

The kickoff listed MCCP2, MCCP3, MSSP, MSDP, ATCP and MXP beside the options Vosh speaks, and an MSDP fallback for servers without GMCP. Vosh speaks TTYPE with MTTS, NAWS, `NEW-ENVIRON`, CHARSET, EOR, ECHO, SGA and GMCP, and turns down every other option. None of the others ever worked, so the list now names only what Vosh speaks, the MSDP fallback left with MSDP, and README makes the same claim.

### The native surface stays on macOS

The macOS build draws the terminal on a native surface under the page. The same surface sat behind a hidden switch on Windows and Linux and had never run on real hardware there, so it left those platforms for 1.0. They draw with xterm.js as they always did by default, and the platform seam stays so a native surface can come to them later.

### Windows ships unsigned for now

Signing on Windows needs a certificate from an outside service, and a certificate costs money. So Windows builds of 1.0 ship unsigned, README says how to get past SmartScreen, and the release workflow keeps a place for the certificate so signing can follow without other changes.

### What waits until after 1.0

The review of October 27 checked every line of this file against the code. The promises below were never built, and no player has asked for them, so they wait until after 1.0.

- StartTLS. Vosh connects over TLS from the first byte, and turns the StartTLS option down as it does every option it does not speak.
- Inline images. No world Vosh targets sends them.
- Key chords and modal sets for macros. Groups you switch with `#group` already give one key a job in each set.
- A ligature switch and font smoothing choices. The native surface draws every glyph in grayscale to keep the grid exact.
- Glyph sets of your own on the map. A bitmap tileset covers the same need.
- Lua access to your files and the network. The sandbox stays closed, which suits the anti goals.
- One universal Mac app. Each chip gets its own build, and the updater fetches the right one.

### Lines that now say what Vosh does

The same review rewrote six lines so they describe what Vosh does rather than what the kickoff guessed. Triggers use the Rust regex syntax, since it can never hang a session. A highlight composes with the game's color through its base and wash choices, not a separate blend mode. The caret styles belong to the command line, where you type. The GMCP line names the packages Aabahran sends, which have no Char.Items.List and no Char.Skills.List. Scripts load from one scripts folder, and plugins from theirs. Profile files carry no schema number, because Vosh records each upgrade it makes and never runs one twice.

The review also put seven items on the plan for 1.0, each one a line this file already holds. They are command history that outlives a restart, a throttle on what Vosh sends, a charset for each profile, a tick offset and a reset from a line you type, JSON and regex helpers in Lua, `#list`, and a way to show your profile file and restore it from a backup.
