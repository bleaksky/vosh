# Vosh help

This file is the same content the in-client Help view shows. The Help view (top bar help button) is the primary place to read it. This file mirrors the same topics for offline reading and copy-out.

The source of truth for both is `src/lib/helpContent.ts`.

---

## Get connected

### 1.1 Connect to a world

Connections start from the session chip in the top bar. While idle the chip reads `connect` beside a status dot.

- Click the session chip.
- Fill in the host and port. The form defaults to `play.theforsakenlands.com` and `1848`. Tick the `tls` checkbox when your server offers TLS.
- Click `connect`. The dot shifts from connecting to connected.
- Type your character name at the login prompt and press `Enter`.
- When the server asks for a password, the input row swaps to a masked field with the placeholder `password`. Nothing you type shows on screen, echoes to the terminal, lands in command history, or reaches the session log. Vosh sends it exactly as typed, with no aliases, variables, or `#` commands applied. Press `Enter` to submit. `Shift+Enter` submits here too instead of adding a line.

While connected, the chip shows your character name in lowercase along with the host and port. If a saved profile matches the host and port you dialed, Vosh switches to that profile before connecting. A profile pinned to a character attaches right after login, when the server reports who you are.

To disconnect, click the chip and press `disconnect`, or right click the terminal and choose `disconnect`.

### 1.2 Reconnect

The session chip reports connection state through its status dot. The dot turns to its error state when the connection fails and back to idle when the session closes cleanly.

- Click the chip and press `connect` to dial the same host and port again.
- Scroll up or press `PageUp` to read output from before the drop. The terminal scrollback survives a disconnect, and nothing clears it unless you pick `clear buffer` yourself.
- To stage commands while offline, type the first command, press `Shift+Enter` to stack more lines under it, and leave the block in the compose box. After you reconnect, press `Enter` once and each line submits separately, in order.

Two things reset on a disconnect. The chat pane buffer empties the moment the session drops, and session variables set with `#var` clear when the next connection opens, so they never carry into a new session. Aliases, triggers, macros, and profile variables stay loaded because they live in your profile, not in the connection.

`disconnect` lives in three places. The session chip form while connected, the terminal right click menu, and the `Cmd+K` palette.

### 1.3 Save your profile

`#profile save` writes the current client state to the active profile file, and the profile loads again on startup with no extra step. The file is a TOML snapshot under `~/Library/Application Support/com.aabahran.vosh`.

- Set up the client state you want to keep. Aliases, triggers, macros, variables, and tick settings all count.
- Type `#profile save` in the input bar. Vosh writes the snapshot to the active profile TOML.
- Or press `Cmd+K` and run the `#profile save` palette entry. It sends the same command.

The snapshot covers connection defaults, aliases, profile variables, triggers, tick configuration, macros, ui settings, enabled plugins, and the groups you disabled.

Variables set with `#var` live in session scope. They clear when the next connection opens and never reach the file. A lasting value belongs in the `profile_vars` table of your profile file at `profiles/<name>.toml` in the app data folder. Edit it there while Vosh is closed, or set the value from Lua with `mud.set_profile_var`.

`#profile load` pulls the saved file back into the live session. In loadout mode the profile commands become notices instead, because loadout mode saves your changes automatically.

## Play

### 2.1 Send commands

The command input sends lines to the server. It handles single commands, chained commands, multi line blocks, and pastes.

- Type a command and press `Enter` to send it.
- Chain commands on one line with `;`. Each piece goes out as its own command. Type `\;` for a literal semicolon.
- Press `Shift+Enter` to add a line without sending. The box grows and a line number gutter appears once it holds two or more lines. Press `Enter` and every line submits separately, in order, with blank lines dropped.
- Press `Enter` on an empty box to send a bare line. Many MUD prompts advance on that.
- Paste multi line text straight into the input. A single line submits immediately. Two or more lines become a paste burst, sent one line every 500 ms by default, with a `paste N/M esc cancels` counter beside the prompt.
- Press `Esc` during a burst to cancel every line that has not gone out yet. Starting a new paste also cancels the old burst.

With the `keep last command` setting on, a sent command stays in the box fully selected. Press `Enter` again to resend it, or start typing to replace it.

The burst delay is configurable from 0 to 10000 ms.

### 2.2 Recall command history

Command history records every line you send during a session and replays it from the input bar.

- Press `Up` in an empty input to step back through sent commands, newest first.
- Type a few characters before pressing `Up` to turn recall into a prefix search. Only lines starting with that text cycle past.
- Press `Down` to step toward newer matches. One step past the newest restores exactly what you had typed before the search began.
- Edit the recalled line at any point. Editing ends the search, and the next `Up` starts a fresh one from whatever is now in the box.

History skips consecutive duplicates and never records anything you type in password mode.

With the `keep last command` setting on, the line you just sent stays in the box fully selected. `Enter` resends it and typing anything replaces it.

In a multi line compose the arrows do their normal job first. `Up` moves the caret up a line unless you are already on the first line, and `Down` moves it down unless you are on the last, so history recall fires only from the edges of the block.

Example. Type `tell` and press `Up` to cycle through only the lines that start with `tell`.

### 2.3 Complete names with Tab

Tab completion finishes a partly typed word in the command line from names Vosh already knows.

- Type the first letters of the word anywhere in the command line.
- Press `Tab`. Vosh completes the word under the caret with its best match.
- Press `Tab` again to cycle through the remaining candidates, or `Shift+Tab` to cycle backward. The list wraps around.
- Keep typing, or press any other key, and the cycle resets with the current completion left in place.

Candidates come from three sources, checked in this order.

- Words from commands you have typed, most recent first.
- Characters in your room, when the server sends `Room.Chars` over GMCP.
- Capitalized names Vosh spotted in the output during the last 30 minutes.

Matching is a case insensitive prefix match, duplicates collapse, and Vosh skips a candidate identical to what you already typed. Completion works on the word under the caret, so you can edit the middle of a line without touching the rest.

### 2.4 Scroll back through history

Scrollback opens in a split pane above the live terminal, so old output stays readable while new output keeps flowing underneath.

- Scroll the mouse wheel up over the terminal. The first notch opens the scrollback split above the live pane, and further scrolling walks the history line by line.
- Or press `PageUp` to open the split and page upward, then `PageDown` to page back down. A Mac keyboard produces these with `Fn+Up` and `Fn+Down`.
- Or press `Cmd+\` on macOS or `Ctrl+\` elsewhere to open the split, and press it again to close it. The palette lists it as `Split terminal`.
- Read the `↑ N / max` badge at the top right of the history pane to see your depth.
- Drag the divider between the panes to resize the split. It snaps to whole terminal rows. The handle also answers the keyboard, arrow keys nudge it 16px and `Shift` with an arrow jumps 64px.
- Return to live three ways. Scroll or page down until history reaches its bottom and the split closes itself. Press `Esc`. Or middle click the terminal.

The live pane never scrolls away while the split is open. New output keeps landing at its tail, and the lines you type mirror into the history pane so the record stays continuous.

On the native macOS renderer there is no split. The surface scrolls its own grid, and `PageUp`, `PageDown`, and `Esc` still page it and snap it back to the bottom.

### 2.5 Find text

The find toolbar searches the whole session scrollback. It drops over the top right of the terminal.

- Press `Cmd+F` on macOS or `Ctrl+F` elsewhere. The toolbar opens even while you are typing in the command input.
- Type your query into the `find in scrollback` box.
- Press `Enter` for the next match and `Shift+Enter` for the previous one. The `↑` and `↓` buttons do the same jobs.
- Read the badge beside the box. It shows `N / M` while you step through hits and `no match` when the query finds nothing.
- Narrow the query with the three toggles. `case` makes it case sensitive, `word` matches whole words only, and `regex` treats the query as a regular expression.
- Press `Esc` to close the toolbar, clear every highlight, and return focus to the command input.

A match above the visible screen opens the scrollback split with the hit highlighted near the top of the history pane. A match already on screen closes any open split instead.

The toolbar also opens from the terminal right click menu item `search scrollback` and from the `Cmd+K` palette.

### 2.6 Copy terminal text

Terminal text copies to the system clipboard through a drag selection.

- Drag across the output you want. An active text selection stops the usual click from refocusing the command input, so the selection stays put.
- Press `Cmd+C` on macOS or `Ctrl+C` elsewhere.
- Or right click the terminal and choose `copy`. The menu shows the `⌘C` shortcut beside it.
- To copy everything, press `Cmd+A` on macOS or `Ctrl+A` elsewhere while the command line is empty. It selects the whole terminal, scrollback included, and `Cmd+C` then copies it. The right click menu item `Select all` does the same.

One priority rule. When the input box itself holds a selection, `Cmd+C` copies that selection rather than the terminal. Clear the input selection, or use the right click `copy` item, when the terminal text is what you want.

The right click menu's `paste` item inserts the clipboard into the input row without sending anything. Edit the line as needed, then press `Enter` yourself.

On the xterm renderer the right click menu also offers `clear buffer`, which wipes the terminal buffer.

### 2.7 Use the command palette

The command palette runs Vosh actions from the keyboard. It covers commands, pane toggles, settings tabs, and aliases.

- Press `Cmd+K` or `Ctrl+K` to open the palette. The same shortcut closes it again.
- Type a few letters to filter. Entries whose title starts with your text rank first, then title substrings, then hint and keyword matches.
- Move the selection with the arrow keys and press `Enter` to run the highlighted entry.
- Press `Tab` or `Shift+Tab` to cycle the scope filter and show a single group.
- Press `Esc` to close without running anything.

The palette holds four groups.

- Commands. `connect` or `disconnect` depending on state, `open splits` and `close splits` for the well panes, `search scrollback`, `#profile save`, and `open help`.
- Panes. A show or hide toggle for each panel (`map`, `group`, `vitals`, `roomstrip`, `chat`, `affects`, `combat`, `imm`).
- Settings. One entry per Settings tab. Each opens the Settings window already on that tab.
- Aliases. Every enabled alias. A parameterless alias runs the moment you pick it. An alias that takes arguments inserts its name into the input row instead, so you finish the line and press `Enter`.

### 2.8 Use the right click menu

The terminal right click menu collects the terminal's everyday actions in one place.

- Right click anywhere on the terminal to open it.
- `copy` copies the current selection. The menu lists its `⌘C` shortcut.
- `paste` inserts the clipboard into the input row. Nothing sends until you press `Enter` yourself.
- `Select all` selects the whole terminal, scrollback included. The menu lists its `⌘A` shortcut, which works while the command line is empty.
- `open splits` and `close splits` toggle the session, chat, and log panes inside the terminal well. These are workspace panes, separate from the scrollback split.
- `search scrollback` opens the find toolbar.
- `clear buffer` wipes the terminal. The item appears only on the xterm renderer. The native macOS grid has no clear command, so the item hides there.
- While connected, `disconnect` sits at the bottom in danger styling and closes the session.

The menu closes on `Esc`, on a click anywhere outside it, or the instant you pick an item. It clamps itself to the window edges, so a right click near a corner never opens it half off screen.

## Automate

### 3.1 Create an alias

Aliases expand a short name into one or more commands. They live in the settings window under `aliases`, and the input bar defines them too.

- Click the gear button in the top bar to open settings, then pick the `aliases` tab.
- Click `+ alias` to add a blank row.
- Enter a name in the name field.
- Enter the expansion in the expansion field. `;` splits the expansion into separate commands and `\;` keeps a literal semicolon.
- Click `save`. The unsaved dot clears and `saved.` appears.

Captures pull words from the line you typed. `%1` through `%9` pull the first through ninth word after the alias name. `%0` pulls the whole tail, `%1-` pulls word one through the end with spacing intact, and a missing word expands to nothing. `%%` gives a literal percent.

Give related aliases a shared group name to toggle them as a folder, either with the group checkbox in the tab or with `#group <name> on|off`. The `tpl` button on each row switches the expansion to a Lua script body.

Example. An alias named `kk` with the expansion `kick %1; backstab %1` turns `kk dragon` into `kick dragon` followed by `backstab dragon`.

The input bar defines aliases too. `#alias gc get all corpse` sets one and echoes `alias gc set`, `#aliases` lists every alias, and `#unalias gc` removes one. Setting an alias again, with `#alias`, `#endrec`, or `mud.alias` in Lua, keeps it in its group.

### 3.2 Create a trigger

Triggers watch incoming lines and run actions when a pattern matches. They live in the settings window under `triggers`, and a trigger pairs one visual with any number of effects.

- Open settings and pick the `triggers` tab. Keep the editor pill on `form`.
- Click `+ trigger` to add a card.
- Enter a name and a pattern. Patterns are regexes, so escape literal punctuation. `+ pattern` adds more rows and the card fires when any enabled row matches.
- Leave priority at `5`, the default for a new card, or raise it to run before other triggers. Higher priority triggers run first. Leave the target on `line`.
- Pick a visual. The chips are `none`, `highlight`, `replace`, and `gag`.
- Add effects with `+ send`, `+ route`, and `+ script`. Each button opens an editor for its text. Send and replace templates reach capture groups with `$1` through `$9` or `${name}`, and `;` splits a send into separate commands.
- Click `save`.

Example. The pattern `(\w+) is DEAD!` with a send of `get all corpse` loots each kill as the death line arrives.

The input bar builds triggers too. `#trigger name {pattern} send command` creates one at priority 0 on the `line` target, `#triggers` lists everything by priority, and `#untrigger name` removes one. Vosh rejects an invalid regex and names the broken pattern.

### 3.3 Highlight lines

A highlight trigger restyles every line that matches a pattern. Define one from the input bar with `#trigger` or from the settings `triggers` tab.

- Type `#trigger <name> {pattern} highlight <color> [styles]`. Every line matching the pattern renders in that color and style.
- Add `wash` to the style list to tint the whole line instead of restyling the text alone.
- Type `#triggers` to confirm the pattern and action. Defining a trigger under an existing name replaces it.

A plain highlight restyles the text. A wash tints the whole line with a dim quarter strength version of the highlight color, fills it edge to edge, and draws an accent bar at the left edge. Wash pairs well with a quiet color, since the tint covers the entire line.

Colors take the sixteen ANSI names. `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, and `white`, plus a `bright_` variant of each. `purple` maps to magenta and `gray` to `bright_black`. Stack `bold`, `underline`, and `inverse` freely, and add `bg:<color>` for a background.

Example. `#trigger tell-glow {tells you} highlight bright_yellow bold` renders every tell bright yellow and bold. `#trigger tell-glow {tells you} highlight bright_yellow wash` replaces it with a full line wash.

The settings `triggers` tab offers the same options with selects. Pick the `highlight` visual chip on a trigger card, choose fg and bg from the color lists, and check `full-line wash`.

### 3.4 Route lines to a pane

A route effect sends matching lines to a named pane. Build one on a trigger card in the settings `triggers` tab.

- Open settings, pick the `triggers` tab, and click `+ trigger`.
- Enter a name for the card.
- Enter a pattern. Click `+ pattern` to add more rows. The trigger fires when any enabled row matches.
- Click `+ route` and enter the pane's name in the pane field. `chat` is the placeholder the editor suggests.
- Click `save`. Matching lines land in the pane.

Route is an effect, so it stacks with anything else on the card. Pair it with a `highlight` visual to color the line, or add a `+ send` effect beside it.

Place the pane where you want it in the `panels` tab. Each panel row has a zone select drawn from the zones that panel supports, and the schematic preview up top moves chips with chevrons so you can order panels within a zone. The `side panels span full height` checkbox decides whether side panes run past the input row.

Example. A card named `chat-feed` with the patterns `tells you '` and `gossips '` and a route to `chat` collects tells and gossip in the chat pane.

The inline form is `#trigger chat-feed {tells you '} route chat`. It creates a single pattern trigger, so build multi pattern feeds in the settings tab.

### 3.5 Set and use variables

Variables store values you reference in commands as `$name`. Set them from the input bar with `#var`, and Vosh expands them in the lines you type before they leave.

- Type `#var <name> <value>` to set a variable. Vosh echoes `var <name> set`.
- Reference it in any command as `$name`. The line expands before it leaves, so the server receives the value.
- Type `#var <name>` to check a value, `#vars` to list them all, and `#unvar <name>` to remove one.

`$name` works when the name ends at whitespace or punctuation. Wrap the name in braces, as `${name}`, when letters follow immediately. `$$` sends a literal dollar sign, and unknown names pass through untouched, so `$100` reaches the server as typed.

Interpolation runs on the line you type, before alias expansion, and Vosh does not interpolate alias output again. Put variables in the line you type, or resolve them in a Lua script body instead.

`#var` writes session scope, which clears when the next connection opens, so a session value never survives into a new session. Profile variables persist across restarts in your profile TOML under `profile_vars`, and a session value shadows a profile value of the same name.

Vosh also fills session variables on its own. GMCP binds `hp`, `maxhp`, `char_name`, `room_name`, `target_name`, and more, and setting a target with `tar` mirrors it into `$target`.

Trigger send templates use `${name}` for regex capture groups, not this store, and trigger sends skip interpolation entirely.

Example. `#var potion yellow` followed by `quaff $potion` sends `quaff yellow` to the server. With a target set, `cast dispel $target` aims at your current mark.

### 3.6 Bind keys to macros

Macros bind a key to a command that fires while the input bar has focus. They live in the settings window under `macros`.

- Open settings and pick the `macros` tab.
- Click the key field in the top row. It reads `press a key...` while capturing.
- Press the key you want. The field records the canonical key name. Capture accepts function keys, modifier combos like `Ctrl+N`, numpad keys like `Numpad7`, and plain printable keys.
- Enter the command. `;` chains multiple actions.
- Enter a group name to put the binding in a toggleable folder, then click `add`.

Rows apply on add or save, so the tab has no separate save step. Existing rows carry their own `save` and `delete` buttons. A binding fires only while the input bar has focus.

Turn on `echo macro commands` in the `general` tab's `macros` row to make each press show what it sent. Group headers carry an `enabled` checkbox, and `#group <name> on|off` flips macro groups from the input bar along with matching alias and trigger groups.

Example. Bind `F1` to `stand; flee` and pressing `F1` in the input bar sends both commands.

`#record` builds something different. It captures the commands you type and saves them as an alias you invoke by name, not by key. Use the macros tab when you want a key, `#record` when you want a word.

### 3.7 Use slash commands

Slash commands drive Vosh from the input bar without opening settings. Vosh handles every line that starts with `#` locally, and it never reaches the MUD.

- Type `#help` any time for the full list.
- Manage aliases with `#alias <name> <expansion>`, `#unalias <name>`, and `#aliases`.
- Manage variables with `#var <name> [value]`, `#unvar <name>`, and `#vars`.
- Manage triggers with `#trigger <name> {pattern} <action>`, `#untrigger <name>`, and `#triggers`.
- Tell Vosh how to read your prompt with `#prompt game {setting}` and `#prompt fight {setting}`, the codes you type in the game, or with `#prompt {regex}`, each named group like `(?<hp>\d+)` a value. `#prompt` alone says how Vosh reads it, and `#unprompt` stops.
- Pick where your prompt shows with `#prompt show text|lifted|pinned`.
- Use Vosh's default prompt design with `#prompt default`. It takes the place of the design in this profile, and Vosh keeps yours as an earlier design.
- Flip whole folders with `#group <name> on|off` and inspect them with `#groups`.
- Tune the tick with `#tick`, `#tick interval <secs>`, `#tick warn at <secs>`, and the rest listed under `#help`.
- Record a command sequence with `#record <name>`, finish with `#endrec`, abort with `#record cancel`.
- Configure quick keys with `#qkey <name> <verb>` and list them with `#qkeys`.
- Drive Lua with `#script load <name>`, `#script reload`, `#scripts`, and `#lua <code>`.
- Snapshot with `#profile save`, `#profile load`, and `#profile reset`.
- Import TinTin++ files with `#import-tintin <path>`.
- Clear old passwords out of your session log with `#logs forget-passwords`, then `#logs forget-passwords now`.
- Work targets with `#target <args>`, or bare `tar`, `tarn`, `tarp`, and `tarclear` with no `#` at all.
- Switch renderers with `#nativesurface on|off|default`, applied on restart.

An unknown command echoes a pointer to `#help`, and errors come back wrapped in square brackets.

### 3.8 Script Vosh with Lua

Lua scripts run inside Vosh and register automation through the global `mud` table. Script files live in the `scripts` folder under the app data directory, `~/Library/Application Support/com.aabahran.vosh/scripts/` on macOS.

- Save a `.lua` file in the `scripts` folder.
- Type `#script load <name>` to load it. Vosh appends `.lua` to a bare name.
- Type `#scripts` to see loaded scripts and the triggers they registered.
- After editing a file, type `#script reload` to run every loaded script again.
- Run one liners with `#lua <code>`.

Scripts talk to Vosh through the global `mud` table. `mud.send(text)` goes straight to the server and `mud.input(text)` feeds back through the input pipeline. `mud.echo(text)` prints locally. `mud.alias(name, expansion)` and `mud.trigger(name, pattern, callback)` register automation, with `captures[1]` holding the full match and `captures[2]` onward the groups. `mud.on_gmcp(package, callback)` hands you server data as a table, and `mud.timer(secs, callback)` schedules work you can cancel with `mud.cancel_timer`.

Loads from `#script load` last for the session. For autoload, make a plugin. Create `plugins/<slug>/` under the app data directory with a `manifest.toml` naming the plugin and its entry script, `main.lua` by default. Enabled plugin names persist in your profile TOML under `[plugins]`, and every enabled plugin loads at launch. Enabling runs immediately, disabling takes effect next launch.

The sandbox strips file and process access. `require`, `io`, and `os.execute` are gone.

Example. `#script load combat` loads `combat.lua` from the scripts folder, and `#lua mud.echo("hello")` prints a line locally.

## Shape the window

### 4.1 Arrange the panels

The `panels` tab in the settings window controls where every panel sits. Column widths resize by dragging in the main window. The eight panels are `map`, `group`, `vitals`, `roomstrip`, `chat`, `affects`, `combat`, and `imm`.

- Click the gear in the top bar, titled `settings`, and open the `panels` tab.
- Pick a home for each panel with its `zone` select. Zones are `top`, `bottom`, `left`, `right`, and `hidden`.
- Set `align` for a left or right panel. `top` panels stack down from the column top and `bottom` panels stack up from the floor. The map hides this select because it always fills the leftover column height.
- Reorder panels within a zone with the up and down chevrons on the layout map chips.
- Tick `side panels span full height (input lives under terminal only)` to run the side columns to the window's bottom edge.
- Resize a column from the main window by dragging the 8px channel between the terminal and the column. A small ember tick fades in on the handle when you hover. Tab to a handle and press an arrow key to nudge it 16px, or `Shift` plus an arrow for 64px.

Changes save live. `reset to defaults` restores the stock layout.Panels render as raised cards with 8px of exposed ground at every seam, so the layout stays clean when you move one.

The command palette on `⌘K` (`Ctrl+K` off macOS) lists a `show <id> pane` or `hide <id> pane` entry for every panel. A panel you show again returns to its last visible zone.

### 4.2 Use the map

The map pane draws the server map. It fills the right column by default, and its header names the current area once room data arrives.

- Move the map with its `zone` select in the `panels` tab. It accepts `left`, `right`, or `hidden` only, since a horizontal map at full width is unusable.
- Toggle it with the map button in the top bar. The button flips the map between hidden and its last visible spot.
- Read the header. It says `map` until the first room push arrives, then `map · <area>` with the area name from `Room.Info`.
- Click the sliders button labeled `map controls` to open the controls row. Vosh remembers whether you left it open.
- Switch draw modes with `squares`, `glyphs`, or `tileset`.
- Zoom with `−` and `+`, or hold `Ctrl` and wheel over the map. The readout shows the percent and `⤺` resets it.
- Read the status text. `radius N` means live map data. `waiting for server map` means none has arrived yet.

Tileset mode adds a bar with `load tileset` and `clear` buttons for your own tile art.

### 4.3 Use the chat pane

The chat pane collects channel talk in its own buffer, one line per message. Add it with `Add a pane` in the title band, or pick `Show here instead` in any pane's menu.

- Lines arrive on their own. `Comm.Channel` GMCP feeds the pane automatically.
- Read a line as `[tell] Selune: meet at the bank`. The tag names the channel and the speaker is bold, even a name of several words like `a Blackwatch villager`. Wrapped lines hang two cells in, so the tags run down the left edge.
- Each line takes the color the game prints that channel in, from your theme's terminal colors. Say is bright yellow, tell green, gtell bright magenta, yell cyan, pray bright white, cabal bright blue, clan bright cyan, faction yellow, newbie bright green, immortal bright red, and imp bright cyan. Switch themes and the chat follows.
- Point at a message to see when it arrived.
- Filter with the channel select beside the pane's name. `All` shows every channel. Each chat pane keeps its own filter, so you can split one off for tells alone.
- Route trigger output in. Add a `route` effect to a trigger under Automation, then Triggers, and enter a pane name. Those lines land in the chat pane under that name, in their own words.
- See the tells you send. The game sends no GMCP for them, so route the terminal line in. A trigger on `^You (tell|project to) ` with a route to `tell` makes each one read `[tell] to Selune: text`. It catches the tells a telepath projects too. The pane skips the `You tell your group` line the trigger also catches, because your gtell already arrives over GMCP.

The buffer holds a rolling 500 lines, survives closing and reopening the pane, and clears only on disconnect. The pane sticks to its tail. Scroll up to read back, and it sticks again once you come within 24px of the bottom.

### 4.4 Configure the vitals readout

The vitals readout shows hp, mana, and moves. Configure it in the `vitals` tab in Settings, where changes save automatically and you can drag the live preview's bars to scrub the numbers.

- Pick a layout. The options are `ember`, `stacked`, and `inline`. Ember draws a sidebar pane with a `vitals` head, the tick countdown beside it, and three fixed thin bars with mono current and max numbers.
- Outside ember, choose columns with the `bar`, `percent`, `numeric`, and `delta` pills, set `bar style` to `solid`, `ramped`, or `track`, and pick bar glyphs and width.
- Open the `advanced` disclosure to recolor hp, mana, and moves, or turn on `drain through red as bars empty`.
- Tick `pulse red vignette under 30% hp` to pulse a red vignette when hp drops under 30%.
- Turn on `custom template (overrides layout)` to write the readout yourself with tokens like `%hp`, `%pct_hp`, `%bar_hp`, `%tick`, and `%time`. Any `Char.Vitals` or `Char.Worth` field resolves as `%fieldname`, and `%%` prints a literal percent.

Place the bar in any zone from the `panels` tab. It ships in the right column, listed as `vitals (hp bar)`. Tracked affects live in Settings under Characters, then Tracked affects.

`reset vitals` restores the stock config.

When the game hides your vitals, as it does under lamented tears, every value reads `?` in dim text over an empty meter, in the panel and on the status line alike. Nothing turns yellow or red while they stay hidden. Your numbers come back with the next update the game sends. In a fight the opponent row reads `?` the same way when the game hides its health, and the status line drops the health of your target.

### 4.5 Watch your group and affects

The group pane shows member health while grouped and your worth while solo. The affects pane shows what affects you and the hours each has left. The group pane sits at the top of the right column, and its header shows `group` plus a member count, or `solo`.

- Read the group rows. While grouped, each member gets a row with a mono name, a 44px hp mini bar, and the percent. The row tone drops through three tiers, healthy at 67% and up, warning down to 34%, danger below.
- While solo, the pane shows your worth instead. The fields are tnl, exp, gold, bank, trains, and prac.
- Pick the affects you track in Settings under Characters, then Tracked affects. Choose `Add affect…` and pick one of the affects on you now, or type its name. Matching ignores case and extra spaces. Under Advanced you can give a tracked affect a short label like `sanc` to show in its place, and set the order of your slots.
- Pick how the affects pane draws in Settings under Layout, then Affects, or from the pane's own menu under `Style` and `Marker`. Each character keeps its own. `Timers first` is the default. `Countdown` lists every affect by the hours it has left. `Grouped chips` puts what to recast first.
- In `Timers first`, read the affects pane in two columns. Each entry shows the hours left, then the name exactly as the game sends it. `+` means permanent and `-` means you do not have it, the same marks the game uses in its own affects bar. A pane narrower than about 360 pt shows one column.
- Your tracked affects fill the top rows in your order and keep their places as the hours change. The dot beside each agrees with its hours. Green is up, yellow has two hours left, and red has one hour or none. A hollow red ring and a red name mean you are missing it.
- Everything else sits under a thin line. Harmful affects like `faerie fire` come first, with a red diamond and a red name. The rest follow by hours left, down the left column and then down the right.
- In `Countdown`, the tracked affects you are missing come first, then every affect by the hours left, down the left column and then down the right. A thin meter under each drains as its hours run down.
- In `Grouped chips`, `Recast` holds the tracked affects you are missing and the ones running out, `Tracked` the rest you track, and `Other` everything else. A missing affect is a dashed red chip. A tracked chip is filled, and the fill drains from the left as its hours run down. One running out turns yellow at two hours and red at one or none. Other affects sit in outlined chips, and harmful ones in red.
- The meters and the fills measure each affect against the most hours Vosh has seen for it since you last cast it, and Vosh remembers that for each character between logins. An affect Vosh first sees partway through starts full, and a permanent one stays full.
- `Marker` sets the mark beside each affect you track in `Timers first` and `Countdown`. Pick a dot, a square, plus and minus, or none. Plus and minus shows a plus while you have the affect and a minus while you miss it. The color shows the state in every shape, and with none the hours and the red names still do.
- Turn on `Tint what to recast` to put a missing affect on a red wash and one about to drop on yellow or red, in `Timers first` and `Countdown`. `Grouped chips` always marks what to recast.
- The hours follow the game. One hour or none reads in bold red, and two in yellow. The header counts the tracked affects you are missing and the ones running out.
- The pane shows whole rows only. When some do not fit, the last entry says how many more there are, like `5 more`. Click it to scroll to them, and point away to scroll back.

Group data arrives from `Group.Info` and worth from `Char.Worth`. Affects come from `Char.Affects`, which the game sends when you log in, whenever an affect changes, and every tick. With no group the pane says so and suggests `follow <name>` to start one.

When the game hides your affects or your group, as it does under lamented tears, the pane says so in place of its rows. The affects pane marks no tracked affect missing, and the group pane shows no member health from before. Each fills in again with the next update the game sends.

### 4.6 Read the room strip

The room strip is a one line summary of your location. It runs along the top of the window and shows the area, the room name with vnum, the terrain, and the exit list in `N E S W U D` order.

- Read the `here` chips for who is in the room. Character chips color NPCs and players differently, stacks collapse to `(N) name`, and your current target carries a `▶` marker.
- Read the `items` chips for what is on the ground. Each colors by type, so money, weapons, armor, potions, and food read apart at a glance.
- Move the strip from the `panels` tab, where it lists as `room strip (area info)`. In a left or right column it switches to a column variant that wraps onto multiple lines instead of scrolling sideways.

GMCP feeds everything. `Room.Info` names the room, `Map.Tiles` colors the area, `Room.Chars` and `Room.Items` fill the chips, and the target marker follows the backend target.

In the top strip, overflowing content fades out at the right edge. An empty slot holds its height so the layout never jumps.

### 4.7 Split the well

Splits divide the terminal well into three panes, the main session, channel chat, and a raw log tail. These are workspace panes, separate from the scrollback split.

- Open them from the palette. Press `⌘K` and run `open splits`, or right click the terminal and pick `open splits` from the menu.
- Read the pane chips. The main terminal takes `1` plus a session name drawn from the host, `theforsakenlands` on the default world, or `session` before you connect.
- Watch channel talk in the `2 chat` pane, colored per channel with dim `HH:MM` timestamps.
- Watch raw session output tail through the `3 log · raw` pane below it.
- Read the status bar. While splits are open its center lists `1 session`, `2 chat`, and `3 log`, with the active pane marked.
- Run `close splits` from the same palette entry or menu item to fold back to a single well.

The choice persists across launches. Toggling never remounts the terminal, so the session never flickers. Both side panes stick to their bottom edge. Before any traffic the chat pane reads `no channel chat yet` and the log pane reads `quiet — raw session output tails here`.

### 4.8 Use the imm board

The imm panel lists staff queues sorted worst first. Its home is the right column, and it fills only on an immortal login.

- Show the panel with `⌘K` and `show imm pane`, or place `imm` from the `panels` tab, where it lists as `imm (staff queues)`.
- Log in on an immortal. The server lights the panel at login with an `Imm.Queues` push. On a mortal it reads `no staff feed`.
- Read top down. Only queues with work appear, overdue sorts above nearing, and bigger counts rise. The queues are `dcheck`, `applications`, `journals`, `votes`, `notes`, `bugs`, `penalties`, `ideas`, and `typos`.
- Read the chips. Rows trail `N overdue` and `N nearing`, applications add `N unread`, and journals add `N unawarded`.

The header sums the board as `N overdue`, `N nearing`, or `clear`. A count flashes when it grows. An empty board after the feed reads `all clear`.

### 4.9 Choose where your prompt shows

Once Vosh reads your prompt, you choose where it shows. Open Settings, choose Input, open `Advanced`, and pick a place under `Where your prompt shows`.

- `In the text` shows each prompt where the game sends it. The terminal reads as it always has.
- `Lifted` keeps every prompt in the text on a raised band in the selected row color of your theme, scrollback included. A prompt that ends on a character gains one space after its band, so your echo never touches it.
- `Pinned` takes your prompts out of the text and shows your latest one on a band above the command line. The band is only as tall as your prompt. When a fight adds a row, the text above gives up its top line to make room and gets it back when the fight ends, so your newest line always sits right above the band. Every prompt still reaches the session log and your Prompts triggers.

From the input bar, `#prompt show lifted` picks the same place, and `text` or `pinned` in its place picks the others. `#prompt` alone also says where your prompt shows.

The choice needs Vosh to read your prompt. Until it does, the row stays off and says what to do first, and `#prompt show` tells you to type `#prompt game` with your prompt setting in braces. While you have prompts off in the game, the pinned band says so and shows nothing else.

With the xterm renderer, the newest 1000 prompts keep their bands and older ones show plain. The split history pane shows them plain too. The native renderer keeps a band on every prompt in the scrollback.

The choice saves in the `[prompt]` table of your profile as `show`. An older version of Vosh ignores it and shows your prompt in the text. When that version saves your profile, the choice is gone, so pick it again here.

## Tick and target

### 5.1 Configure the tick timer

The tick timer shows the game's tick in the status line under the input row, with the game time and the moons beside it. The game's own tick decides when it fires. Vosh knows the game ticked when the game hour moves, which Aabahran advances once a tick, or when a line matches your `Reset on` pattern. When the tick lands, the count restarts, the sound plays, and your `Send each tick` command goes out, once per tick. Configure it in Settings under Automation, then Timers, where `Tick` sits at the top of the list, and click `Save` to apply your changes.

- Turn on `Enabled`. Every connection starts the tick, and switching characters keeps it running until you turn it off. `Play a sound` under `Advanced` plays a sound when the tick lands.
- Set `Every` in seconds, anywhere from 1 to 3600. It is how long you expect a tick to take. Aabahran picks each tick between 25 and 35 seconds, so once the game ticks, the timer waits for the game instead of firing at `Every`.
- Put a command in `Send each tick` to send it on every tick. Leave it blank for none.
- Give `Reset on` a regex. A line that matches is the tick. A signal within 2 seconds of a tick counts as that tick, so a matching line and the game hour moving together fire once.
- Before the game's first tick in a session, and on a game that never tells Vosh when it ticks, the timer fires on its own every `Every` seconds. When the game goes quiet for twice `Every`, the timer fires once on its own and keeps its own time until the next tick comes in.
- The tick in the status line turns the warn color on a soft ground in its last 5 seconds, and stays that way while the game runs late. Turn on `Warn before it fires` to also print a warning line in the terminal, and set `Warn at` to how many seconds of lead you want. The status line then follows the same lead. The terminal prints the warning once per tick. Fill `Warning text` and `Warning color` to restyle the warning line the terminal prints. The color takes an ANSI name, `#rrggbb` hex, or a 256 palette index, and blank keeps the defaults.
- While the game runs late, the tick pulses gently in the warn color until the tick lands. With Reduce motion on in your system settings, it holds still in the warn color.
- Pick which way the tick counts in Settings under Layout, then Status line, in the Tick counts row. `Up` shows the seconds since the last tick and keeps counting past `Every` while the game runs late, like `31s`. `Down` shows the seconds left until the tick, from `Every` right after one down to `1s` in its last second, and waits at `0s` when the game runs late. `Down past 0` counts down the same way and keeps counting below zero until the tick lands. An early tick restarts either count at once.
- Pick how the status line shows the tick, the time, and the moons in the Tick and time row just above it. `Value` shows each value alone, like `14s` and `8:42`. `Caption` puts Tick, Time, and Moons before them. `Icon` puts a ring before the tick. Counting up it fills clockwise as the seconds pass, closes when the tick is due, and stays closed while the game runs late. Counting down it shows the time left and empties clockwise toward the top, and only the faint ring shows while the game runs late. Before the time it draws the sun on its path over the horizon. The sun rises on the left, stands highest at midday, and sets on the right, and after dark it drops under the horizon as an open dot.

The game time takes a tint from your theme for the part of the day. Each moon in the sky shows as a small icon of its phase in its own color. Each moon takes the color the game gives its name from your theme, so Lysenties draws in the theme's bright white, Nercuros in its bright cyan, and Dyphrities in its red. On a light theme the icons are ink on paper, like a printed calendar, with the dark part filled in, so a new moon is a solid disc and a full moon an open ring. Hover a moon to read its name and phase, like `Nercuros, nearly full and still growing`. During an eclipse, the triad, or a near alignment, one word in the warn color follows the moons. A dormant moon stays hidden, and the moons leave the line while you are not connected.

### 5.2 Track a target with quick keys

A set target shows in the status bar, the room strip, and the combat pane. Quick keys are name and verb pairs listed beside it in the status bar.

- Read the status bar. Once a target is set, the left block shows `tar` plus the target name, then your quick keys as name and verb pairs separated by `·`.
- Find the target in the room strip. Its character chip carries a `▶` marker.
- Place the `combat` pane for a dedicated readout. It ships hidden, which renders it inline inside the vitals bar. Move it to a zone in the `panels` tab and it becomes a standalone pane with the target name over an hp track bar. It collapses when no target exists.
- Park `combat` and `vitals` together in the bottom zone and combat shrinks to a chip attached to the vitals block.
- Use a quick key by typing its name as the first word of a command. Vosh suppresses its own echo, because the backend echoes the expansion instead.

Quick keys live in the running session. They reset to the stock `gg`, `xx`, `zz`, and `tt` slots on restart, so set your verbs again with `#qkey` after each launch.

## Make it yours

### 6.1 Switch themes

Themes recolor the whole app, chrome and terminal alike. They live in the settings window under `themes`.

- Click the gear button in the main window top bar to open the settings window.
- Pick the `themes` tab in the left rail. It sits under the `appearance` group beside `typography`.
- Browse the catalog. Cards for the stock themes come first, then any custom themes tagged `custom`, then the dashed `+ new from active` card. Each card shows a state dot, the theme name, and three swatches for the surface, accent, and warn colors.
- Click a card. The theme activates on the spot, and the header note reads `changes save automatically`.

A theme changes both layers of the app. The chrome layer covers surfaces, text, borders, the accent pair, and the semantic warn, danger, info, and success colors. The terminal layer covers background, foreground, cursor, selection, and all sixteen ANSI colors. Some themes also turn on terminal tint by default. Ember ships with its pastel ANSI on.

Stock themes stay locked. A custom theme shows an `×` on its card while it is inactive, and clicking it deletes the theme immediately with no confirm dialog. If the active theme ever disappears, Vosh falls back to the default theme.

### 6.2 Create a custom theme

A custom theme forks an existing theme and recolors it slot by slot. The editor lives in the settings window under `themes`.

- Open the settings window and pick the `themes` tab.
- Activate the theme you want as the starting point.
- Click the dashed `+ new from active` card at the end of the catalog. Vosh forks the active theme, names the copy after the base theme with `(custom)` appended, and switches to it.
- Set the `label` and `description` fields in the editor that appears under the catalog.
- Recolor the `chrome` section. Its groups are surfaces, text, borders, accent, and semantic.
- Recolor the `terminal` section. Its groups are terminal surfaces, selection, `ANSI 0-7`, and `ANSI 8-15 (bright)`.
- Adjust any slot with the color picker beside it, or type a value into its text field.

Every edit applies live in the main window, and Vosh saves automatically a moment after each change. The text field commits a value only when CSS can render it. It accepts rgba values, and the picker strips them to hex for its own display.

The editor renders only while the active theme is a custom one. Switching to a stock theme hides it until you activate the custom theme again.

### 6.3 Control terminal colors

The terminal palette, and the two accent colors Vosh draws over it, live in the settings window under `themes`.

- Open the settings window and pick the `themes` tab.
- Find the `tint` section and its `terminal tint` row.
- Check `tint output with theme` to recolor server output with the theme's own ANSI set, or uncheck it to show the base palette. Each theme picks its own default, and Ember ships its pastel ANSI on.
- Edit the `terminal base palette` section to change the base palette itself. Its sixteen slots are the ANSI 0 to 15 colors used while `tint output with theme` is off. Change any slot with its picker or text field. Touching one slot turns all sixteen into a custom list.
- Click `reset to canonical` to discard the custom list and return to the canonical chart. The button shows only while the palette is custom.
- Set `sent command color` to recolor the local echo of every command you send. The row holds a color picker, a text field, and a `clear` button.
- Set `split scrollback divider` the same way to recolor the line between the history and live panes. Its text field placeholder reads `theme default (#rrggbb, rgba, named)`.

Both accent rows apply live the moment you commit a value, and `clear` returns either one to the theme default.

### 6.4 Set the terminal font

The terminal typeface and size live in the settings window under `typography`.

- Open the settings window and pick the `typography` tab.
- In the `terminal face` section, click one of the quick chips. The chips are `BerkeleyMono`, `JetBrainsMono`, `Menlo`, `Monaco`, and `Courier New`. The first two ship inside Vosh, so they work on every machine.
- Or type a full CSS stack into the family field, following the placeholder shape `"BerkeleyMono Bundled", Menlo, monospace`.
- Set the size with the number input. It accepts 9 to 32 px and defaults to 14.
- Check `bright text as bold` to render SGR bright colors 8 to 15 in the heavier cut. This applies on the native renderer only.
- To use a font installed on your machine, click into the `system fonts` filter field. Vosh loads the installed list on first focus, and the placeholder tells you when it is ready. Keep `monospace only` checked to hide proportional faces. The list shows up to 200 families, each row drawn in its own face.
- Click a family in the list. Vosh sets your stack to that family with Menlo and monospace as fallbacks.
- Check the `preview` section. It renders a fixed sample line at your chosen family and size.

Changes save automatically about a quarter second after you stop typing, and a `saved.` note confirms it.

## Characters and data

### 7.1 Manage profiles

A profile carries its own aliases, triggers, macros, and variables, and auto match rules pick the right profile when you connect. Manage profiles in Settings under `profiles`.

- Click the gear button in the top bar to open Settings, then pick `profiles` under `characters` in the left rail.
- Type a name in the create row and click `+ new`. Names take letters, digits, `-`, `_`, and spaces.
- Click `auto-match` on a row to attach matching rules. Enter the `host`, an optional `port`, and `characters` as a list separated by commas. Any listed name matches. Click `save`.
- Click `switch` on a row to change the active profile. Everything on this tab saves automatically.

The `scope` section decides what travels with a profile. Flip the pill from `global` to `profile` on any of `theme`, `font`, `dock layout`, `keep last command`, or `auto check updates`. Global keeps one value across every profile. Profile moves the value with the active profile, and the `font` row covers family and size as one toggle.

`duplicate` copies a profile's whole setup but deliberately leaves the auto match rules behind. You cannot delete the active profile, so switch away first.

Example. A profile named `aabahran-erelei` with a `characters` list of `Erelei, Akletus, Vanek` matches a login on any of those three names.

From the input bar, `#profile save` and `#profile load` write and reload the active profile's file on demand.

Each profile reads your prompt on its own. Vosh moves the capture trigger that `#prompt` made into each profile that draws your own prompt, turns the trigger off, and tells you once at launch. Profiles that draw nothing then show the game's prompt. On The Forsaken Lands the moved pattern switches to your prompt codes the first time the game shows them, when you log in or when you type `prompt`. From then on Vosh follows each prompt you set in the game and keeps your design and the draw switch as they are. When a color code runs into a code in that prompt, or when the pattern fills a value under a name no prompt code fills, such as `health`, the pattern stays and `#prompt` says why. A pattern you set with `#prompt {regex}` never switches. An older version of Vosh shows the game's prompt in every profile until you turn `prompt-capture` on again under Automation. Back in this version, Vosh moves the capture into your profiles again and turns the trigger off.

### 7.2 Set up loadouts

Loadouts flip whole groups of aliases, triggers, and macros on and off from one shared catalog. Loadout mode starts with a one time migration from per profile files.

- Open Settings, pick `import` under `tools`, and find the `migrate between scopes` section. Click `preview migration`.
- Review the plan. The wizard shows how your profiles would merge into a single shared catalog with one generated loadout per source profile. The preview writes nothing.
- Apply the migration. Vosh copies each profile file to `profiles/legacy/`, writes the catalog and the loadouts, and takes the aliases, triggers, and macros out of each profile file. Every other setting stays with its profile except the presets. Loadout mode keeps one list of presets that are on, and every character shares it. The list starts with every preset that any profile file had on, and the preview names each character that gains or loses a preset. Loadout mode waits for the next launch, so click `quit Vosh` in the wizard and reopen the app. Every loadout starts off, so each profile keeps on the items it had on, at launch and when you switch.
- Reopen Settings and pick the `loadouts` tab, which now appears under `characters`. Check the boxes for the loadouts you want live. The runtime enables the union of their groups across every active loadout.

The catalog keeps your folder names where it can. Each alias, trigger, and macro lands in a group that is on for exactly the profiles that had it on, so a folder two characters filled differently can become more than one group. `combat` holds what most characters kept in their combat folder, `combat (Healer)` holds the combat items only the Healer had, and `(Healer)` holds the items the Healer had outside any folder. Each profile file remembers which groups its folders became, so `#group combat on` and `#group combat off` still turn on and off exactly what that profile had in its combat folder.

A trigger two characters had in different versions keeps each version, and the second one takes a name that adds its characters, such as `greet (Healer)`. An alias or a macro keeps the one version you pick in the wizard, since its name is what you type or press. Triggers keep the order each character had them in, since every trigger that matches a line fires in that order. Where two characters had the same triggers in different orders, one of them gets its own copy of a trigger, named the same way.

Click `deactivate all` to park the catalog dormant. Dormant disables every grouped alias, trigger, and macro, and it survives restarts and profile switches. Items without a group always stay live.

When no active loadout declares any enabled groups, the loadouts impose nothing and your durable checkbox state from the automation tabs stands.

Activation is the only edit the `loadouts` tab makes. Author or reshape loadouts by editing `loadouts.toml` in the app data folder while Vosh is closed. The migration wizard runs once. It will not build a new catalog while `catalog.toml` or `loadouts.toml` sits in the app data folder, while `profiles/legacy/` holds the copies from an earlier run, or while an earlier run waits to finish at the next launch.

After the move, `catalog.toml` holds your aliases, triggers, and macros, with every one you add or change later. Each file in `profiles/legacy/` is a backup of its profile as it was before the move. Never copy a backup back while `catalog.toml` sits in the app data folder. Vosh would lay the old aliases, triggers, and macros of the backup over the catalog, turn on for that character items that only other characters had, and at the next save put the old versions in the catalog for every character. To keep your items, leave `catalog.toml` where it is and change them in Automation settings. To build a new catalog from the backups, quit Vosh first, since Vosh saves `catalog.toml` again as it quits. Then move `catalog.toml` and `loadouts.toml` out of the app data folder, copy the files in `profiles/legacy/` back over the ones in `profiles/`, and move `profiles/legacy/` out too. Each profile comes back as it was before the move, every setting included, and loses every change you made to its settings since. Your items as they are now stay in the `catalog.toml` you moved out. Then open Vosh again and run the wizard.

### 7.3 Import a TinTin++ file

The `#import-tintin` command reads aliases and variables out of a TinTin++ `.tin` file and loads them into the live profile. It runs from the input bar.

- Type `#import-tintin <path>` and point it at the `.tin` file. `~` expands in the path.
- Read the echo. It prints `imported <path>` and a count line like `12 aliases, 4 vars`.
- Check the `skipped (unsupported)` line. It tallies directives Vosh does not model by name, so you can port them by hand.
- Check the `unparsed` count. It flags alias or variable lines the parser could not read.

The importer handles `#alias {name} {expansion}` and `#variable {name} {value}`, with `#var` accepted as a short form. Nested braces and escaped braces inside the values parse correctly. The importer silently skips `#nop` lines and comments starting with `;`. Imported aliases overwrite existing aliases with the same name. Variables land at profile scope, so they persist with the profile.

Example. `#import-tintin ~/aabahran.tin` imports the file from your home folder, and a skip line of `event=2 ticker=1` reports two `event` directives and one `ticker` directive left behind.

Files from other clients go through Settings instead. Open the `import` tab, pick or paste a MUSHclient, Mudlet, GMUD, or `CMUD / zMUD` export, leave the format on `auto-detect`, and hit `apply`. The summary lists counts plus anything rejected, unsupported, or unparsed.

### 7.4 Search session logs

Vosh logs every session automatically and searches the store with regular expressions. The search lives in Settings under `logs`.

- Open Settings and pick `logs` under `tools` in the left rail.
- Pick a scope in the sessions list on the left. `all sessions` searches everything, and clicking one session narrows the search to it. Each row shows the host, port, date, and line count.
- Type a pattern in the search box and click `search`. Patterns are regular expressions.
- Tick `case` for case sensitive matching. Tick `show all` to lift the 500 result cap and return every match.

Each hit shows its timestamp, the host and port when you search across sessions, and the line in its original colors when the raw bytes are on record. Changing the session scope or either checkbox reruns the current search on its own. A new pattern needs another click on `search`.

Example. The pattern `dragon|wyvern` finds lines containing either word.

The `copy` button on a session row copies that whole session to your clipboard as plain text. There is no file download yet. The store is `logs.sqlite` in the app data folder and it fills on every connection, so logging needs no setup.

The log keeps what the game sent and each line you sent, marked `> `. Lines you type at a password prompt are not saved. Each one shows as `> (hidden)` in its place. Older versions of Vosh saved those lines in full, so a session you logged before updating can still show your password after a `> `. The game also shows two kinds of password as you type them, the one you set for a new character and any you give a command like `password <old> <new>`, and the log saves those in full in every version.

Type `#logs forget-passwords` to count the lines that hold a password. Vosh says how many it found and in how many sessions, and it never shows the lines themselves. Type `#logs forget-passwords now` to blank them. Each one then reads `> (hidden)`, and Vosh rewrites `logs.sqlite` so the old text is gone from the disk too. On a large log this takes a few seconds, and new game text waits until it finishes. The rewrite needs free disk space about the size of `logs.sqlite`. When Vosh cannot finish it, the lines stay blanked, Vosh says so, and the next `#logs forget-passwords now` finishes the rewrite. A backup of your disk, like Time Machine, keeps its own copy of the old file. If you copied or shared one of those sessions, change your password in the game.

### 7.5 Check for updates

Vosh checks for new builds and installs them in place. The controls live in Settings on the `general` tab.

- Open Settings. The `general` tab opens by default. Scroll to the `app` section and find the `updates` row.
- Click `check now`. The status reads `checking…`, then `up to date` when nothing is newer.
- When a build is available, click the `install v<version> + restart` button that appears. The status shows `installing…`, then Vosh relaunches on the new build.
- Tick `check on launch` to run the check at every start. It is off by default. With it on, a banner appears in the main window when an update is waiting.

Updates download from the project's GitHub releases, and Vosh checks every build's signature before installing.

`auto check updates` is one of the five scope rows on the `profiles` tab. It defaults to global, so one setting covers every profile. Flip it to profile when one character should check on launch while the others stay quiet.

## Fix it

### 8.1 Switch terminal renderers

Vosh ships two terminal renderers. The native GPU surface is the default on macOS and the xterm renderer is the default on Windows and Linux. The `#nativesurface` command switches between them from the input bar.

- Type `#nativesurface off` to force the xterm renderer everywhere.
- Type `#nativesurface on` to force the native surface everywhere.
- Type `#nativesurface default` to return to the platform default.
- Restart Vosh. The switch applies only on restart, and the echo reminds you with `restart Vosh to apply`.

The command runs entirely in the frontend and stores your choice locally under the key `vosh.nativesurface`. A bad argument echoes `usage #nativesurface on | off | default (takes effect on restart)`.

If the text renders in the wrong typeface, open Settings and pick the `typography` tab. The default family stack starts with `BerkeleyMono Nerd Font` and falls through `JetBrains Mono`, `Fira Code`, `Menlo`, and `Consolas` before generic monospace. Your machine renders the first family in that stack it has installed, so install the font you want or move it to the front. Font size defaults to 14.

Fonts follow profile scope. In Settings under `profiles`, the `font` row covers family and size as one toggle. Set it to `global` for one look everywhere or `profile` to let each profile carry its own.

On the xterm renderer the right click menu offers `clear buffer`. The native surface hides that item because its grid has no clear command.

### 8.2 Recover a bad connection

The session chip in the top bar holds the connection controls. Its status dot shows idle, connecting, connected, or error, and while live the chip also shows your character name and `host:port`.

- Click the chip. A dropdown form opens with `host`, `port`, a `tls` checkbox, and a submit button that reads `connect` or `disconnect` depending on state.
- Press `disconnect` and wait for the dot to go idle.
- Confirm the address. The defaults are host `play.theforsakenlands.com` and port `1848` with `tls` off.
- Press `connect`.

The `tls` checkbox wraps the connection in TLS. Match it to what the server offers on that port. The default port `1848` expects it off.

Disconnecting has side effects. Session scoped variables clear when the next connection opens, so anything set with `#var` never carries into the new session, while profile variables survive. The chat pane buffer clears at disconnect. On reconnect, Vosh matches the host and port against your profiles and switches to the best match automatically, and it picks up a character pinned profile after login.

Two other paths reach the same controls. Right click the terminal and pick `disconnect`, the item that appears at the bottom of the menu only while connected. Or press `Cmd+K` on macOS or `Ctrl+K` elsewhere and run the palette entry `connect` or `disconnect`.

### 8.3 Find your data on disk

Vosh keeps all of its data in one app data folder named `com.aabahran.vosh`.

- On macOS, open `~/Library/Application Support/com.aabahran.vosh`.
- On Linux, open `~/.local/share/com.aabahran.vosh`.
- On Windows, open `%APPDATA%\com.aabahran.vosh`.

Inside that folder.

- `profiles.toml` indexes your profiles and names the active one.
- `profiles/<name>.toml` holds each profile snapshot with connection defaults, aliases, variables, triggers, timers, tick config, and macros. In loadout mode the aliases, triggers, and macros live in `catalog.toml` instead, and the profile file keeps the rest.
- `profiles/legacy/` holds a copy of each profile file as it was when the loadouts migration ran.
- `global.toml` holds cross profile UI preferences.
- `catalog.toml` and `loadouts.toml` appear once loadout mode is active.
- `logs.sqlite` stores session logs, with `-wal` and `-shm` sidecars alongside.
- `scrollback.txt` persists the last 10,000 terminal lines across restarts.
- `maps.sqlite` stores map data.
- `scripts/` holds Lua files for `#script load`.
- `plugins/` holds plugin folders, each with a `manifest.toml`.

Every TOML save is safe by design. Vosh writes the new text to a temp file, copies the old file to `<file>.bak.<timestamp>` with a millisecond timestamp, swaps the temp file in atomically, and keeps the ten newest backups. A save that fails leaves the old file in place. To roll back a bad profile edit, copy the backup you want over the live file.

A leftover `profile.toml` at the root is the legacy single profile file. Vosh migrates it to `profiles/default.toml` on the first multi profile launch.

## Reference

### 9.1 Slash commands

This is every slash command Vosh understands today.

- `#help` prints the command summary.
- `#alias <name> <expansion>` defines, `#unalias <name>` removes, `#aliases` lists.
- `#var <name> [value]` sets or shows a session variable, `#unvar <name>` removes it from both scopes, `#vars` lists.
- `#trigger <name> {pattern} <action> [args]` defines, `#untrigger <name>` removes, `#triggers` lists by priority.
- `#prompt game {setting}` and `#prompt fight {setting}` read your prompt in this profile from the codes of your PROMPT and fight prompt, `#prompt {regex}` reads it with a pattern, `#prompt` says how Vosh reads it, and `#unprompt` stops reading it.
- `#prompt show text|lifted|pinned` shows your prompt in this profile in the text, lifted on a band in the text, or pinned above the command line.
- `#prompt default` puts Vosh's default design in place of the design in this profile and keeps yours as an earlier design.
- `#group <name> on|off` toggles a group, `#group <name>` shows state, `#groups` lists.
- `#tick`, `#tick interval <secs>`, `#tick reset`, `#tick on {pattern}`, `#tick off`, `#tick fire <command>`, `#tick nofire`, `#tick sound on|off`, `#tick disable`, `#tick enable` drive the tick timer.
- `#tick warn`, `#tick warn at <secs>`, `#tick warn message <text>`, `#tick warn color <name>`, `#tick warn off` shape the tick warning.
- `#script load <name>` loads a Lua file, `#script reload` reruns loaded scripts, `#scripts` lists them.
- `#lua <code>` evaluates Lua inline.
- `#profile save`, `#profile load`, `#profile reset` manage the profile snapshot. In loadout mode all three become notices.
- `#import-tintin <path>` imports TinTin++ aliases and variables.
- `#logs forget-passwords` counts the lines in your session log where you sent a password, and `#logs forget-passwords now` blanks them.
- `#record <name>` starts recording, `#record` shows status, `#record cancel` discards, `#endrec` saves the recording as an alias.
- `#qkey <name> <verb>` configures a quick key, `#qkey clear <name>` clears, `#qkeys` lists.
- `#target <args>` mirrors `tar`, with `#target clear|next|prev`, `#tarn`, `#tarp`, `#tarclear` as slash forms.
- `#nativesurface on|off|default` forces the renderer, applied on restart.

Targeting also works bare with no `#`. Type `tar` to list, `tar <N>` or `tar <substr>` to pick, `tarn` and `tarp` to cycle, `tarclear` to clear.

An unknown command points you at `#help`. Errors echo wrapped in square brackets.

### 9.2 Keyboard shortcuts

This is every built in key Vosh binds, grouped by where it works.

In the command input.

- `Enter` submits. `Shift+Enter` inserts a newline for multi line compose, and in password mode it submits instead.
- `Tab` and `Shift+Tab` cycle tab completion through your history words, room characters, and recently seen names.
- `ArrowUp` and `ArrowDown` recall history, filtered by whatever prefix you already typed.
- `PageUp` and `PageDown` page the scrollback. On macOS press `Fn+Up` and `Fn+Down`.
- `Escape` cancels an in flight paste burst, closes the scrollback split, and snaps the terminal to its tail.
- `Home` and `End` jump the caret, also reachable as `Cmd+Left` and `Cmd+Right` or `Fn+Left` and `Fn+Right` on macOS. Add `Shift` to extend the selection.
- `Cmd+C` or `Ctrl+C` with nothing selected in the input copies the native surface selection.

Anywhere in the window.

- `Cmd+F` or `Ctrl+F` opens the find toolbar.
- `Cmd+K` or `Ctrl+K` toggles the command palette.

In the find toolbar. `Enter` finds the next match, `Shift+Enter` the previous, `Escape` closes and clears.

In the command palette. `ArrowUp` and `ArrowDown` move the selection, `Enter` runs the entry, `Tab` and `Shift+Tab` cycle the scope filter, `Escape` closes.

Mouse on the terminal. Wheel up opens the scrollback split on the xterm renderer, the native surface scrolls its own grid. Middle click closes the split and snaps to bottom. Right click opens the terminal menu.

Bind your own keys as macros in Settings under `macros`. Canonical names look like `F1`, `Ctrl+N`, `Shift+F5`, and `Ctrl+Alt+Numpad7`.

### 9.3 Prompt design codes

Your own prompt is a design of text and codes. Customize prompt writes the codes for you as you click the parts of your prompt and pick values. Choose `Edit as text` there to read them or type your own.

| Code                                          | What it does                                                      |
| --------------------------------------------- | ----------------------------------------------------------------- |
| `%hp` `%mana` `%move`                         | Your current Health, Mana or Moves.                               |
| `%maxhp` `%maxmana` `%maxmove`                | The most you can have.                                            |
| `%pct_hp`                                     | Health as a percent with no sign. Add %% for the sign.            |
| `%hp_bar:10:auto`                             | A bar ten cells wide, colored by how full it is.                  |
| `%{gold:grouped}`                             | Any value from the picker, in any of its forms.                   |
| `%c_green` `%c_hp`                            | A theme color, or Health's color by how full it is.               |
| `%c_default`                                  | Back to the terminal text color. Bold and italic stay on.         |
| `%c_reset`                                    | Back to plain text with every color and style off.                |
| `%s_italic` `%s_bold` `%s_underline` `%s_off` | Turns a style on, or every style off.                             |
| `%nl`                                         | Starts a new line.                                                |
| `%{if:fight}` `%{ifnot:fight}` `%{end}`       | Shows what sits between them only in a fight, or only out of one. |
| `%{raw}`                                      | Your prompt exactly as the game sent it.                          |
| `%%`                                          | A percent sign.                                                   |

Every value in `Insert value…` has codes of its own, and the picker shows them beside each form.
