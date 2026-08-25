// In-client help content. Source of truth for both the Help view
// (HelpView.tsx) and the standalone HELP.md document at the repo
// root. Keep them in sync; the markdown file mirrors this catalog
// one-to-one. Every topic is a task walkthrough in the project
// writing style. Bodies use the lightweight format HelpView parses:
//   - Paragraphs separated by a blank line (\n\n).
//   - Lines starting with "- " render as bullet list items.
//   - Backticks delimit inline code.

export interface HelpTopic {
  /** Stable id, used as the key in the topic rail. */
  id: string;
  /** Display number, like "1.3". Used in the rail and as a search target. */
  number: string;
  /** Short title shown in the rail and as the body heading. */
  title: string;
  /** Section heading the topic groups under in the rail. */
  section: string;
  /** Help body in the lightweight markdown subset described above. */
  body: string;
}

/** Section labels in the order they appear in the rail. */
export const HELP_SECTIONS: string[] = [
  'Get connected',
  'Play',
  'Automate',
  'Shape the window',
  'Tick and target',
  'Make it yours',
  'Characters and data',
  'Fix it',
  'Reference',
];

export const HELP_TOPICS: HelpTopic[] = [
  {
    id: 'get-connected.connect',
    number: '1.1',
    title: 'Connect to a world',
    section: 'Get connected',
    body: 'You finish this walkthrough logged in, with a live session chip in the top bar.\n\n- Click the session chip in the top bar. While idle it reads `connect` beside a status dot.\n- Fill in the host and port. The form defaults to `play.theforsakenlands.com` and `1848`. Tick the `tls` checkbox when your server offers TLS.\n- Click `connect` and watch the dot shift from connecting to connected.\n- Type your character name at the login prompt and press `Enter`.\n- When the server asks for your password, the input row swaps to a masked field with the placeholder `password`. Nothing you type shows on screen, echoes to the terminal, or lands in command history. Press `Enter` to submit. `Shift+Enter` submits here too instead of adding a line.\n\nOnce you are in, the chip shows your character name in lowercase along with the host and port. If a saved profile matches the host and port you dialed, Vosh switches to that profile before connecting. A profile pinned to a character attaches right after login, when the server reports who you are.\n\nTo leave, click the chip and press `disconnect`, or right click the terminal and choose `disconnect`.',
  },
  {
    id: 'get-connected.reconnect',
    number: '1.2',
    title: 'Reconnect and recover',
    section: 'Get connected',
    body: 'You come back from a dropped connection with your scrollback intact and your next commands already staged.\n\n- Read the session chip first. The status dot turns to its error state when the connection fails and back to idle when the session closes cleanly.\n- Click the chip and press `connect` to dial the same host and port again.\n- Review what happened before the drop. The terminal scrollback survives a disconnect, so scroll up or press `PageUp` and read back through it. Nothing clears it unless you pick `clear buffer` yourself.\n- Stage your recovery while offline. Type the first command, press `Shift+Enter` to stack more lines under it, and leave the block sitting in the compose box. After you reconnect, press `Enter` once and each line submits separately, in order.\n\nTwo things do reset. The chat pane buffer empties the moment the session drops, and session variables set with `#var` clear when the next connection opens, so they never carry into a new session. Aliases, triggers, macros, and profile variables stay loaded because they live in your profile, not in the connection.\n\nWhen you want out on purpose, `disconnect` lives in three places. The session chip form while connected, the terminal right click menu, and the `Cmd+K` palette.',
  },
  {
    id: 'get-connected.profile-save',
    number: '1.3',
    title: 'Make Vosh remember you',
    section: 'Get connected',
    body: 'You set Vosh up once and every launch after that starts with your aliases, triggers, and settings already in place.\n\n- Shape the client the way you want it. Define aliases, triggers, macros, variables, and tick settings.\n- Type `#profile save`. Vosh writes the snapshot to your active profile TOML under `~/Library/Application Support/com.aabahran.vosh`.\n- Or press `Cmd+K` and run the `#profile save` palette entry. It sends the same command.\n- Restart Vosh. The profile loads on startup with no extra step.\n\nWhat persists. Connection defaults, aliases, profile variables, triggers, tick configuration, macros, ui settings, enabled plugins, and the groups you disabled.\n\nWhat does not. Variables set with `#var` live in session scope. They clear when the next connection opens and never reach the file. A lasting value belongs in the `profile_vars` table of your profile file at `profiles/<name>.toml` in the app data folder. Edit it there while Vosh is closed, or set the value from Lua with `mud.set_profile_var`.\n\n`#profile load` pulls the saved file back into the live session. In loadout mode the profile commands become notices instead, because loadout mode saves your changes automatically.',
  },
  {
    id: 'play.send-commands',
    number: '2.1',
    title: 'Send commands',
    section: 'Play',
    body: 'You move from single commands to chained lines, multi line blocks, and safe bulk pastes.\n\n- Type a command and press `Enter` to send it.\n- Chain commands on one line with `;`. Each piece goes out as its own command. Type `\\;` when you mean a literal semicolon.\n- Press `Shift+Enter` to add a line without sending. The box grows and a line number gutter appears once you have two or more lines. Press `Enter` and every line submits separately, in order, with blank lines dropped.\n- Press `Enter` on an empty box to send a bare line. Many MUD prompts advance on that.\n- Paste multi line text straight into the input. A single line submits immediately. Two or more lines become a paste burst, sent one line every 500 ms by default, with a `paste N/M esc cancels` counter beside the prompt.\n- Press `Esc` during a burst to cancel every line that has not gone out yet. Starting a new paste also cancels the old burst.\n\nTurn on the `keep last command` setting and a sent command stays in the box fully selected. Press `Enter` again to resend it, or just start typing to replace it.\n\nThe burst delay is configurable from 0 to 10000 ms, so pace it to whatever your server tolerates.',
  },
  {
    id: 'play.reuse-history',
    number: '2.2',
    title: 'Reuse what you typed',
    section: 'Play',
    body: 'You recall anything you have sent this session without retyping it.\n\n- Press `Up` in an empty input to step back through your sent commands, newest first.\n- Type a few characters before pressing `Up` and recall becomes a prefix search. Type `tell` then `Up` and only lines starting with `tell` cycle past.\n- Press `Down` to step toward newer matches. One step past the newest restores exactly what you had typed before the search began.\n- Edit the recalled line whenever you like. Editing ends the search, and the next `Up` starts a fresh one from whatever is now in the box.\n\nHistory skips consecutive duplicates and never records anything you type in password mode.\n\nTurn on the `keep last command` setting and the line you just sent stays in the box, fully selected. `Enter` resends it and typing anything replaces it, which suits commands you repeat in quick succession.\n\nIn a multi line compose the arrows first do their normal job. `Up` moves the caret up a line unless you are already on the first line, and `Down` moves it down unless you are on the last, so history recall fires only from the edges of the block.',
  },
  {
    id: 'play.tab-complete',
    number: '2.3',
    title: 'Complete names with Tab',
    section: 'Play',
    body: 'You finish long names with a keypress instead of spelling them out in the middle of a fight.\n\n- Type the first letters of the name anywhere in your command line.\n- Press `Tab`. Vosh completes the word under the caret with its best match.\n- Press `Tab` again to cycle through the remaining candidates, or `Shift+Tab` to cycle backward. The list wraps around.\n- Keep typing, or press any other key, and the cycle resets with the current completion left in place.\n\nCandidates come from three sources, checked in this order.\n\n- Words from commands you have typed, most recent first.\n- Characters in your room, when the server sends `Room.Chars` over GMCP.\n- Capitalized names Vosh spotted in the output during the last 30 minutes.\n\nMatching is a case insensitive prefix match, duplicates collapse, and Vosh skips a candidate identical to what you already typed. Completion works on the word under the caret, so you can fix the middle of a line without touching the rest.',
  },
  {
    id: 'play.scroll-back',
    number: '2.4',
    title: 'Scroll back through history',
    section: 'Play',
    body: 'You read old output in a split pane while live output keeps flowing underneath.\n\n- Scroll the mouse wheel up over the terminal. The first notch opens a scrollback split above the live pane, and further scrolling walks the history line by line.\n- Or press `PageUp` to open the split and page upward, then `PageDown` to page back down. A Mac keyboard produces these with `Fn+Up` and `Fn+Down`.\n- Track your depth with the `↑ N / max` badge at the top right of the history pane.\n- Drag the divider between the panes to resize the split. It snaps to whole terminal rows. The handle also answers the keyboard, arrow keys nudge it 16px and `Shift` with an arrow jumps 64px.\n- Return to live three ways. Scroll or page down until history reaches its bottom and the split closes itself. Press `Esc`. Or middle click the terminal.\n\nThe live pane never scrolls away while the split is open. New output keeps landing at its tail, and the lines you type mirror into the history pane so the story stays continuous.\n\nOn the native macOS renderer there is no split. The surface scrolls its own grid, and `PageUp`, `PageDown`, and `Esc` still page it and snap it back to the bottom.',
  },
  {
    id: 'play.find-text',
    number: '2.5',
    title: 'Find text',
    section: 'Play',
    body: 'You jump straight to any text in the session, however far back it scrolled.\n\n- Press `Cmd+F` on macOS or `Ctrl+F` elsewhere. The find toolbar drops over the top right of the terminal, and it opens even while you are typing in the command input.\n- Type your query into the `find in scrollback` box.\n- Press `Enter` for the next match and `Shift+Enter` for the previous one. The `↑` and `↓` buttons do the same jobs.\n- Read the badge beside the box. It shows `N / M` while you step through hits and `no match` when the query finds nothing.\n- Sharpen the query with the three toggles. `case` makes it case sensitive, `word` matches whole words only, and `regex` treats the query as a regular expression.\n- Press `Esc` to close the toolbar, clear every highlight, and put focus back on the command input.\n\nA match above the visible screen opens the scrollback split with the hit highlighted near the top of the history pane. A match already on screen closes any open split instead.\n\nThe toolbar also opens from the terminal right click menu item `search scrollback` and from the `Cmd+K` palette.',
  },
  {
    id: 'play.copy-text',
    number: '2.6',
    title: 'Copy text out',
    section: 'Play',
    body: "You land terminal text on the clipboard, ready for notes, bug reports, or a friend.\n\n- Drag across the output you want. An active text selection stops the usual click from refocusing the command input, so your selection stays put.\n- Press `Cmd+C` on macOS or `Ctrl+C` elsewhere.\n- Or right click the terminal and choose `copy`. The menu shows the `⌘C` shortcut beside it.\n- Paste wherever you need it.\n\nOne priority rule. When the input box itself holds a selection, `Cmd+C` copies that selection rather than the terminal. Clear the input selection, or use the right click `copy` item, when the terminal text is what you want.\n\nThe trip works in reverse too. The right click menu's `paste` item inserts the clipboard into the input row without sending anything. Edit the line as long as you like, then press `Enter` yourself.\n\nOn the xterm renderer you can also wipe the buffer once you have copied what matters. Right click and choose `clear buffer`.",
  },
  {
    id: 'play.palette',
    number: '2.7',
    title: 'Drive everything from the palette',
    section: 'Play',
    body: 'You run nearly every Vosh action from one keyboard surface.\n\n- Press `Cmd+K` or `Ctrl+K` to open the palette. The same shortcut closes it again.\n- Type a few letters. Entries whose title starts with your text rank first, then title substrings, then hint and keyword matches.\n- Move the selection with the arrow keys and press `Enter` to run the highlighted entry.\n- Press `Tab` or `Shift+Tab` to cycle the scope filter when you want a single group.\n- Press `Esc` to close without running anything.\n\nFour groups live inside.\n\n- Commands. `connect` or `disconnect` depending on state, `open splits` and `close splits` for the well panes, `search scrollback`, `#profile save`, and `open help`.\n- Panes. A show or hide toggle for each panel (`map`, `group`, `vitals`, `roomstrip`, `chat`, `affects`, `combat`, `imm`).\n- Settings. One entry per Settings tab. Each opens the Settings window already on that tab.\n- Aliases. Every enabled alias. A parameterless alias runs the moment you pick it. An alias that takes arguments inserts its name into the input row instead, so you finish the line and press `Enter`.',
  },
  {
    id: 'play.right-click-menu',
    number: '2.8',
    title: 'Use the right click menu',
    section: 'Play',
    body: "You reach the terminal's everyday actions from a single menu under your pointer.\n\n- Right click anywhere on the terminal to open it.\n- Choose `copy` to copy the current selection. The menu lists its `⌘C` shortcut.\n- Choose `paste` to insert the clipboard into the input row. Nothing sends until you press `Enter` yourself.\n- Choose `open splits` or `close splits` to toggle the session, chat, and log panes inside the terminal well. These are workspace panes, separate from the scrollback split.\n- Choose `search scrollback` to open the find toolbar.\n- Choose `clear buffer` to wipe the terminal. The item appears only on the xterm renderer. The native macOS grid has no clear command, so the item hides there rather than sit dead.\n- While connected, `disconnect` waits at the bottom in danger styling and closes the session.\n\nThe menu closes on `Esc`, on a click anywhere outside it, or the instant you pick an item. It clamps itself to the window edges, so a right click near a corner never opens it half off screen.",
  },
  {
    id: 'automate.first-alias',
    number: '3.1',
    title: 'Make your first alias',
    section: 'Automate',
    body: 'When you finish, typing `kk dragon` will kick and backstab the dragon with one short command.\n\n- Click the gear button in the top bar to open settings, then pick the `aliases` tab.\n- Click `+ alias` to add a blank row.\n- Type `kk` in the name field.\n- Type `kick %1; backstab %1` in the expansion field. `;` splits the expansion into separate commands.\n- Click `save`. The unsaved dot clears and `saved.` appears.\n- Back in the main window, type `kk dragon`. Vosh sends `kick dragon` then `backstab dragon`.\n\nCaptures give aliases their reach. `%1` through `%9` pull the first through ninth word after the alias name. `%0` pulls the whole tail, `%1-` pulls word one through the end with spacing intact, and a missing word expands to nothing. `%%` gives a literal percent and `\\;` keeps a literal semicolon.\n\nYou can also define aliases without opening settings. `#alias gc get all corpse` sets one and echoes `alias gc set`. `#aliases` lists every alias and `#unalias gc` removes one.\n\nGive related aliases a shared group name to toggle them as a folder, either with the group checkbox in the tab or with `#group <name> on|off`. The `tpl` button on each row switches the expansion to a Lua script body when a template stops being enough.',
  },
  {
    id: 'automate.first-trigger',
    number: '3.2',
    title: 'Make your first trigger',
    section: 'Automate',
    body: 'When you finish, Vosh will loot every kill the moment the death line lands.\n\n- Open settings, pick the `triggers` tab, and keep the editor pill on `form`.\n- Click `+ trigger` to add a card.\n- Name it `auto-loot`. Leave priority at `5`, the default for a new card, and target on `line`.\n- Type `(\\w+) is DEAD!` in the pattern field. Patterns are regexes, so escape literal punctuation.\n- Click `+ send` and type `get all corpse` in the editor that appears.\n- Click `save`, then go kill something.\n\nA trigger pairs one visual with any number of effects. The visual chips are `none`, `highlight`, `replace`, and `gag`, and the effect buttons are `+ send`, `+ route`, and `+ script`. Send and replace templates reach capture groups with `$1` through `$9` or `${name}`, and `;` splits a send into separate commands. A card holds several patterns through `+ pattern` and fires when any enabled row matches. Higher priority triggers run first.\n\nThe input bar works too. `#trigger auto-loot {(\\w+) is DEAD!} send get all corpse` builds the same trigger with priority 0 on the `line` target. `#triggers` lists everything by priority and `#untrigger auto-loot` removes it. Vosh rejects an invalid regex and names the broken pattern, so nothing half works silently.',
  },
  {
    id: 'automate.highlight-lines',
    number: '3.3',
    title: 'Highlight lines that matter',
    section: 'Automate',
    body: 'When you finish, every tell will glow instead of scrolling past unnoticed.\n\n- Type `#trigger tell-glow {tells you} highlight bright_yellow bold`. Every line containing a tell now renders bright yellow and bold.\n- Upgrade it to a wash. Type `#trigger tell-glow {tells you} highlight bright_yellow wash`. Defining a trigger under an existing name replaces it.\n- Type `#triggers` to confirm the pattern and action.\n\nA plain highlight restyles the text. A wash tints the whole line with a dim quarter strength version of the highlight color, fills it edge to edge, and draws an accent bar at the left edge, so important lines read as banners you cannot miss.\n\nColors take the sixteen ANSI names. `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, and `white`, plus a `bright_` variant of each. `purple` maps to magenta and `gray` to `bright_black`. Stack `bold`, `underline`, and `inverse` freely, and add `bg:<color>` for a background.\n\nThe settings `triggers` tab offers the same power with selects. Pick the `highlight` visual chip on a trigger card, choose fg and bg from the color lists, and check `full-line wash`. Wash pairs well with a quiet color, since the tint covers the entire line.',
  },
  {
    id: 'automate.route-chat',
    number: '3.4',
    title: 'Route chat into panes',
    section: 'Automate',
    body: "When you finish, tells and gossip will collect in the chat pane instead of drowning in combat spam.\n\n- Open settings, pick the `triggers` tab, and click `+ trigger`.\n- Name the card `chat-feed`.\n- Type `tells you '` in the first pattern field.\n- Click `+ pattern` and type `gossips '` in the second row. The trigger fires when any enabled row matches.\n- Click `+ route` and type `chat` in the pane field.\n- Click `save`. Matching lines now land in the chat pane.\n\nRoute is an effect, so it stacks with anything else on the card. Pair it with a `highlight` visual to color the line, or add a `+ send` effect beside it. The pane field takes the pane's name, and `chat` is the placeholder the editor suggests.\n\nPlace the pane where you want it in the `panels` tab. Each panel row has a zone select drawn from the zones that panel supports, and the schematic preview up top moves chips with chevrons so you can order panels within a zone. The `side panels span full height` checkbox decides whether side panes run past the input row.\n\nThe inline form is `#trigger chat-feed {tells you '} route chat`. It creates a single pattern trigger, so build multi pattern feeds in the settings tab.",
  },
  {
    id: 'automate.variables',
    number: '3.5',
    title: 'Set and use variables',
    section: 'Automate',
    body: 'When you finish, one variable will feed every command you aim at it.\n\n- Type `#var potion yellow`. Vosh echoes `var potion set`.\n- Type `quaff $potion`. The line expands before it leaves, so the server receives `quaff yellow`.\n- Check a value with `#var potion`, list them all with `#vars`, and remove one with `#unvar potion`.\n\n`$potion` works when the name ends at whitespace or punctuation. Write `${potion}s` when letters follow immediately. `$$` sends a literal dollar sign, and unknown names pass through untouched, so `$100` reaches the server as typed.\n\nInterpolation runs on the line you type, before alias expansion, and Vosh does not interpolate alias output again. Put variables in the line you type, or resolve them in a Lua script body instead.\n\n`#var` writes session scope, which clears when the next connection opens, so a session value never survives into a new session. Profile variables persist across restarts in your profile TOML under `profile_vars`, and a session value shadows a profile value of the same name.\n\nVosh also fills session variables for you. GMCP binds `hp`, `maxhp`, `char_name`, `room_name`, `target_name`, and more, and setting a target with `tar` mirrors it into `$target`, so `cast dispel $target` always aims at your current mark.\n\nOne trap. Trigger send templates use `${name}` for regex capture groups, not this store, and trigger sends skip interpolation entirely.',
  },
  {
    id: 'automate.macros',
    number: '3.6',
    title: 'Bind keys to macros',
    section: 'Automate',
    body: "When you finish, `F1` will fire a command the instant you press it.\n\n- Open settings and pick the `macros` tab.\n- Click the key field in the top row. It reads `press a key...` while capturing.\n- Press `F1`. The field records the canonical key name.\n- Type the command, for example `stand; flee`. Use `;` to chain actions.\n- Type a group name if you want the binding in a toggleable folder, then click `add`.\n- Focus the input bar in the main window and press `F1`.\n\nRows apply on add or save, so the tab has no separate save step. Existing rows carry their own `save` and `delete` buttons.\n\nCapture accepts function keys, modifier combos like `Ctrl+N`, numpad keys like `Numpad7`, and plain printable keys. A binding fires only while the input bar has focus.\n\nTurn on `echo macro commands` in the `general` tab's `macros` row when you want each press to show what it sent. Group headers carry an `enabled` checkbox, and `#group <name> on|off` flips macro groups from the input bar along with matching alias and trigger groups.\n\n`#record` builds something different. It captures the commands you type and saves them as an alias you invoke by name, not by key. Use the macros tab when you want a key, `#record` when you want a word.",
  },
  {
    id: 'automate.slash-commands',
    number: '3.7',
    title: 'Command the client inline',
    section: 'Automate',
    body: 'When you finish, you will drive Vosh from the input bar without opening settings. Vosh handles every line that starts with `#` locally, and it never reaches the MUD.\n\n- Type `#help` any time for the full list.\n- Manage aliases with `#alias <name> <expansion>`, `#unalias <name>`, and `#aliases`.\n- Manage variables with `#var <name> [value]`, `#unvar <name>`, and `#vars`.\n- Manage triggers with `#trigger <name> {pattern} <action>`, `#untrigger <name>`, and `#triggers`.\n- Bind prompt stats with `#prompt {regex}` using named groups like `(?<hp>\\d+)`, and clear with `#unprompt`.\n- Flip whole folders with `#group <name> on|off` and inspect them with `#groups`.\n- Tune the tick with `#tick`, `#tick interval <secs>`, `#tick warn at <secs>`, and the rest listed under `#help`.\n- Record a command sequence with `#record <name>`, finish with `#endrec`, abort with `#record cancel`.\n- Configure quick keys with `#qkey <name> <verb>` and list them with `#qkeys`.\n- Drive Lua with `#script load <name>`, `#script reload`, `#scripts`, and `#lua <code>`.\n- Snapshot with `#profile save`, `#profile load`, and `#profile reset`.\n- Import TinTin++ files with `#import-tintin <path>`.\n- Work targets with `#target <args>`, or bare `tar`, `tarn`, `tarp`, and `tarclear` with no `#` at all.\n- Switch renderers with `#nativesurface on|off|default`, applied on restart.\n\nAn unknown command echoes a pointer to `#help`, and errors come back wrapped in square brackets.',
  },
  {
    id: 'automate.lua-scripts',
    number: '3.8',
    title: 'Script Vosh with Lua',
    section: 'Automate',
    body: 'When you finish, a Lua script will run inside Vosh and a plugin will load itself at every launch.\n\n- Save a script as `combat.lua` in the `scripts` folder under the app data directory, `~/Library/Application Support/com.aabahran.vosh/scripts/` on macOS.\n- Type `#script load combat`. Vosh appends `.lua` to a bare name.\n- Type `#scripts` to see loaded scripts and the triggers they registered.\n- Edit the file, then type `#script reload` to run every loaded script again.\n- Try one liners with `#lua mud.echo("hello")`.\n\nScripts talk to Vosh through the global `mud` table. `mud.send(text)` goes straight to the server and `mud.input(text)` feeds back through the input pipeline. `mud.echo(text)` prints locally. `mud.alias(name, expansion)` and `mud.trigger(name, pattern, callback)` register automation, with `captures[1]` holding the full match and `captures[2]` onward the groups. `mud.on_gmcp(package, callback)` hands you server data as a table, and `mud.timer(secs, callback)` schedules work you can cancel with `mud.cancel_timer`.\n\nLoads from `#script load` last for the session. For autoload, make a plugin. Create `plugins/<slug>/` under the app data directory with a `manifest.toml` naming the plugin and its entry script, `main.lua` by default. Enabled plugin names persist in your profile TOML under `[plugins]`, and every enabled plugin loads at launch. Enabling runs immediately, disabling takes effect next launch.\n\nThe sandbox strips file and process access. `require`, `io`, and `os.execute` are gone.',
  },
  {
    id: 'shape.arrange-panels',
    number: '4.1',
    title: 'Arrange the panels',
    section: 'Shape the window',
    body: "When you finish, every panel sits in the zone you chose at the size you want.\n\n- Click the gear in the top bar, titled `settings`, and open the `panels` tab.\n- Pick a home for each panel with its `zone` select. Zones are `top`, `bottom`, `left`, `right`, and `hidden`. The eight panels are `map`, `group`, `vitals`, `roomstrip`, `chat`, `affects`, `combat`, and `imm`.\n- Set `align` for a left or right panel. `top` panels stack down from the column top and `bottom` panels stack up from the floor. The map hides this select because it always fills the leftover column height.\n- Reorder panels within a zone using the up and down chevrons on the layout map chips.\n- Tick `side panels span full height (input lives under terminal only)` to run the side columns to the window's bottom edge.\n- Back in the main window, drag the 8px channel between the terminal and a column to resize it. A small ember tick fades in on the handle when you hover. Tab to a handle and press an arrow key to nudge 16px, or `Shift` plus an arrow for 64px.\n\nChanges save live. `reset to defaults` restores the stock layout. Panels render as raised cards with 8px of exposed ground at every seam, so moving one never leaves a scar.\n\nPrefer the keyboard for visibility. `⌘K` (`Ctrl+K` off macOS) lists a `show <id> pane` or `hide <id> pane` entry for every panel, and a panel you show again returns to its last visible zone.",
  },
  {
    id: 'shape.use-the-map',
    number: '4.2',
    title: 'Use the map',
    section: 'Shape the window',
    body: 'When you finish, the server map draws the way you like at the zoom you like.\n\n- The map fills the right column by default. Move it with its `zone` select in the `panels` tab. It accepts `left`, `right`, or `hidden` only, since a horizontal map at full width is unusable.\n- Toggle it from the map button in the top bar. The button flips the map between hidden and its last visible spot.\n- Read the header. It says `map` until the first room push arrives, then `map · <area>` with the area name from `Room.Info`.\n- Click the sliders button labeled `map controls` to open the controls row. Vosh remembers whether you left it open.\n- Switch draw modes with `squares`, `glyphs`, or `tileset`.\n- Zoom with `−` and `+`, or hold `Ctrl` and wheel over the map. The readout shows the percent and `⤺` resets it.\n- Check the status text. `radius N` means live map data. `waiting for server map` means none has arrived yet.\n\nTileset mode adds a bar with `load tileset` and `clear` buttons for your own tile art.',
  },
  {
    id: 'shape.chat-pane',
    number: '4.3',
    title: 'Tame the chat pane',
    section: 'Shape the window',
    body: "When you finish, channel talk collects in its own pane where you can filter it by channel.\n\n- Press `⌘K` and run `show chat pane`. Chat ships hidden and its home is the bottom strip.\n- Let lines arrive on their own. `Comm.Channel` GMCP feeds the pane automatically. Each line renders as `[pane] text` in its channel color, with the speaker shown as `Name: `.\n- Click a tab in the header to filter. `all` sits first, then one tab per channel name seen in the buffer. The count beside them reads `visible`, or `visible/total` while filtered.\n- Route trigger output in. In the `triggers` tab, add a `route` effect to a trigger and type a pane name. Those lines land in the chat pane under their own tab.\n- Drag the pane's handle to resize it. Click the `×` labeled `hide chat` to put it away.\n\nThe buffer holds a rolling 500 lines, survives closing and reopening the pane, and clears only on disconnect. The pane sticks to its tail. Scroll up to read back, and it sticks again once you come within 24px of the bottom.",
  },
  {
    id: 'shape.read-vitals',
    number: '4.4',
    title: 'Read your vitals',
    section: 'Shape the window',
    body: "When you finish, the hp, mana, and moves readout shows exactly what you want to see under pressure.\n\n- Open the `vitals` tab in Settings. Changes save automatically, and you can drag the live preview's bars to scrub the numbers.\n- Pick a layout, `ember`, `stacked`, or `inline`. Ember draws a sidebar pane with a `vitals` head, the tick countdown beside it, and three fixed thin bars with mono current and max numbers.\n- Outside ember, choose columns with the `bar`, `percent`, `numeric`, and `delta` pills, set `bar style` to `solid`, `ramped`, or `track`, and pick bar glyphs and width.\n- Open the `advanced` disclosure to recolor hp, mana, and moves, or turn on `drain through red as bars empty`.\n- Tick `pulse red vignette under 30% hp` for a warning you cannot miss when health drops low.\n- Turn on `custom template (overrides layout)` to write the readout yourself with tokens like `%hp`, `%pct_hp`, `%bar_hp`, `%tick`, and `%time`. Any `Char.Vitals` or `Char.Worth` field resolves as `%fieldname`, and `%%` prints a literal percent.\n\nPlace the bar in any zone from the `panels` tab. It ships in the right column, listed as `vitals (hp bar)`. Tracked affects also live in the `panels` tab, not here. `reset vitals` restores the stock config when an experiment goes wrong.",
  },
  {
    id: 'shape.group-affects',
    number: '4.5',
    title: 'Watch your group and affects',
    section: 'Shape the window',
    body: "When you finish, group health and your spell durations both read at a glance.\n\n- Find the `group` pane at the top of the right column. The header shows `group` plus a member count, or `solo`.\n- Group up. Each member gets a row with a mono name, a 44px hp mini bar, and the percent. The row tone drops through three tiers, healthy at 67% and up, warning down to 34%, danger below.\n- Stay solo and the pane shows your worth instead, tnl, exp, gold, bank, trains, and prac.\n- Configure affects in the `panels` tab under `tracked affects`. Type the server's affect name, add an optional label, and press `add`. Matching is case insensitive, and the affects pane renders nothing until you list at least one name.\n- Put `affects` in a side zone for full rows, each with a mono name, a duration mini bar filled by the fraction of a day remaining, and a countdown. In the top or bottom strip it compresses to pills, and absent tracked affects render dim with `—`.\n\nGroup data arrives from `Group.Info` and worth from `Char.Worth`. Affects come from `Char.Affects`, and duration color shifts with urgency. With no group the pane tells you so and suggests `follow <name>` to start one.",
  },
  {
    id: 'shape.room-strip',
    number: '4.6',
    title: 'Read the room strip',
    section: 'Shape the window',
    body: 'When you finish, one line tells you where you are, who is here, and what is on the ground.\n\n- Look at the strip along the top of the window. It runs area, room name with vnum, terrain, and the exit list in `N E S W U D` order.\n- Scan the `here` chips. Character chips color NPCs and players differently, stacks collapse to `(N) name`, and your current target carries a `▶` marker.\n- Scan the `items` chips. Each colors by type, so money, weapons, armor, potions, and food read apart at a glance.\n- Move the strip from the `panels` tab, where it lists as `room strip (area info)`. Drop it into a left or right column and it switches to a column variant that wraps onto multiple lines instead of scrolling sideways.\n\nGMCP feeds everything. `Room.Info` names the room, `Map.Tiles` colors the area, `Room.Chars` and `Room.Items` fill the chips, and the target marker follows the backend target. In the top strip, overflowing content fades out at the right edge. An empty slot holds its height so the layout never jumps.',
  },
  {
    id: 'shape.split-the-well',
    number: '4.7',
    title: 'Split the well',
    section: 'Shape the window',
    body: 'When you finish, the terminal well holds three panes, your session, channel chat, and a raw log tail.\n\n- Press `⌘K` and run `open splits`, or right click the terminal and pick `open splits` from the menu.\n- Read the pane chips. The main terminal takes `1` plus a session name drawn from the host, `theforsakenlands` on the default world, or `session` before you connect.\n- Watch channel talk in the `2 chat` pane, colored per channel with dim `HH:MM` timestamps.\n- Watch raw session output tail through the `3 log · raw` pane below it.\n- Glance at the status bar. While splits are open its center lists `1 session`, `2 chat`, and `3 log`, with the active pane marked.\n- Run `close splits` from the same palette entry or menu item to fold back to a single well.\n\nThe choice persists across launches. Toggling never remounts the terminal, so the session never flickers. Both side panes stick to their bottom edge. Before any traffic the chat pane reads `no channel chat yet` and the log pane reads `quiet — raw session output tails here`.',
  },
  {
    id: 'shape.imm-board',
    number: '4.8',
    title: 'Work the imm board',
    section: 'Shape the window',
    body: 'When you finish, staff work sorts itself worst first in one panel.\n\n- Show the panel with `⌘K` and `show imm pane`, or place `imm` from the `panels` tab, where it lists as `imm (staff queues)`. Its home is the right column.\n- Log in on an immortal. The server lights the panel at login with an `Imm.Queues` push. On a mortal it reads `no staff feed`.\n- Read top down. Only queues with work appear, overdue sorts above nearing, and bigger counts rise. The queues are `dcheck`, `applications`, `journals`, `votes`, `notes`, `bugs`, `penalties`, `ideas`, and `typos`.\n- Trust the chips. Rows trail `N overdue` and `N nearing`, applications add `N unread`, and journals add `N unawarded`.\n\nThe header sums the board as `N overdue`, `N nearing`, or `clear`. A count flashes when it grows. An empty board after the feed reads `all clear`.',
  },
  {
    id: 'tick.tick-timer',
    number: '5.1',
    title: 'Run the tick timer',
    section: 'Tick and target',
    body: "When you finish, a countdown chip rides your input row and warns you before every tick.\n\n- Open the `tick & chips` tab in Settings.\n- In the `timer` row, tick `enabled`. Add `sound on fire` if you want to hear it land.\n- Set `interval` in seconds, anywhere from 1 to 3600.\n- Put a command in `auto-fire` to send it on every tick. Leave it blank for none.\n- Give `reset on` a regex. Every line that matches resets the countdown, so the MUD's own tick message keeps the timer honest.\n- Enable `warn` and set how many seconds of lead you want, 5 by default. Fill `warn text` and `color` to restyle the warning. The color takes an ANSI name, `#rrggbb` hex, or a 256 palette index, and blank keeps the defaults.\n- Pick a chip style under `input row chip`. `value only` is just the number, `caption + value` adds labels, and `icon + value` swaps them for compact icons. The chip renders at the right edge of the input row.\n- Place the moons in the `status strip` section. `right edge`, `before the clock`, and `after the clock` position the Aabahran moon phases.\n\nThe ember vitals layout repeats the countdown in its pane head, so a sidebar glance works too. Changes here apply live.",
  },
  {
    id: 'tick.track-target',
    number: '5.2',
    title: 'Track a target with quick keys',
    section: 'Tick and target',
    body: 'When you finish, your target reads from three places and your quick keys sit one glance away.\n\n- Watch the status bar once you set a target. The left block shows `tar` plus the target name, then your quick keys as name and verb pairs separated by `·`.\n- Find the target in the room strip. Its character chip carries a `▶` marker, so you can pick it out of a crowd.\n- Give combat its own pane. `combat` ships hidden, which renders it inline inside the vitals bar. Move it to a zone in the `panels` tab and it becomes a standalone pane with the target name over an hp track bar. It collapses when no target exists.\n- Park `combat` and `vitals` together in the bottom zone and combat shrinks to a chip attached to the vitals block.\n- Use a quick key. Type its name as the first word of a command and Vosh suppresses its own echo, because the backend echoes the expansion instead.\n\nQuick keys live in the running session. They reset to the stock `gg`, `xx`, `zz`, and `tt` slots on restart, so set your verbs again with `#qkey` after each launch.',
  },
  {
    id: 'make-it-yours.switch-themes',
    number: '6.1',
    title: 'Switch themes',
    section: 'Make it yours',
    body: 'When you finish you will have a new look applied across the whole app, terminal included.\n\n- Click the gear button in the main window top bar to open the settings window.\n- Pick the `themes` tab in the left rail. It sits under the `appearance` group beside `typography`.\n- Read the catalog. Cards for the stock themes come first, then any custom themes tagged `custom`, then the dashed `+ new from active` card.\n- Each card shows a state dot, the theme name, and three swatches for the surface, accent, and warn colors.\n- Click a card. The theme activates on the spot, and the header note reads `changes save automatically`.\n\nA theme changes both layers of the app. The chrome layer covers surfaces, text, borders, the accent pair, and the semantic warn, danger, info, and success colors. The terminal layer covers background, foreground, cursor, selection, and all sixteen ANSI colors. Some themes also turn on terminal tint by default. Ember ships with its pastel ANSI on.\n\nStock themes stay locked. A custom theme shows an `×` on its card while it is inactive, and clicking it deletes the theme immediately with no confirm dialog. If the theme you are using ever disappears, Vosh falls back to the default theme.',
  },
  {
    id: 'make-it-yours.build-your-own-theme',
    number: '6.2',
    title: 'Build your own theme',
    section: 'Make it yours',
    body: "When you finish you will have your own theme, forked from an existing look and recolored slot by slot.\n\n- Open the settings window and pick the `themes` tab.\n- Activate the theme you want as your starting point.\n- Click the dashed `+ new from active` card at the end of the catalog. Vosh forks the active theme, gives it the base theme's name with `(custom)` appended, and switches to it.\n- The editor appears under the catalog. Set the `label` and `description` fields first.\n- Recolor the `chrome` section. Its groups are surfaces, text, borders, accent, and semantic.\n- Recolor the `terminal` section. Its groups are terminal surfaces, selection, `ANSI 0-7`, and `ANSI 8-15 (bright)`.\n- Adjust any slot with the color picker beside it, or type a value into its text field.\n\nEvery edit applies live in the main window, so keep it visible while you work. Saves happen automatically a beat after each change.\n\nThe text field commits a value only when CSS can render it. You can type an rgba value, and the picker strips it to hex for its own display. The editor only renders while the active theme is one of yours, so switching back to a stock theme hides it until you activate the custom one again.",
  },
  {
    id: 'make-it-yours.control-terminal-colors',
    number: '6.3',
    title: 'Control terminal colors',
    section: 'Make it yours',
    body: "When you finish you will control the terminal palette itself, plus the two accent colors Vosh draws over it.\n\n- Open the settings window and pick the `themes` tab.\n- Find the `tint` section and its `terminal tint` row.\n- Check `tint output with theme` to recolor server output with the theme's own ANSI set, or uncheck it to show the base palette. Each theme picks its own default. Ember ships its pastel ANSI on.\n- Scroll to the `terminal base palette` section. Its sixteen slots are the ANSI 0 to 15 colors used while `tint output with theme` is off.\n- Edit any slot with its picker or text field. Touching one slot turns all sixteen into a custom list.\n- Click `reset to canonical` to throw the custom list away and return to the canonical chart. The button only shows while your palette is custom.\n- Set `sent command color` to recolor the local echo of every command you send. The row holds a color picker, a text field, and a `clear` button.\n- Set `split scrollback divider` the same way to recolor the line between the history and live panes. Its text field placeholder reads `theme default (#rrggbb, rgba, named)`.\n\nBoth accent rows apply live the moment you commit a value. `clear` returns either one to the theme default.",
  },
  {
    id: 'make-it-yours.pick-your-fonts',
    number: '6.4',
    title: 'Pick your fonts',
    section: 'Make it yours',
    body: 'When you finish the terminal will render in the face and size you chose.\n\n- Open the settings window and pick the `typography` tab.\n- In the `terminal face` section, click one of the quick chips. `BerkeleyMono`, `JetBrainsMono`, `Menlo`, `Monaco`, or `Courier New`. The first two ship inside Vosh, so they work on every machine.\n- Or type a full CSS stack into the family field. Follow the placeholder shape, `"BerkeleyMono Bundled", Menlo, monospace`.\n- Set the size with the number input. It accepts 9 to 32 px and defaults to 14.\n- Check `bright text as bold` to render SGR bright colors 8 to 15 in the heavier cut. This applies on the native renderer only.\n- To use a font installed on your machine, click into the `system fonts` filter field. Vosh loads the installed list on first focus, and the placeholder tells you when it is ready.\n- Keep `monospace only` checked to hide proportional faces. The list shows up to 200 families, each row drawn in its own face.\n- Click a family. Vosh sets your stack to that family with Menlo and monospace as fallbacks.\n- Check the `preview` section. It renders a fixed sample line at your chosen family and size.\n\nChanges save automatically about a quarter second after you stop typing, and a `saved.` note confirms it.',
  },
  {
    id: 'characters-and-data.profiles',
    number: '7.1',
    title: 'Keep characters separate with profiles',
    section: 'Characters and data',
    body: "When you finish, each character has its own profile carrying its aliases, triggers, macros, and variables, and the right one loads by itself when you connect.\n\n- Click the gear button in the top bar to open Settings, then pick `profiles` under `characters` in the left rail.\n- Type a name in the create row, `aabahran-erelei` for example, and click `+ new`. Names take letters, digits, `-`, `_`, and spaces.\n- Click `auto-match` on the new row. Enter the `host`, an optional `port`, and `characters` as a list separated by commas, `Erelei, Akletus, Vanek` for example. Any listed name matches. Click `save`.\n- Click `switch` on a row to change the active profile. Everything on this tab saves automatically.\n\nThe `scope` section decides what travels with a profile. Flip the pill from `global` to `profile` on any of `theme`, `font`, `dock layout`, `keep last command`, or `auto check updates`. Global keeps one value across every profile. Profile moves the value with the active profile, and the `font` row covers family and size as one toggle.\n\n`duplicate` copies a profile's whole setup but deliberately leaves the auto match rules behind. You cannot delete the active profile, so switch away first. From the input bar, `#profile save` and `#profile load` write and reload the active profile's file on demand.",
  },
  {
    id: 'characters-and-data.loadouts',
    number: '7.2',
    title: 'Group your automation with loadouts',
    section: 'Characters and data',
    body: 'When you finish, named loadouts flip whole groups of aliases, triggers, and macros on and off from one shared catalog.\n\n- Open Settings, pick `import` under `tools`, and find the `migrate between scopes` section. Click `preview migration`.\n- Review the plan. The wizard shows how your profiles would merge into a single shared catalog with one generated loadout per source profile. The preview writes nothing.\n- Apply the migration. Vosh writes the catalog and parks your old per profile files in `profiles/legacy/`. Loadout mode waits for the next launch, so click `quit Vosh` in the wizard and reopen the app. The profile you were on becomes the sole active loadout.\n- Reopen Settings. A `loadouts` tab now appears under `characters`. Check the boxes for the loadouts you want live. The runtime enables the union of their groups across every active loadout.\n\nClick `deactivate all` to park the catalog dormant. Dormant disables every grouped alias, trigger, and macro, and it survives restarts and profile switches. Items without a group always stay live.\n\nWhen no active loadout declares any enabled groups, the loadouts have no opinion and your durable checkbox state from the automation tabs stands. Activation is the only edit this tab makes. Author or reshape loadouts by editing `loadouts.toml` in the app data folder, or run the migration wizard again.',
  },
  {
    id: 'characters-and-data.tintin-import',
    number: '7.3',
    title: 'Bring your TinTin++ setup over',
    section: 'Characters and data',
    body: 'When you finish, your TinTin++ aliases and variables live in Vosh and the report tells you exactly what did not carry over.\n\n- Type `#import-tintin <path>` in the input bar and point it at your `.tin` file. `~` expands, so `#import-tintin ~/aabahran.tin` works.\n- Read the echo. It prints `imported <path>` and a count line like `12 aliases, 4 vars`.\n- Check the `skipped (unsupported)` line. It tallies directives Vosh does not model by name, `event=2 ticker=1` for example, so you can port them by hand.\n- Check the `unparsed` count. It flags alias or variable lines the parser could not read.\n\nThe importer handles `#alias {name} {expansion}` and `#variable {name} {value}`, with `#var` accepted as a short form. Nested braces and escaped braces inside the values parse correctly. The importer silently skips `#nop` lines and comments starting with `;`. Imported aliases overwrite existing aliases with the same name. Variables land at profile scope, so they persist with the profile.\n\nFiles from other clients go through Settings instead. Open the `import` tab, pick or paste a MUSHclient, Mudlet, GMUD, or `CMUD / zMUD` export, leave the format on `auto-detect`, and hit `apply`. The summary lists counts plus anything rejected, unsupported, or unparsed.',
  },
  {
    id: 'characters-and-data.search-logs',
    number: '7.4',
    title: 'Search your session logs',
    section: 'Characters and data',
    body: 'When you finish, you can pull any line from any past session back out with a regex search.\n\n- Open Settings and pick `logs` under `tools` in the left rail.\n- Pick a scope in the sessions list on the left. `all sessions` searches everything. Clicking one session narrows the search to it, and each row shows the host, port, date, and line count.\n- Type a pattern in the search box and click `search`. Patterns are regular expressions, so `dragon|wyvern` finds both.\n- Tick `case` for case sensitive matching. Tick `show all` to lift the 500 result cap and return every match.\n\nEach hit shows its timestamp, the host and port when you search across sessions, and the line in its original colors when the raw bytes are on record. Changing the session scope or either checkbox reruns the current search on its own. A new pattern needs another click on `search`.\n\nThe `copy` button on a session row copies that whole session to your clipboard as plain text. There is no file download yet. The store is `logs.sqlite` in the app data folder and it fills on every connection, so logging costs you nothing to set up.',
  },
  {
    id: 'characters-and-data.stay-updated',
    number: '7.5',
    title: 'Keep Vosh up to date',
    section: 'Characters and data',
    body: "When you finish, Vosh checks for new builds on its own and installs them in place.\n\n- Open Settings. The `general` tab opens by default. Scroll to the `app` section and find the `updates` row.\n- Click `check now`. The status reads `checking…`, then `up to date` when nothing is newer.\n- When a build is available, click the `install v<version> + restart` button that appears. The status shows `installing…`, then Vosh relaunches on the new build.\n- Tick `check on launch` to run the check at every start. It is off by default. With it on, a banner appears in the main window when an update is waiting.\n\nUpdates download from the project's GitHub releases, and Vosh checks every build's signature before installing.\n\n`auto check updates` is one of the five scope rows on the `profiles` tab. It defaults to global, so one setting covers every profile. Flip it to profile when one character should check on launch while the others stay quiet.",
  },
  {
    id: 'fix-it.terminal-renderer',
    number: '8.1',
    title: 'Switch renderers when the terminal looks wrong',
    section: 'Fix it',
    body: 'When you finish, the terminal draws with the renderer you chose and the text matches the font you set.\n\nVosh ships two renderers. The native GPU surface is the default on macOS. The xterm renderer is the default on Windows and Linux.\n\n- Type `#nativesurface off` to force the xterm renderer everywhere.\n- Type `#nativesurface on` to force the native surface everywhere.\n- Restart Vosh. The switch applies only on restart, and the echo reminds you with `restart Vosh to apply`.\n- Type `#nativesurface default` to return to the platform default.\n\nThe command runs entirely in the frontend and stores your choice locally under the key `vosh.nativesurface`. A bad argument echoes `usage #nativesurface on | off | default (takes effect on restart)`.\n\nIf the text looks like the wrong typeface, open Settings and pick the `typography` tab. The default family stack starts with `BerkeleyMono Nerd Font` and falls through `JetBrains Mono`, `Fira Code`, `Menlo`, and `Consolas` before generic monospace. Your machine renders the first family in that stack it has installed, so install the font you want or move it to the front. Font size defaults to 14.\n\nFonts also follow profile scope. In Settings under `profiles`, the `font` row covers family and size as one toggle. Set it to `global` for one look everywhere or `profile` to let each profile carry its own.\n\nOn the xterm renderer the right click menu offers `clear buffer`. The native surface hides that item because its grid has no clear command.',
  },
  {
    id: 'fix-it.reconnect',
    number: '8.2',
    title: 'Recover a bad connection',
    section: 'Fix it',
    body: 'When you finish, the session chip shows a steady connected dot and the world responds again.\n\n- Look at the session chip in the top bar. Its status dot shows idle, connecting, connected, or error, and while live the chip also shows your character name and `host:port`.\n- Click the chip. A dropdown form opens with `host`, `port`, a `tls` checkbox, and a submit button that reads `connect` or `disconnect` depending on state.\n- Press `disconnect`, wait for the dot to go idle, then press `connect`.\n- Confirm the address. The defaults are host `play.theforsakenlands.com` and port `1848` with `tls` off.\n\nTwo other paths reach the same controls. Right click the terminal and pick `disconnect`, the item that appears at the bottom of the menu only while connected. Or press `Cmd+K` on macOS or `Ctrl+K` elsewhere and run the palette entry `connect` or `disconnect`.\n\nThe `tls` checkbox wraps the connection in TLS. Match it to what the server actually offers on that port. The default port `1848` expects it off.\n\nDisconnecting has side effects worth knowing. Session scoped variables clear when the next connection opens, so anything set with `#var` never carries into the new session, while profile variables survive. The chat pane buffer clears at disconnect. When you reconnect, Vosh matches the host and port against your profiles and switches to the best match automatically, and it picks up a character pinned profile after login.',
  },
  {
    id: 'fix-it.data-on-disk',
    number: '8.3',
    title: 'Find your data on disk',
    section: 'Fix it',
    body: 'When you finish, you know which file holds each piece of your Vosh data and how to back it up.\n\nEverything lives in one app data folder named `com.aabahran.vosh`.\n\n- On macOS, open `~/Library/Application Support/com.aabahran.vosh`.\n- On Linux, open `~/.local/share/com.aabahran.vosh`.\n- On Windows, open `%APPDATA%\\com.aabahran.vosh`.\n\nInside that folder.\n\n- `profiles.toml` indexes your profiles and names the active one.\n- `profiles/<name>.toml` holds each profile snapshot with connection defaults, aliases, variables, triggers, tick config, and macros.\n- `global.toml` holds cross profile UI preferences.\n- `catalog.toml` and `loadouts.toml` appear once loadout mode is active.\n- `logs.sqlite` stores session logs, with `-wal` and `-shm` sidecars alongside.\n- `scrollback.txt` persists the last 10,000 terminal lines across restarts.\n- `maps.sqlite` stores map data.\n- `scripts/` holds Lua files for `#script load`.\n- `plugins/` holds plugin folders, each with a `manifest.toml`.\n\nEvery TOML save is safe by design. Vosh renames the old file to `<file>.bak.<timestamp>` with a millisecond timestamp, writes a temp file, swaps it in atomically, and keeps the ten newest backups. To roll back a bad profile edit, copy the backup you want over the live file.\n\nA leftover `profile.toml` at the root is the legacy single profile file. Vosh migrates it to `profiles/default.toml` on the first multi profile launch.',
  },
  {
    id: 'reference.slash-commands',
    number: '9.1',
    title: 'Find the right slash command',
    section: 'Reference',
    body: 'This is every slash command Vosh understands today, so you never guess at syntax.\n\n- `#help` prints the command summary.\n- `#alias <name> <expansion>` defines, `#unalias <name>` removes, `#aliases` lists.\n- `#var <name> [value]` sets or shows a session variable, `#unvar <name>` removes it from both scopes, `#vars` lists.\n- `#trigger <name> {pattern} <action> [args]` defines, `#untrigger <name>` removes, `#triggers` lists by priority.\n- `#prompt {regex}` binds named captures to prompt vars, `#unprompt` removes it.\n- `#group <name> on|off` toggles a group, `#group <name>` shows state, `#groups` lists.\n- `#tick`, `#tick interval <secs>`, `#tick reset`, `#tick on {pattern}`, `#tick off`, `#tick fire <command>`, `#tick nofire`, `#tick sound on|off`, `#tick disable`, `#tick enable` drive the tick timer.\n- `#tick warn`, `#tick warn at <secs>`, `#tick warn message <text>`, `#tick warn color <name>`, `#tick warn off` shape the tick warning.\n- `#script load <name>` loads a Lua file, `#script reload` reruns loaded scripts, `#scripts` lists them.\n- `#lua <code>` evaluates Lua inline.\n- `#profile save`, `#profile load`, `#profile reset` manage the profile snapshot. In loadout mode all three become notices.\n- `#import-tintin <path>` imports TinTin++ aliases and variables.\n- `#record <name>` starts recording, `#record` shows status, `#record cancel` discards, `#endrec` saves the recording as an alias.\n- `#qkey <name> <verb>` configures a quick key, `#qkey clear <name>` clears, `#qkeys` lists.\n- `#target <args>` mirrors `tar`, with `#target clear|next|prev`, `#tarn`, `#tarp`, `#tarclear` as slash forms.\n- `#nativesurface on|off|default` forces the renderer, applied on restart.\n\nTargeting also works bare with no `#`. Type `tar` to list, `tar <N>` or `tar <substr>` to pick, `tarn` and `tarp` to cycle, `tarclear` to clear.\n\nAn unknown command points you at `#help`. Errors echo wrapped in square brackets.',
  },
  {
    id: 'reference.keyboard-shortcuts',
    number: '9.2',
    title: 'Look up any keyboard shortcut',
    section: 'Reference',
    body: 'This is every built in key Vosh binds, grouped by where it works.\n\nIn the command input.\n\n- `Enter` submits. `Shift+Enter` inserts a newline for multi line compose, and in password mode it submits instead.\n- `Tab` and `Shift+Tab` cycle tab completion through your history words, room characters, and recently seen names.\n- `ArrowUp` and `ArrowDown` recall history, filtered by whatever prefix you already typed.\n- `PageUp` and `PageDown` page the scrollback. On macOS press `Fn+Up` and `Fn+Down`.\n- `Escape` cancels an in flight paste burst, closes the scrollback split, and snaps the terminal to its tail.\n- `Home` and `End` jump the caret, also reachable as `Cmd+Left` and `Cmd+Right` or `Fn+Left` and `Fn+Right` on macOS. Add `Shift` to extend the selection.\n- `Cmd+C` or `Ctrl+C` with nothing selected in the input copies the native surface selection.\n\nAnywhere in the window.\n\n- `Cmd+F` or `Ctrl+F` opens the find toolbar.\n- `Cmd+K` or `Ctrl+K` toggles the command palette.\n\nIn the find toolbar. `Enter` finds the next match, `Shift+Enter` the previous, `Escape` closes and clears.\n\nIn the command palette. `ArrowUp` and `ArrowDown` move the selection, `Enter` runs the entry, `Tab` and `Shift+Tab` cycle the scope filter, `Escape` closes.\n\nMouse on the terminal. Wheel up opens the scrollback split on the xterm renderer, the native surface scrolls its own grid. Middle click closes the split and snaps to bottom. Right click opens the terminal menu.\n\nBind your own keys as macros in Settings under `macros`. Canonical names look like `F1`, `Ctrl+N`, `Shift+F5`, and `Ctrl+Alt+Numpad7`.',
  },
];

export function searchTopics(query: string, topics: HelpTopic[] = HELP_TOPICS): HelpTopic[] {
  const q = query.trim().toLowerCase();
  if (q.length === 0) return topics;
  return topics.filter((t) => {
    if (t.number.toLowerCase().includes(q)) return true;
    if (t.title.toLowerCase().includes(q)) return true;
    if (t.section.toLowerCase().includes(q)) return true;
    if (t.body.toLowerCase().includes(q)) return true;
    return false;
  });
}
