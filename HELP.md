# Vosh help

This file is the same content the in-client Help view shows. The Help view (top bar help button) is the primary place to read it. This file mirrors the same topics for offline reading and copy-out.

The source of truth for both is `src/lib/helpContent.ts`.

---

## Get connected

### 1.1 Connect to a world

You finish this walkthrough logged in, with a live session chip in the top bar.

- Click the session chip in the top bar. While idle it reads `connect` beside a status dot.
- Fill in the host and port. The form defaults to `play.theforsakenlands.com` and `1848`. Tick the `tls` checkbox when your server offers TLS.
- Click `connect` and watch the dot shift from connecting to connected.
- Type your character name at the login prompt and press `Enter`.
- When the server asks for your password, the input row swaps to a masked field with the placeholder `password`. Nothing you type shows on screen, echoes to the terminal, or lands in command history. Press `Enter` to submit. `Shift+Enter` submits here too instead of adding a line.

Once you are in, the chip shows your character name in lowercase along with the host and port. If a saved profile matches the host and port you dialed, Vosh switches to that profile before connecting. A profile pinned to a character attaches right after login, when the server reports who you are.

To leave, click the chip and press `disconnect`, or right click the terminal and choose `disconnect`.

### 1.2 Reconnect and recover

You come back from a dropped connection with your scrollback intact and your next commands already staged.

- Read the session chip first. The status dot turns to its error state when the connection fails and back to idle when the session closes cleanly.
- Click the chip and press `connect` to dial the same host and port again.
- Review what happened before the drop. The terminal scrollback survives a disconnect, so scroll up or press `PageUp` and read back through it. Nothing clears it unless you pick `clear buffer` yourself.
- Stage your recovery while offline. Type the first command, press `Shift+Enter` to stack more lines under it, and leave the block sitting in the compose box. After you reconnect, press `Enter` once and each line submits separately, in order.

Two things do reset. The chat pane buffer empties the moment the session drops, and session variables set with `#var` clear when the next connection opens, so they never carry into a new session. Aliases, triggers, macros, and profile variables stay loaded because they live in your profile, not in the connection.

When you want out on purpose, `disconnect` lives in three places. The session chip form while connected, the terminal right click menu, and the `Cmd+K` palette.

### 1.3 Make Vosh remember you

You set Vosh up once and every launch after that starts with your aliases, triggers, and settings already in place.

- Shape the client the way you want it. Define aliases, triggers, macros, variables, and tick settings.
- Type `#profile save`. Vosh writes the snapshot to your active profile TOML under `~/Library/Application Support/com.aabahran.vosh`.
- Or press `Cmd+K` and run the `#profile save` palette entry. It sends the same command.
- Restart Vosh. The profile loads on startup with no extra step.

What persists. Connection defaults, aliases, profile variables, triggers, tick configuration, macros, ui settings, enabled plugins, and the groups you disabled.

What does not. Variables set with `#var` live in session scope. They clear when the next connection opens and never reach the file. A lasting value belongs in the `profile_vars` table of your profile file at `profiles/<name>.toml` in the app data folder. Edit it there while Vosh is closed, or set the value from Lua with `mud.set_profile_var`.

`#profile load` pulls the saved file back into the live session. In loadout mode the profile commands become notices instead, because loadout mode saves your changes automatically.

## Play

### 2.1 Send commands

You move from single commands to chained lines, multi line blocks, and safe bulk pastes.

- Type a command and press `Enter` to send it.
- Chain commands on one line with `;`. Each piece goes out as its own command. Type `\;` when you mean a literal semicolon.
- Press `Shift+Enter` to add a line without sending. The box grows and a line number gutter appears once you have two or more lines. Press `Enter` and every line submits separately, in order, with blank lines dropped.
- Press `Enter` on an empty box to send a bare line. Many MUD prompts advance on that.
- Paste multi line text straight into the input. A single line submits immediately. Two or more lines become a paste burst, sent one line every 500 ms by default, with a `paste N/M esc cancels` counter beside the prompt.
- Press `Esc` during a burst to cancel every line that has not gone out yet. Starting a new paste also cancels the old burst.

Turn on the `keep last command` setting and a sent command stays in the box fully selected. Press `Enter` again to resend it, or just start typing to replace it.

The burst delay is configurable from 0 to 10000 ms, so pace it to whatever your server tolerates.

### 2.2 Reuse what you typed

You recall anything you have sent this session without retyping it.

- Press `Up` in an empty input to step back through your sent commands, newest first.
- Type a few characters before pressing `Up` and recall becomes a prefix search. Type `tell` then `Up` and only lines starting with `tell` cycle past.
- Press `Down` to step toward newer matches. One step past the newest restores exactly what you had typed before the search began.
- Edit the recalled line whenever you like. Editing ends the search, and the next `Up` starts a fresh one from whatever is now in the box.

History skips consecutive duplicates and never records anything you type in password mode.

Turn on the `keep last command` setting and the line you just sent stays in the box, fully selected. `Enter` resends it and typing anything replaces it, which suits commands you repeat in quick succession.

In a multi line compose the arrows first do their normal job. `Up` moves the caret up a line unless you are already on the first line, and `Down` moves it down unless you are on the last, so history recall fires only from the edges of the block.

### 2.3 Complete names with Tab

You finish long names with a keypress instead of spelling them out in the middle of a fight.

- Type the first letters of the name anywhere in your command line.
- Press `Tab`. Vosh completes the word under the caret with its best match.
- Press `Tab` again to cycle through the remaining candidates, or `Shift+Tab` to cycle backward. The list wraps around.
- Keep typing, or press any other key, and the cycle resets with the current completion left in place.

Candidates come from three sources, checked in this order.

- Words from commands you have typed, most recent first.
- Characters in your room, when the server sends `Room.Chars` over GMCP.
- Capitalized names Vosh spotted in the output during the last 30 minutes.

Matching is a case insensitive prefix match, duplicates collapse, and Vosh skips a candidate identical to what you already typed. Completion works on the word under the caret, so you can fix the middle of a line without touching the rest.

### 2.4 Scroll back through history

You read old output in a split pane while live output keeps flowing underneath.

- Scroll the mouse wheel up over the terminal. The first notch opens a scrollback split above the live pane, and further scrolling walks the history line by line.
- Or press `PageUp` to open the split and page upward, then `PageDown` to page back down. A Mac keyboard produces these with `Fn+Up` and `Fn+Down`.
- Track your depth with the `↑ N / max` badge at the top right of the history pane.
- Drag the divider between the panes to resize the split. It snaps to whole terminal rows. The handle also answers the keyboard, arrow keys nudge it 16px and `Shift` with an arrow jumps 64px.
- Return to live three ways. Scroll or page down until history reaches its bottom and the split closes itself. Press `Esc`. Or middle click the terminal.

The live pane never scrolls away while the split is open. New output keeps landing at its tail, and the lines you type mirror into the history pane so the story stays continuous.

On the native macOS renderer there is no split. The surface scrolls its own grid, and `PageUp`, `PageDown`, and `Esc` still page it and snap it back to the bottom.

### 2.5 Find text

You jump straight to any text in the session, however far back it scrolled.

- Press `Cmd+F` on macOS or `Ctrl+F` elsewhere. The find toolbar drops over the top right of the terminal, and it opens even while you are typing in the command input.
- Type your query into the `find in scrollback` box.
- Press `Enter` for the next match and `Shift+Enter` for the previous one. The `↑` and `↓` buttons do the same jobs.
- Read the badge beside the box. It shows `N / M` while you step through hits and `no match` when the query finds nothing.
- Sharpen the query with the three toggles. `case` makes it case sensitive, `word` matches whole words only, and `regex` treats the query as a regular expression.
- Press `Esc` to close the toolbar, clear every highlight, and put focus back on the command input.

A match above the visible screen opens the scrollback split with the hit highlighted near the top of the history pane. A match already on screen closes any open split instead.

The toolbar also opens from the terminal right click menu item `search scrollback` and from the `Cmd+K` palette.

### 2.6 Copy text out

You land terminal text on the clipboard, ready for notes, bug reports, or a friend.

- Drag across the output you want. An active text selection stops the usual click from refocusing the command input, so your selection stays put.
- Press `Cmd+C` on macOS or `Ctrl+C` elsewhere.
- Or right click the terminal and choose `copy`. The menu shows the `⌘C` shortcut beside it.
- Paste wherever you need it.

One priority rule. When the input box itself holds a selection, `Cmd+C` copies that selection rather than the terminal. Clear the input selection, or use the right click `copy` item, when the terminal text is what you want.

The trip works in reverse too. The right click menu's `paste` item inserts the clipboard into the input row without sending anything. Edit the line as long as you like, then press `Enter` yourself.

On the xterm renderer you can also wipe the buffer once you have copied what matters. Right click and choose `clear buffer`.

### 2.7 Drive everything from the palette

You run nearly every Vosh action from one keyboard surface.

- Press `Cmd+K` or `Ctrl+K` to open the palette. The same shortcut closes it again.
- Type a few letters. Entries whose title starts with your text rank first, then title substrings, then hint and keyword matches.
- Move the selection with the arrow keys and press `Enter` to run the highlighted entry.
- Press `Tab` or `Shift+Tab` to cycle the scope filter when you want a single group.
- Press `Esc` to close without running anything.

Four groups live inside.

- Commands. `connect` or `disconnect` depending on state, `open splits` and `close splits` for the well panes, `search scrollback`, `#profile save`, and `open help`.
- Panes. A show or hide toggle for each panel (`map`, `group`, `vitals`, `roomstrip`, `chat`, `affects`, `combat`, `imm`).
- Settings. One entry per Settings tab. Each opens the Settings window already on that tab.
- Aliases. Every enabled alias. A parameterless alias runs the moment you pick it. An alias that takes arguments inserts its name into the input row instead, so you finish the line and press `Enter`.

### 2.8 Use the right click menu

You reach the terminal's everyday actions from a single menu under your pointer.

- Right click anywhere on the terminal to open it.
- Choose `copy` to copy the current selection. The menu lists its `⌘C` shortcut.
- Choose `paste` to insert the clipboard into the input row. Nothing sends until you press `Enter` yourself.
- Choose `open splits` or `close splits` to toggle the session, chat, and log panes inside the terminal well. These are workspace panes, separate from the scrollback split.
- Choose `search scrollback` to open the find toolbar.
- Choose `clear buffer` to wipe the terminal. The item appears only on the xterm renderer. The native macOS grid has no clear command, so the item hides there rather than sit dead.
- While connected, `disconnect` waits at the bottom in danger styling and closes the session.

The menu closes on `Esc`, on a click anywhere outside it, or the instant you pick an item. It clamps itself to the window edges, so a right click near a corner never opens it half off screen.

## Automate

### 3.1 Make your first alias

When you finish, typing `kk dragon` will kick and backstab the dragon with one short command.

- Click the gear button in the top bar to open settings, then pick the `aliases` tab.
- Click `+ alias` to add a blank row.
- Type `kk` in the name field.
- Type `kick %1; backstab %1` in the expansion field. `;` splits the expansion into separate commands.
- Click `save`. The unsaved dot clears and `saved.` appears.
- Back in the main window, type `kk dragon`. Vosh sends `kick dragon` then `backstab dragon`.

Captures give aliases their reach. `%1` through `%9` pull the first through ninth word after the alias name. `%0` pulls the whole tail, `%1-` pulls word one through the end with spacing intact, and a missing word expands to nothing. `%%` gives a literal percent and `\;` keeps a literal semicolon.

You can also define aliases without opening settings. `#alias gc get all corpse` sets one and echoes `alias gc set`. `#aliases` lists every alias and `#unalias gc` removes one.

Give related aliases a shared group name to toggle them as a folder, either with the group checkbox in the tab or with `#group <name> on|off`. The `tpl` button on each row switches the expansion to a Lua script body when a template stops being enough.

### 3.2 Make your first trigger

When you finish, Vosh will loot every kill the moment the death line lands.

- Open settings, pick the `triggers` tab, and keep the editor pill on `form`.
- Click `+ trigger` to add a card.
- Name it `auto-loot`. Leave priority at `5`, the default for a new card, and target on `line`.
- Type `(\w+) is DEAD!` in the pattern field. Patterns are regexes, so escape literal punctuation.
- Click `+ send` and type `get all corpse` in the editor that appears.
- Click `save`, then go kill something.

A trigger pairs one visual with any number of effects. The visual chips are `none`, `highlight`, `replace`, and `gag`, and the effect buttons are `+ send`, `+ route`, and `+ script`. Send and replace templates reach capture groups with `$1` through `$9` or `${name}`, and `;` splits a send into separate commands. A card holds several patterns through `+ pattern` and fires when any enabled row matches. Higher priority triggers run first.

The input bar works too. `#trigger auto-loot {(\w+) is DEAD!} send get all corpse` builds the same trigger with priority 0 on the `line` target. `#triggers` lists everything by priority and `#untrigger auto-loot` removes it. Vosh rejects an invalid regex and names the broken pattern, so nothing half works silently.

### 3.3 Highlight lines that matter

When you finish, every tell will glow instead of scrolling past unnoticed.

- Type `#trigger tell-glow {tells you} highlight bright_yellow bold`. Every line containing a tell now renders bright yellow and bold.
- Upgrade it to a wash. Type `#trigger tell-glow {tells you} highlight bright_yellow wash`. Defining a trigger under an existing name replaces it.
- Type `#triggers` to confirm the pattern and action.

A plain highlight restyles the text. A wash tints the whole line with a dim quarter strength version of the highlight color, fills it edge to edge, and draws an accent bar at the left edge, so important lines read as banners you cannot miss.

Colors take the sixteen ANSI names. `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, and `white`, plus a `bright_` variant of each. `purple` maps to magenta and `gray` to `bright_black`. Stack `bold`, `underline`, and `inverse` freely, and add `bg:<color>` for a background.

The settings `triggers` tab offers the same power with selects. Pick the `highlight` visual chip on a trigger card, choose fg and bg from the color lists, and check `full-line wash`. Wash pairs well with a quiet color, since the tint covers the entire line.

### 3.4 Route chat into panes

When you finish, tells and gossip will collect in the chat pane instead of drowning in combat spam.

- Open settings, pick the `triggers` tab, and click `+ trigger`.
- Name the card `chat-feed`.
- Type `tells you '` in the first pattern field.
- Click `+ pattern` and type `gossips '` in the second row. The trigger fires when any enabled row matches.
- Click `+ route` and type `chat` in the pane field.
- Click `save`. Matching lines now land in the chat pane.

Route is an effect, so it stacks with anything else on the card. Pair it with a `highlight` visual to color the line, or add a `+ send` effect beside it. The pane field takes the pane's name, and `chat` is the placeholder the editor suggests.

Place the pane where you want it in the `panels` tab. Each panel row has a zone select drawn from the zones that panel supports, and the schematic preview up top moves chips with chevrons so you can order panels within a zone. The `side panels span full height` checkbox decides whether side panes run past the input row.

The inline form is `#trigger chat-feed {tells you '} route chat`. It creates a single pattern trigger, so build multi pattern feeds in the settings tab.

### 3.5 Set and use variables

When you finish, one variable will feed every command you aim at it.

- Type `#var potion yellow`. Vosh echoes `var potion set`.
- Type `quaff $potion`. The line expands before it leaves, so the server receives `quaff yellow`.
- Check a value with `#var potion`, list them all with `#vars`, and remove one with `#unvar potion`.

`$potion` works when the name ends at whitespace or punctuation. Write `${potion}s` when letters follow immediately. `$$` sends a literal dollar sign, and unknown names pass through untouched, so `$100` reaches the server as typed.

Interpolation runs on the line you type, before alias expansion, and Vosh does not interpolate alias output again. Put variables in the line you type, or resolve them in a Lua script body instead.

`#var` writes session scope, which clears when the next connection opens, so a session value never survives into a new session. Profile variables persist across restarts in your profile TOML under `profile_vars`, and a session value shadows a profile value of the same name.

Vosh also fills session variables for you. GMCP binds `hp`, `maxhp`, `char_name`, `room_name`, `target_name`, and more, and setting a target with `tar` mirrors it into `$target`, so `cast dispel $target` always aims at your current mark.

One trap. Trigger send templates use `${name}` for regex capture groups, not this store, and trigger sends skip interpolation entirely.

### 3.6 Bind keys to macros

When you finish, `F1` will fire a command the instant you press it.

- Open settings and pick the `macros` tab.
- Click the key field in the top row. It reads `press a key...` while capturing.
- Press `F1`. The field records the canonical key name.
- Type the command, for example `stand; flee`. Use `;` to chain actions.
- Type a group name if you want the binding in a toggleable folder, then click `add`.
- Focus the input bar in the main window and press `F1`.

Rows apply on add or save, so the tab has no separate save step. Existing rows carry their own `save` and `delete` buttons.

Capture accepts function keys, modifier combos like `Ctrl+N`, numpad keys like `Numpad7`, and plain printable keys. A binding fires only while the input bar has focus.

Turn on `echo macro commands` in the `general` tab's `macros` row when you want each press to show what it sent. Group headers carry an `enabled` checkbox, and `#group <name> on|off` flips macro groups from the input bar along with matching alias and trigger groups.

`#record` builds something different. It captures the commands you type and saves them as an alias you invoke by name, not by key. Use the macros tab when you want a key, `#record` when you want a word.

### 3.7 Command the client inline

When you finish, you will drive Vosh from the input bar without opening settings. Vosh handles every line that starts with `#` locally, and it never reaches the MUD.

- Type `#help` any time for the full list.
- Manage aliases with `#alias <name> <expansion>`, `#unalias <name>`, and `#aliases`.
- Manage variables with `#var <name> [value]`, `#unvar <name>`, and `#vars`.
- Manage triggers with `#trigger <name> {pattern} <action>`, `#untrigger <name>`, and `#triggers`.
- Bind prompt stats with `#prompt {regex}` using named groups like `(?<hp>\d+)`, and clear with `#unprompt`.
- Flip whole folders with `#group <name> on|off` and inspect them with `#groups`.
- Tune the tick with `#tick`, `#tick interval <secs>`, `#tick warn at <secs>`, and the rest listed under `#help`.
- Record a command sequence with `#record <name>`, finish with `#endrec`, abort with `#record cancel`.
- Configure quick keys with `#qkey <name> <verb>` and list them with `#qkeys`.
- Drive Lua with `#script load <name>`, `#script reload`, `#scripts`, and `#lua <code>`.
- Snapshot with `#profile save`, `#profile load`, and `#profile reset`.
- Import TinTin++ files with `#import-tintin <path>`.
- Work targets with `#target <args>`, or bare `tar`, `tarn`, `tarp`, and `tarclear` with no `#` at all.
- Switch renderers with `#nativesurface on|off|default`, applied on restart.

An unknown command echoes a pointer to `#help`, and errors come back wrapped in square brackets.

### 3.8 Script Vosh with Lua

When you finish, a Lua script will run inside Vosh and a plugin will load itself at every launch.

- Save a script as `combat.lua` in the `scripts` folder under the app data directory, `~/Library/Application Support/com.aabahran.vosh/scripts/` on macOS.
- Type `#script load combat`. Vosh appends `.lua` to a bare name.
- Type `#scripts` to see loaded scripts and the triggers they registered.
- Edit the file, then type `#script reload` to run every loaded script again.
- Try one liners with `#lua mud.echo("hello")`.

Scripts talk to Vosh through the global `mud` table. `mud.send(text)` goes straight to the server and `mud.input(text)` feeds back through the input pipeline. `mud.echo(text)` prints locally. `mud.alias(name, expansion)` and `mud.trigger(name, pattern, callback)` register automation, with `captures[1]` holding the full match and `captures[2]` onward the groups. `mud.on_gmcp(package, callback)` hands you server data as a table, and `mud.timer(secs, callback)` schedules work you can cancel with `mud.cancel_timer`.

Loads from `#script load` last for the session. For autoload, make a plugin. Create `plugins/<slug>/` under the app data directory with a `manifest.toml` naming the plugin and its entry script, `main.lua` by default. Enabled plugin names persist in your profile TOML under `[plugins]`, and every enabled plugin loads at launch. Enabling runs immediately, disabling takes effect next launch.

The sandbox strips file and process access. `require`, `io`, and `os.execute` are gone.

## Shape the window

### 4.1 Arrange the panels

When you finish, every panel sits in the zone you chose at the size you want.

- Click the gear in the top bar, titled `settings`, and open the `panels` tab.
- Pick a home for each panel with its `zone` select. Zones are `top`, `bottom`, `left`, `right`, and `hidden`. The eight panels are `map`, `group`, `vitals`, `roomstrip`, `chat`, `affects`, `combat`, and `imm`.
- Set `align` for a left or right panel. `top` panels stack down from the column top and `bottom` panels stack up from the floor. The map hides this select because it always fills the leftover column height.
- Reorder panels within a zone using the up and down chevrons on the layout map chips.
- Tick `side panels span full height (input lives under terminal only)` to run the side columns to the window's bottom edge.
- Back in the main window, drag the 8px channel between the terminal and a column to resize it. A small ember tick fades in on the handle when you hover. Tab to a handle and press an arrow key to nudge 16px, or `Shift` plus an arrow for 64px.

Changes save live. `reset to defaults` restores the stock layout. Panels render as raised cards with 8px of exposed ground at every seam, so moving one never leaves a scar.

Prefer the keyboard for visibility. `⌘K` (`Ctrl+K` off macOS) lists a `show <id> pane` or `hide <id> pane` entry for every panel, and a panel you show again returns to its last visible zone.

### 4.2 Use the map

When you finish, the server map draws the way you like at the zoom you like.

- The map fills the right column by default. Move it with its `zone` select in the `panels` tab. It accepts `left`, `right`, or `hidden` only, since a horizontal map at full width is unusable.
- Toggle it from the map button in the top bar. The button flips the map between hidden and its last visible spot.
- Read the header. It says `map` until the first room push arrives, then `map · <area>` with the area name from `Room.Info`.
- Click the sliders button labeled `map controls` to open the controls row. Vosh remembers whether you left it open.
- Switch draw modes with `squares`, `glyphs`, or `tileset`.
- Zoom with `−` and `+`, or hold `Ctrl` and wheel over the map. The readout shows the percent and `⤺` resets it.
- Check the status text. `radius N` means live map data. `waiting for server map` means none has arrived yet.

Tileset mode adds a bar with `load tileset` and `clear` buttons for your own tile art.

### 4.3 Tame the chat pane

When you finish, channel talk collects in its own pane where you can filter it by channel.

- Press `⌘K` and run `show chat pane`. Chat ships hidden and its home is the bottom strip.
- Let lines arrive on their own. `Comm.Channel` GMCP feeds the pane automatically. Each line renders as `[pane] text` in its channel color, with the speaker shown as `Name: `.
- Click a tab in the header to filter. `all` sits first, then one tab per channel name seen in the buffer. The count beside them reads `visible`, or `visible/total` while filtered.
- Route trigger output in. In the `triggers` tab, add a `route` effect to a trigger and type a pane name. Those lines land in the chat pane under their own tab.
- Drag the pane's handle to resize it. Click the `×` labeled `hide chat` to put it away.

The buffer holds a rolling 500 lines, survives closing and reopening the pane, and clears only on disconnect. The pane sticks to its tail. Scroll up to read back, and it sticks again once you come within 24px of the bottom.

### 4.4 Read your vitals

When you finish, the hp, mana, and moves readout shows exactly what you want to see under pressure.

- Open the `vitals` tab in Settings. Changes save automatically, and you can drag the live preview's bars to scrub the numbers.
- Pick a layout, `ember`, `stacked`, or `inline`. Ember draws a sidebar pane with a `vitals` head, the tick countdown beside it, and three fixed thin bars with mono current and max numbers.
- Outside ember, choose columns with the `bar`, `percent`, `numeric`, and `delta` pills, set `bar style` to `solid`, `ramped`, or `track`, and pick bar glyphs and width.
- Open the `advanced` disclosure to recolor hp, mana, and moves, or turn on `drain through red as bars empty`.
- Tick `pulse red vignette under 30% hp` for a warning you cannot miss when health drops low.
- Turn on `custom template (overrides layout)` to write the readout yourself with tokens like `%hp`, `%pct_hp`, `%bar_hp`, `%tick`, and `%time`. Any `Char.Vitals` or `Char.Worth` field resolves as `%fieldname`, and `%%` prints a literal percent.

Place the bar in any zone from the `panels` tab. It ships in the right column, listed as `vitals (hp bar)`. Tracked affects also live in the `panels` tab, not here. `reset vitals` restores the stock config when an experiment goes wrong.

### 4.5 Watch your group and affects

When you finish, group health and your spell durations both read at a glance.

- Find the `group` pane at the top of the right column. The header shows `group` plus a member count, or `solo`.
- Group up. Each member gets a row with a mono name, a 44px hp mini bar, and the percent. The row tone drops through three tiers, healthy at 67% and up, warning down to 34%, danger below.
- Stay solo and the pane shows your worth instead, tnl, exp, gold, bank, trains, and prac.
- Configure affects in the `panels` tab under `tracked affects`. Type the server's affect name, add an optional label, and press `add`. Matching is case insensitive, and the affects pane renders nothing until you list at least one name.
- Put `affects` in a side zone for full rows, each with a mono name, a duration mini bar filled by the fraction of a day remaining, and a countdown. In the top or bottom strip it compresses to pills, and absent tracked affects render dim with `—`.

Group data arrives from `Group.Info` and worth from `Char.Worth`. Affects come from `Char.Affects`, and duration color shifts with urgency. With no group the pane tells you so and suggests `follow <name>` to start one.

### 4.6 Read the room strip

When you finish, one line tells you where you are, who is here, and what is on the ground.

- Look at the strip along the top of the window. It runs area, room name with vnum, terrain, and the exit list in `N E S W U D` order.
- Scan the `here` chips. Character chips color NPCs and players differently, stacks collapse to `(N) name`, and your current target carries a `▶` marker.
- Scan the `items` chips. Each colors by type, so money, weapons, armor, potions, and food read apart at a glance.
- Move the strip from the `panels` tab, where it lists as `room strip (area info)`. Drop it into a left or right column and it switches to a column variant that wraps onto multiple lines instead of scrolling sideways.

GMCP feeds everything. `Room.Info` names the room, `Map.Tiles` colors the area, `Room.Chars` and `Room.Items` fill the chips, and the target marker follows the backend target. In the top strip, overflowing content fades out at the right edge. An empty slot holds its height so the layout never jumps.

### 4.7 Split the well

When you finish, the terminal well holds three panes, your session, channel chat, and a raw log tail.

- Press `⌘K` and run `open splits`, or right click the terminal and pick `open splits` from the menu.
- Read the pane chips. The main terminal takes `1` plus a session name drawn from the host, `theforsakenlands` on the default world, or `session` before you connect.
- Watch channel talk in the `2 chat` pane, colored per channel with dim `HH:MM` timestamps.
- Watch raw session output tail through the `3 log · raw` pane below it.
- Glance at the status bar. While splits are open its center lists `1 session`, `2 chat`, and `3 log`, with the active pane marked.
- Run `close splits` from the same palette entry or menu item to fold back to a single well.

The choice persists across launches. Toggling never remounts the terminal, so the session never flickers. Both side panes stick to their bottom edge. Before any traffic the chat pane reads `no channel chat yet` and the log pane reads `quiet — raw session output tails here`.

### 4.8 Work the imm board

When you finish, staff work sorts itself worst first in one panel.

- Show the panel with `⌘K` and `show imm pane`, or place `imm` from the `panels` tab, where it lists as `imm (staff queues)`. Its home is the right column.
- Log in on an immortal. The server lights the panel at login with an `Imm.Queues` push. On a mortal it reads `no staff feed`.
- Read top down. Only queues with work appear, overdue sorts above nearing, and bigger counts rise. The queues are `dcheck`, `applications`, `journals`, `votes`, `notes`, `bugs`, `penalties`, `ideas`, and `typos`.
- Trust the chips. Rows trail `N overdue` and `N nearing`, applications add `N unread`, and journals add `N unawarded`.

The header sums the board as `N overdue`, `N nearing`, or `clear`. A count flashes when it grows. An empty board after the feed reads `all clear`.

## Tick and target

### 5.1 Run the tick timer

When you finish, a countdown chip rides your input row and warns you before every tick.

- Open the `tick & chips` tab in Settings.
- In the `timer` row, tick `enabled`. Add `sound on fire` if you want to hear it land.
- Set `interval` in seconds, anywhere from 1 to 3600.
- Put a command in `auto-fire` to send it on every tick. Leave it blank for none.
- Give `reset on` a regex. Every line that matches resets the countdown, so the MUD's own tick message keeps the timer honest.
- Enable `warn` and set how many seconds of lead you want, 5 by default. Fill `warn text` and `color` to restyle the warning. The color takes an ANSI name, `#rrggbb` hex, or a 256 palette index, and blank keeps the defaults.
- Pick a chip style under `input row chip`. `value only` is just the number, `caption + value` adds labels, and `icon + value` swaps them for compact icons. The chip renders at the right edge of the input row.
- Place the moons in the `status strip` section. `right edge`, `before the clock`, and `after the clock` position the Aabahran moon phases.

The ember vitals layout repeats the countdown in its pane head, so a sidebar glance works too. Changes here apply live.

### 5.2 Track a target with quick keys

When you finish, your target reads from three places and your quick keys sit one glance away.

- Watch the status bar once you set a target. The left block shows `tar` plus the target name, then your quick keys as name and verb pairs separated by `·`.
- Find the target in the room strip. Its character chip carries a `▶` marker, so you can pick it out of a crowd.
- Give combat its own pane. `combat` ships hidden, which renders it inline inside the vitals bar. Move it to a zone in the `panels` tab and it becomes a standalone pane with the target name over an hp track bar. It collapses when no target exists.
- Park `combat` and `vitals` together in the bottom zone and combat shrinks to a chip attached to the vitals block.
- Use a quick key. Type its name as the first word of a command and Vosh suppresses its own echo, because the backend echoes the expansion instead.

Quick keys live in the running session. They reset to the stock `gg`, `xx`, `zz`, and `tt` slots on restart, so set your verbs again with `#qkey` after each launch.

## Make it yours

### 6.1 Switch themes

When you finish you will have a new look applied across the whole app, terminal included.

- Click the gear button in the main window top bar to open the settings window.
- Pick the `themes` tab in the left rail. It sits under the `appearance` group beside `typography`.
- Read the catalog. Cards for the stock themes come first, then any custom themes tagged `custom`, then the dashed `+ new from active` card.
- Each card shows a state dot, the theme name, and three swatches for the surface, accent, and warn colors.
- Click a card. The theme activates on the spot, and the header note reads `changes save automatically`.

A theme changes both layers of the app. The chrome layer covers surfaces, text, borders, the accent pair, and the semantic warn, danger, info, and success colors. The terminal layer covers background, foreground, cursor, selection, and all sixteen ANSI colors. Some themes also turn on terminal tint by default. Ember ships with its pastel ANSI on.

Stock themes stay locked. A custom theme shows an `×` on its card while it is inactive, and clicking it deletes the theme immediately with no confirm dialog. If the theme you are using ever disappears, Vosh falls back to the default theme.

### 6.2 Build your own theme

When you finish you will have your own theme, forked from an existing look and recolored slot by slot.

- Open the settings window and pick the `themes` tab.
- Activate the theme you want as your starting point.
- Click the dashed `+ new from active` card at the end of the catalog. Vosh forks the active theme, gives it the base theme's name with `(custom)` appended, and switches to it.
- The editor appears under the catalog. Set the `label` and `description` fields first.
- Recolor the `chrome` section. Its groups are surfaces, text, borders, accent, and semantic.
- Recolor the `terminal` section. Its groups are terminal surfaces, selection, `ANSI 0-7`, and `ANSI 8-15 (bright)`.
- Adjust any slot with the color picker beside it, or type a value into its text field.

Every edit applies live in the main window, so keep it visible while you work. Saves happen automatically a beat after each change.

The text field commits a value only when CSS can render it. You can type an rgba value, and the picker strips it to hex for its own display. The editor only renders while the active theme is one of yours, so switching back to a stock theme hides it until you activate the custom one again.

### 6.3 Control terminal colors

When you finish you will control the terminal palette itself, plus the two accent colors Vosh draws over it.

- Open the settings window and pick the `themes` tab.
- Find the `tint` section and its `terminal tint` row.
- Check `tint output with theme` to recolor server output with the theme's own ANSI set, or uncheck it to show the base palette. Each theme picks its own default. Ember ships its pastel ANSI on.
- Scroll to the `terminal base palette` section. Its sixteen slots are the ANSI 0 to 15 colors used while `tint output with theme` is off.
- Edit any slot with its picker or text field. Touching one slot turns all sixteen into a custom list.
- Click `reset to canonical` to throw the custom list away and return to the canonical chart. The button only shows while your palette is custom.
- Set `sent command color` to recolor the local echo of every command you send. The row holds a color picker, a text field, and a `clear` button.
- Set `split scrollback divider` the same way to recolor the line between the history and live panes. Its text field placeholder reads `theme default (#rrggbb, rgba, named)`.

Both accent rows apply live the moment you commit a value. `clear` returns either one to the theme default.

### 6.4 Pick your fonts

When you finish the terminal will render in the face and size you chose.

- Open the settings window and pick the `typography` tab.
- In the `terminal face` section, click one of the quick chips. `BerkeleyMono`, `JetBrainsMono`, `Menlo`, `Monaco`, or `Courier New`. The first two ship inside Vosh, so they work on every machine.
- Or type a full CSS stack into the family field. Follow the placeholder shape, `"BerkeleyMono Bundled", Menlo, monospace`.
- Set the size with the number input. It accepts 9 to 32 px and defaults to 14.
- Check `bright text as bold` to render SGR bright colors 8 to 15 in the heavier cut. This applies on the native renderer only.
- To use a font installed on your machine, click into the `system fonts` filter field. Vosh loads the installed list on first focus, and the placeholder tells you when it is ready.
- Keep `monospace only` checked to hide proportional faces. The list shows up to 200 families, each row drawn in its own face.
- Click a family. Vosh sets your stack to that family with Menlo and monospace as fallbacks.
- Check the `preview` section. It renders a fixed sample line at your chosen family and size.

Changes save automatically about a quarter second after you stop typing, and a `saved.` note confirms it.

## Characters and data

### 7.1 Keep characters separate with profiles

When you finish, each character has its own profile carrying its aliases, triggers, macros, and variables, and the right one loads by itself when you connect.

- Click the gear button in the top bar to open Settings, then pick `profiles` under `characters` in the left rail.
- Type a name in the create row, `aabahran-erelei` for example, and click `+ new`. Names take letters, digits, `-`, `_`, and spaces.
- Click `auto-match` on the new row. Enter the `host`, an optional `port`, and `characters` as a list separated by commas, `Erelei, Akletus, Vanek` for example. Any listed name matches. Click `save`.
- Click `switch` on a row to change the active profile. Everything on this tab saves automatically.

The `scope` section decides what travels with a profile. Flip the pill from `global` to `profile` on any of `theme`, `font`, `dock layout`, `keep last command`, or `auto check updates`. Global keeps one value across every profile. Profile moves the value with the active profile, and the `font` row covers family and size as one toggle.

`duplicate` copies a profile's whole setup but deliberately leaves the auto match rules behind. You cannot delete the active profile, so switch away first. From the input bar, `#profile save` and `#profile load` write and reload the active profile's file on demand.

### 7.2 Group your automation with loadouts

When you finish, named loadouts flip whole groups of aliases, triggers, and macros on and off from one shared catalog.

- Open Settings, pick `import` under `tools`, and find the `migrate between scopes` section. Click `preview migration`.
- Review the plan. The wizard shows how your profiles would merge into a single shared catalog with one generated loadout per source profile. The preview writes nothing.
- Apply the migration. Vosh writes the catalog and parks your old per profile files in `profiles/legacy/`. Loadout mode waits for the next launch, so click `quit Vosh` in the wizard and reopen the app. The profile you were on becomes the sole active loadout.
- Reopen Settings. A `loadouts` tab now appears under `characters`. Check the boxes for the loadouts you want live. The runtime enables the union of their groups across every active loadout.

Click `deactivate all` to park the catalog dormant. Dormant disables every grouped alias, trigger, and macro, and it survives restarts and profile switches. Items without a group always stay live.

When no active loadout declares any enabled groups, the loadouts have no opinion and your durable checkbox state from the automation tabs stands. Activation is the only edit this tab makes. Author or reshape loadouts by editing `loadouts.toml` in the app data folder, or run the migration wizard again.

### 7.3 Bring your TinTin++ setup over

When you finish, your TinTin++ aliases and variables live in Vosh and the report tells you exactly what did not carry over.

- Type `#import-tintin <path>` in the input bar and point it at your `.tin` file. `~` expands, so `#import-tintin ~/aabahran.tin` works.
- Read the echo. It prints `imported <path>` and a count line like `12 aliases, 4 vars`.
- Check the `skipped (unsupported)` line. It tallies directives Vosh does not model by name, `event=2 ticker=1` for example, so you can port them by hand.
- Check the `unparsed` count. It flags alias or variable lines the parser could not read.

The importer handles `#alias {name} {expansion}` and `#variable {name} {value}`, with `#var` accepted as a short form. Nested braces and escaped braces inside the values parse correctly. The importer silently skips `#nop` lines and comments starting with `;`. Imported aliases overwrite existing aliases with the same name. Variables land at profile scope, so they persist with the profile.

Files from other clients go through Settings instead. Open the `import` tab, pick or paste a MUSHclient, Mudlet, GMUD, or `CMUD / zMUD` export, leave the format on `auto-detect`, and hit `apply`. The summary lists counts plus anything rejected, unsupported, or unparsed.

### 7.4 Search your session logs

When you finish, you can pull any line from any past session back out with a regex search.

- Open Settings and pick `logs` under `tools` in the left rail.
- Pick a scope in the sessions list on the left. `all sessions` searches everything. Clicking one session narrows the search to it, and each row shows the host, port, date, and line count.
- Type a pattern in the search box and click `search`. Patterns are regular expressions, so `dragon|wyvern` finds both.
- Tick `case` for case sensitive matching. Tick `show all` to lift the 500 result cap and return every match.

Each hit shows its timestamp, the host and port when you search across sessions, and the line in its original colors when the raw bytes are on record. Changing the session scope or either checkbox reruns the current search on its own. A new pattern needs another click on `search`.

The `copy` button on a session row copies that whole session to your clipboard as plain text. There is no file download yet. The store is `logs.sqlite` in the app data folder and it fills on every connection, so logging costs you nothing to set up.

### 7.5 Keep Vosh up to date

When you finish, Vosh checks for new builds on its own and installs them in place.

- Open Settings. The `general` tab opens by default. Scroll to the `app` section and find the `updates` row.
- Click `check now`. The status reads `checking…`, then `up to date` when nothing is newer.
- When a build is available, click the `install v<version> + restart` button that appears. The status shows `installing…`, then Vosh relaunches on the new build.
- Tick `check on launch` to run the check at every start. It is off by default. With it on, a banner appears in the main window when an update is waiting.

Updates download from the project's GitHub releases, and Vosh checks every build's signature before installing.

`auto check updates` is one of the five scope rows on the `profiles` tab. It defaults to global, so one setting covers every profile. Flip it to profile when one character should check on launch while the others stay quiet.

## Fix it

### 8.1 Switch renderers when the terminal looks wrong

When you finish, the terminal draws with the renderer you chose and the text matches the font you set.

Vosh ships two renderers. The native GPU surface is the default on macOS. The xterm renderer is the default on Windows and Linux.

- Type `#nativesurface off` to force the xterm renderer everywhere.
- Type `#nativesurface on` to force the native surface everywhere.
- Restart Vosh. The switch applies only on restart, and the echo reminds you with `restart Vosh to apply`.
- Type `#nativesurface default` to return to the platform default.

The command runs entirely in the frontend and stores your choice locally under the key `vosh.nativesurface`. A bad argument echoes `usage #nativesurface on | off | default (takes effect on restart)`.

If the text looks like the wrong typeface, open Settings and pick the `typography` tab. The default family stack starts with `BerkeleyMono Nerd Font` and falls through `JetBrains Mono`, `Fira Code`, `Menlo`, and `Consolas` before generic monospace. Your machine renders the first family in that stack it has installed, so install the font you want or move it to the front. Font size defaults to 14.

Fonts also follow profile scope. In Settings under `profiles`, the `font` row covers family and size as one toggle. Set it to `global` for one look everywhere or `profile` to let each profile carry its own.

On the xterm renderer the right click menu offers `clear buffer`. The native surface hides that item because its grid has no clear command.

### 8.2 Recover a bad connection

When you finish, the session chip shows a steady connected dot and the world responds again.

- Look at the session chip in the top bar. Its status dot shows idle, connecting, connected, or error, and while live the chip also shows your character name and `host:port`.
- Click the chip. A dropdown form opens with `host`, `port`, a `tls` checkbox, and a submit button that reads `connect` or `disconnect` depending on state.
- Press `disconnect`, wait for the dot to go idle, then press `connect`.
- Confirm the address. The defaults are host `play.theforsakenlands.com` and port `1848` with `tls` off.

Two other paths reach the same controls. Right click the terminal and pick `disconnect`, the item that appears at the bottom of the menu only while connected. Or press `Cmd+K` on macOS or `Ctrl+K` elsewhere and run the palette entry `connect` or `disconnect`.

The `tls` checkbox wraps the connection in TLS. Match it to what the server actually offers on that port. The default port `1848` expects it off.

Disconnecting has side effects worth knowing. Session scoped variables clear when the next connection opens, so anything set with `#var` never carries into the new session, while profile variables survive. The chat pane buffer clears at disconnect. When you reconnect, Vosh matches the host and port against your profiles and switches to the best match automatically, and it picks up a character pinned profile after login.

### 8.3 Find your data on disk

When you finish, you know which file holds each piece of your Vosh data and how to back it up.

Everything lives in one app data folder named `com.aabahran.vosh`.

- On macOS, open `~/Library/Application Support/com.aabahran.vosh`.
- On Linux, open `~/.local/share/com.aabahran.vosh`.
- On Windows, open `%APPDATA%\com.aabahran.vosh`.

Inside that folder.

- `profiles.toml` indexes your profiles and names the active one.
- `profiles/<name>.toml` holds each profile snapshot with connection defaults, aliases, variables, triggers, tick config, and macros.
- `global.toml` holds cross profile UI preferences.
- `catalog.toml` and `loadouts.toml` appear once loadout mode is active.
- `logs.sqlite` stores session logs, with `-wal` and `-shm` sidecars alongside.
- `scrollback.txt` persists the last 10,000 terminal lines across restarts.
- `maps.sqlite` stores map data.
- `scripts/` holds Lua files for `#script load`.
- `plugins/` holds plugin folders, each with a `manifest.toml`.

Every TOML save is safe by design. Vosh renames the old file to `<file>.bak.<timestamp>` with a millisecond timestamp, writes a temp file, swaps it in atomically, and keeps the ten newest backups. To roll back a bad profile edit, copy the backup you want over the live file.

A leftover `profile.toml` at the root is the legacy single profile file. Vosh migrates it to `profiles/default.toml` on the first multi profile launch.

## Reference

### 9.1 Find the right slash command

This is every slash command Vosh understands today, so you never guess at syntax.

- `#help` prints the command summary.
- `#alias <name> <expansion>` defines, `#unalias <name>` removes, `#aliases` lists.
- `#var <name> [value]` sets or shows a session variable, `#unvar <name>` removes it from both scopes, `#vars` lists.
- `#trigger <name> {pattern} <action> [args]` defines, `#untrigger <name>` removes, `#triggers` lists by priority.
- `#prompt {regex}` binds named captures to prompt vars, `#unprompt` removes it.
- `#group <name> on|off` toggles a group, `#group <name>` shows state, `#groups` lists.
- `#tick`, `#tick interval <secs>`, `#tick reset`, `#tick on {pattern}`, `#tick off`, `#tick fire <command>`, `#tick nofire`, `#tick sound on|off`, `#tick disable`, `#tick enable` drive the tick timer.
- `#tick warn`, `#tick warn at <secs>`, `#tick warn message <text>`, `#tick warn color <name>`, `#tick warn off` shape the tick warning.
- `#script load <name>` loads a Lua file, `#script reload` reruns loaded scripts, `#scripts` lists them.
- `#lua <code>` evaluates Lua inline.
- `#profile save`, `#profile load`, `#profile reset` manage the profile snapshot. In loadout mode all three become notices.
- `#import-tintin <path>` imports TinTin++ aliases and variables.
- `#record <name>` starts recording, `#record` shows status, `#record cancel` discards, `#endrec` saves the recording as an alias.
- `#qkey <name> <verb>` configures a quick key, `#qkey clear <name>` clears, `#qkeys` lists.
- `#target <args>` mirrors `tar`, with `#target clear|next|prev`, `#tarn`, `#tarp`, `#tarclear` as slash forms.
- `#nativesurface on|off|default` forces the renderer, applied on restart.

Targeting also works bare with no `#`. Type `tar` to list, `tar <N>` or `tar <substr>` to pick, `tarn` and `tarp` to cycle, `tarclear` to clear.

An unknown command points you at `#help`. Errors echo wrapped in square brackets.

### 9.2 Look up any keyboard shortcut

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
