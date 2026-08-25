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
    body: 'Connections start from the session chip in the top bar. While idle the chip reads `connect` beside a status dot.\n\n- Click the session chip.\n- Fill in the host and port. The form defaults to `play.theforsakenlands.com` and `1848`. Tick the `tls` checkbox when your server offers TLS.\n- Click `connect`. The dot shifts from connecting to connected.\n- Type your character name at the login prompt and press `Enter`.\n- When the server asks for a password, the input row swaps to a masked field with the placeholder `password`. Nothing you type shows on screen, echoes to the terminal, or lands in command history. Press `Enter` to submit. `Shift+Enter` submits here too instead of adding a line.\n\nWhile connected, the chip shows your character name in lowercase along with the host and port. If a saved profile matches the host and port you dialed, Vosh switches to that profile before connecting. A profile pinned to a character attaches right after login, when the server reports who you are.\n\nTo disconnect, click the chip and press `disconnect`, or right click the terminal and choose `disconnect`.',
  },
  {
    id: 'get-connected.reconnect',
    number: '1.2',
    title: 'Reconnect',
    section: 'Get connected',
    body: 'The session chip reports connection state through its status dot. The dot turns to its error state when the connection fails and back to idle when the session closes cleanly.\n\n- Click the chip and press `connect` to dial the same host and port again.\n- Scroll up or press `PageUp` to read output from before the drop. The terminal scrollback survives a disconnect, and nothing clears it unless you pick `clear buffer` yourself.\n- To stage commands while offline, type the first command, press `Shift+Enter` to stack more lines under it, and leave the block in the compose box. After you reconnect, press `Enter` once and each line submits separately, in order.\n\nTwo things reset on a disconnect. The chat pane buffer empties the moment the session drops, and session variables set with `#var` clear when the next connection opens, so they never carry into a new session. Aliases, triggers, macros, and profile variables stay loaded because they live in your profile, not in the connection.\n\n`disconnect` lives in three places. The session chip form while connected, the terminal right click menu, and the `Cmd+K` palette.',
  },
  {
    id: 'get-connected.profile-save',
    number: '1.3',
    title: 'Save your profile',
    section: 'Get connected',
    body: '`#profile save` writes the current client state to the active profile file, and the profile loads again on startup with no extra step. The file is a TOML snapshot under `~/Library/Application Support/com.aabahran.vosh`.\n\n- Set up the client state you want to keep. Aliases, triggers, macros, variables, and tick settings all count.\n- Type `#profile save` in the input bar. Vosh writes the snapshot to the active profile TOML.\n- Or press `Cmd+K` and run the `#profile save` palette entry. It sends the same command.\n\nThe snapshot covers connection defaults, aliases, profile variables, triggers, tick configuration, macros, ui settings, enabled plugins, and the groups you disabled.\n\nVariables set with `#var` live in session scope. They clear when the next connection opens and never reach the file. A lasting value belongs in the `profile_vars` table of your profile file at `profiles/<name>.toml` in the app data folder. Edit it there while Vosh is closed, or set the value from Lua with `mud.set_profile_var`.\n\n`#profile load` pulls the saved file back into the live session. In loadout mode the profile commands become notices instead, because loadout mode saves your changes automatically.',
  },
  {
    id: 'play.send-commands',
    number: '2.1',
    title: 'Send commands',
    section: 'Play',
    body: 'The command input sends lines to the server. It handles single commands, chained commands, multi line blocks, and pastes.\n\n- Type a command and press `Enter` to send it.\n- Chain commands on one line with `;`. Each piece goes out as its own command. Type `\\;` for a literal semicolon.\n- Press `Shift+Enter` to add a line without sending. The box grows and a line number gutter appears once it holds two or more lines. Press `Enter` and every line submits separately, in order, with blank lines dropped.\n- Press `Enter` on an empty box to send a bare line. Many MUD prompts advance on that.\n- Paste multi line text straight into the input. A single line submits immediately. Two or more lines become a paste burst, sent one line every 500 ms by default, with a `paste N/M esc cancels` counter beside the prompt.\n- Press `Esc` during a burst to cancel every line that has not gone out yet. Starting a new paste also cancels the old burst.\n\nWith the `keep last command` setting on, a sent command stays in the box fully selected. Press `Enter` again to resend it, or start typing to replace it.\n\nThe burst delay is configurable from 0 to 10000 ms.',
  },
  {
    id: 'play.reuse-history',
    number: '2.2',
    title: 'Recall command history',
    section: 'Play',
    body: 'Command history records every line you send during a session and replays it from the input bar.\n\n- Press `Up` in an empty input to step back through sent commands, newest first.\n- Type a few characters before pressing `Up` to turn recall into a prefix search. Only lines starting with that text cycle past.\n- Press `Down` to step toward newer matches. One step past the newest restores exactly what you had typed before the search began.\n- Edit the recalled line at any point. Editing ends the search, and the next `Up` starts a fresh one from whatever is now in the box.\n\nHistory skips consecutive duplicates and never records anything you type in password mode.\n\nWith the `keep last command` setting on, the line you just sent stays in the box fully selected. `Enter` resends it and typing anything replaces it.\n\nIn a multi line compose the arrows do their normal job first. `Up` moves the caret up a line unless you are already on the first line, and `Down` moves it down unless you are on the last, so history recall fires only from the edges of the block.\n\nExample. Type `tell` and press `Up` to cycle through only the lines that start with `tell`.',
  },
  {
    id: 'play.tab-complete',
    number: '2.3',
    title: 'Complete names with Tab',
    section: 'Play',
    body: 'Tab completion finishes a partly typed word in the command line from names Vosh already knows.\n\n- Type the first letters of the word anywhere in the command line.\n- Press `Tab`. Vosh completes the word under the caret with its best match.\n- Press `Tab` again to cycle through the remaining candidates, or `Shift+Tab` to cycle backward. The list wraps around.\n- Keep typing, or press any other key, and the cycle resets with the current completion left in place.\n\nCandidates come from three sources, checked in this order.\n\n- Words from commands you have typed, most recent first.\n- Characters in your room, when the server sends `Room.Chars` over GMCP.\n- Capitalized names Vosh spotted in the output during the last 30 minutes.\n\nMatching is a case insensitive prefix match, duplicates collapse, and Vosh skips a candidate identical to what you already typed. Completion works on the word under the caret, so you can edit the middle of a line without touching the rest.',
  },
  {
    id: 'play.scroll-back',
    number: '2.4',
    title: 'Scroll back through history',
    section: 'Play',
    body: 'Scrollback opens in a split pane above the live terminal, so old output stays readable while new output keeps flowing underneath.\n\n- Scroll the mouse wheel up over the terminal. The first notch opens the scrollback split above the live pane, and further scrolling walks the history line by line.\n- Or press `PageUp` to open the split and page upward, then `PageDown` to page back down. A Mac keyboard produces these with `Fn+Up` and `Fn+Down`.\n- Read the `↑ N / max` badge at the top right of the history pane to see your depth.\n- Drag the divider between the panes to resize the split. It snaps to whole terminal rows. The handle also answers the keyboard, arrow keys nudge it 16px and `Shift` with an arrow jumps 64px.\n- Return to live three ways. Scroll or page down until history reaches its bottom and the split closes itself. Press `Esc`. Or middle click the terminal.\n\nThe live pane never scrolls away while the split is open. New output keeps landing at its tail, and the lines you type mirror into the history pane so the record stays continuous.\n\nOn the native macOS renderer there is no split. The surface scrolls its own grid, and `PageUp`, `PageDown`, and `Esc` still page it and snap it back to the bottom.',
  },
  {
    id: 'play.find-text',
    number: '2.5',
    title: 'Find text',
    section: 'Play',
    body: 'The find toolbar searches the whole session scrollback. It drops over the top right of the terminal.\n\n- Press `Cmd+F` on macOS or `Ctrl+F` elsewhere. The toolbar opens even while you are typing in the command input.\n- Type your query into the `find in scrollback` box.\n- Press `Enter` for the next match and `Shift+Enter` for the previous one. The `↑` and `↓` buttons do the same jobs.\n- Read the badge beside the box. It shows `N / M` while you step through hits and `no match` when the query finds nothing.\n- Narrow the query with the three toggles. `case` makes it case sensitive, `word` matches whole words only, and `regex` treats the query as a regular expression.\n- Press `Esc` to close the toolbar, clear every highlight, and return focus to the command input.\n\nA match above the visible screen opens the scrollback split with the hit highlighted near the top of the history pane. A match already on screen closes any open split instead.\n\nThe toolbar also opens from the terminal right click menu item `search scrollback` and from the `Cmd+K` palette.',
  },
  {
    id: 'play.copy-text',
    number: '2.6',
    title: 'Copy terminal text',
    section: 'Play',
    body: "Terminal text copies to the system clipboard through a drag selection.\n\n- Drag across the output you want. An active text selection stops the usual click from refocusing the command input, so the selection stays put.\n- Press `Cmd+C` on macOS or `Ctrl+C` elsewhere.\n- Or right click the terminal and choose `copy`. The menu shows the `⌘C` shortcut beside it.\n\nOne priority rule. When the input box itself holds a selection, `Cmd+C` copies that selection rather than the terminal. Clear the input selection, or use the right click `copy` item, when the terminal text is what you want.\n\nThe right click menu's `paste` item inserts the clipboard into the input row without sending anything. Edit the line as needed, then press `Enter` yourself.\n\nOn the xterm renderer the right click menu also offers `clear buffer`, which wipes the terminal buffer.",
  },
  {
    id: 'play.palette',
    number: '2.7',
    title: 'Use the command palette',
    section: 'Play',
    body: 'The command palette runs Vosh actions from the keyboard. It covers commands, pane toggles, settings tabs, and aliases.\n\n- Press `Cmd+K` or `Ctrl+K` to open the palette. The same shortcut closes it again.\n- Type a few letters to filter. Entries whose title starts with your text rank first, then title substrings, then hint and keyword matches.\n- Move the selection with the arrow keys and press `Enter` to run the highlighted entry.\n- Press `Tab` or `Shift+Tab` to cycle the scope filter and show a single group.\n- Press `Esc` to close without running anything.\n\nThe palette holds four groups.\n\n- Commands. `connect` or `disconnect` depending on state, `open splits` and `close splits` for the well panes, `search scrollback`, `#profile save`, and `open help`.\n- Panes. A show or hide toggle for each panel (`map`, `group`, `vitals`, `roomstrip`, `chat`, `affects`, `combat`, `imm`).\n- Settings. One entry per Settings tab. Each opens the Settings window already on that tab.\n- Aliases. Every enabled alias. A parameterless alias runs the moment you pick it. An alias that takes arguments inserts its name into the input row instead, so you finish the line and press `Enter`.',
  },
  {
    id: 'play.right-click-menu',
    number: '2.8',
    title: 'Use the right click menu',
    section: 'Play',
    body: "The terminal right click menu collects the terminal's everyday actions in one place.\n\n- Right click anywhere on the terminal to open it.\n- `copy` copies the current selection. The menu lists its `⌘C` shortcut.\n- `paste` inserts the clipboard into the input row. Nothing sends until you press `Enter` yourself.\n- `open splits` and `close splits` toggle the session, chat, and log panes inside the terminal well. These are workspace panes, separate from the scrollback split.\n- `search scrollback` opens the find toolbar.\n- `clear buffer` wipes the terminal. The item appears only on the xterm renderer. The native macOS grid has no clear command, so the item hides there.\n- While connected, `disconnect` sits at the bottom in danger styling and closes the session.\n\nThe menu closes on `Esc`, on a click anywhere outside it, or the instant you pick an item. It clamps itself to the window edges, so a right click near a corner never opens it half off screen.",
  },
  {
    id: 'automate.first-alias',
    number: '3.1',
    title: 'Create an alias',
    section: 'Automate',
    body: 'Aliases expand a short name into one or more commands. They live in the settings window under `aliases`, and the input bar defines them too.\n\n- Click the gear button in the top bar to open settings, then pick the `aliases` tab.\n- Click `+ alias` to add a blank row.\n- Enter a name in the name field.\n- Enter the expansion in the expansion field. `;` splits the expansion into separate commands and `\\;` keeps a literal semicolon.\n- Click `save`. The unsaved dot clears and `saved.` appears.\n\nCaptures pull words from the line you typed. `%1` through `%9` pull the first through ninth word after the alias name. `%0` pulls the whole tail, `%1-` pulls word one through the end with spacing intact, and a missing word expands to nothing. `%%` gives a literal percent.\n\nGive related aliases a shared group name to toggle them as a folder, either with the group checkbox in the tab or with `#group <name> on|off`. The `tpl` button on each row switches the expansion to a Lua script body.\n\nExample. An alias named `kk` with the expansion `kick %1; backstab %1` turns `kk dragon` into `kick dragon` followed by `backstab dragon`.\n\nThe input bar defines aliases too. `#alias gc get all corpse` sets one and echoes `alias gc set`, `#aliases` lists every alias, and `#unalias gc` removes one.',
  },
  {
    id: 'automate.first-trigger',
    number: '3.2',
    title: 'Create a trigger',
    section: 'Automate',
    body: 'Triggers watch incoming lines and run actions when a pattern matches. They live in the settings window under `triggers`, and a trigger pairs one visual with any number of effects.\n\n- Open settings and pick the `triggers` tab. Keep the editor pill on `form`.\n- Click `+ trigger` to add a card.\n- Enter a name and a pattern. Patterns are regexes, so escape literal punctuation. `+ pattern` adds more rows and the card fires when any enabled row matches.\n- Leave priority at `5`, the default for a new card, or raise it to run before other triggers. Higher priority triggers run first. Leave the target on `line`.\n- Pick a visual. The chips are `none`, `highlight`, `replace`, and `gag`.\n- Add effects with `+ send`, `+ route`, and `+ script`. Each button opens an editor for its text. Send and replace templates reach capture groups with `$1` through `$9` or `${name}`, and `;` splits a send into separate commands.\n- Click `save`.\n\nExample. The pattern `(\\w+) is DEAD!` with a send of `get all corpse` loots each kill as the death line arrives.\n\nThe input bar builds triggers too. `#trigger name {pattern} send command` creates one at priority 0 on the `line` target, `#triggers` lists everything by priority, and `#untrigger name` removes one. Vosh rejects an invalid regex and names the broken pattern.',
  },
  {
    id: 'automate.highlight-lines',
    number: '3.3',
    title: 'Highlight lines',
    section: 'Automate',
    body: 'A highlight trigger restyles every line that matches a pattern. Define one from the input bar with `#trigger` or from the settings `triggers` tab.\n\n- Type `#trigger <name> {pattern} highlight <color> [styles]`. Every line matching the pattern renders in that color and style.\n- Add `wash` to the style list to tint the whole line instead of restyling the text alone.\n- Type `#triggers` to confirm the pattern and action. Defining a trigger under an existing name replaces it.\n\nA plain highlight restyles the text. A wash tints the whole line with a dim quarter strength version of the highlight color, fills it edge to edge, and draws an accent bar at the left edge. Wash pairs well with a quiet color, since the tint covers the entire line.\n\nColors take the sixteen ANSI names. `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, and `white`, plus a `bright_` variant of each. `purple` maps to magenta and `gray` to `bright_black`. Stack `bold`, `underline`, and `inverse` freely, and add `bg:<color>` for a background.\n\nExample. `#trigger tell-glow {tells you} highlight bright_yellow bold` renders every tell bright yellow and bold. `#trigger tell-glow {tells you} highlight bright_yellow wash` replaces it with a full line wash.\n\nThe settings `triggers` tab offers the same options with selects. Pick the `highlight` visual chip on a trigger card, choose fg and bg from the color lists, and check `full-line wash`.',
  },
  {
    id: 'automate.route-chat',
    number: '3.4',
    title: 'Route lines to a pane',
    section: 'Automate',
    body: "A route effect sends matching lines to a named pane. Build one on a trigger card in the settings `triggers` tab.\n\n- Open settings, pick the `triggers` tab, and click `+ trigger`.\n- Enter a name for the card.\n- Enter a pattern. Click `+ pattern` to add more rows. The trigger fires when any enabled row matches.\n- Click `+ route` and enter the pane's name in the pane field. `chat` is the placeholder the editor suggests.\n- Click `save`. Matching lines land in the pane.\n\nRoute is an effect, so it stacks with anything else on the card. Pair it with a `highlight` visual to color the line, or add a `+ send` effect beside it.\n\nPlace the pane where you want it in the `panels` tab. Each panel row has a zone select drawn from the zones that panel supports, and the schematic preview up top moves chips with chevrons so you can order panels within a zone. The `side panels span full height` checkbox decides whether side panes run past the input row.\n\nExample. A card named `chat-feed` with the patterns `tells you '` and `gossips '` and a route to `chat` collects tells and gossip in the chat pane.\n\nThe inline form is `#trigger chat-feed {tells you '} route chat`. It creates a single pattern trigger, so build multi pattern feeds in the settings tab.",
  },
  {
    id: 'automate.variables',
    number: '3.5',
    title: 'Set and use variables',
    section: 'Automate',
    body: 'Variables store values you reference in commands as `$name`. Set them from the input bar with `#var`, and Vosh expands them in the lines you type before they leave.\n\n- Type `#var <name> <value>` to set a variable. Vosh echoes `var <name> set`.\n- Reference it in any command as `$name`. The line expands before it leaves, so the server receives the value.\n- Type `#var <name>` to check a value, `#vars` to list them all, and `#unvar <name>` to remove one.\n\n`$name` works when the name ends at whitespace or punctuation. Wrap the name in braces, as `${name}`, when letters follow immediately. `$$` sends a literal dollar sign, and unknown names pass through untouched, so `$100` reaches the server as typed.\n\nInterpolation runs on the line you type, before alias expansion, and Vosh does not interpolate alias output again. Put variables in the line you type, or resolve them in a Lua script body instead.\n\n`#var` writes session scope, which clears when the next connection opens, so a session value never survives into a new session. Profile variables persist across restarts in your profile TOML under `profile_vars`, and a session value shadows a profile value of the same name.\n\nVosh also fills session variables on its own. GMCP binds `hp`, `maxhp`, `char_name`, `room_name`, `target_name`, and more, and setting a target with `tar` mirrors it into `$target`.\n\nTrigger send templates use `${name}` for regex capture groups, not this store, and trigger sends skip interpolation entirely.\n\nExample. `#var potion yellow` followed by `quaff $potion` sends `quaff yellow` to the server. With a target set, `cast dispel $target` aims at your current mark.',
  },
  {
    id: 'automate.macros',
    number: '3.6',
    title: 'Bind keys to macros',
    section: 'Automate',
    body: "Macros bind a key to a command that fires while the input bar has focus. They live in the settings window under `macros`.\n\n- Open settings and pick the `macros` tab.\n- Click the key field in the top row. It reads `press a key...` while capturing.\n- Press the key you want. The field records the canonical key name. Capture accepts function keys, modifier combos like `Ctrl+N`, numpad keys like `Numpad7`, and plain printable keys.\n- Enter the command. `;` chains multiple actions.\n- Enter a group name to put the binding in a toggleable folder, then click `add`.\n\nRows apply on add or save, so the tab has no separate save step. Existing rows carry their own `save` and `delete` buttons. A binding fires only while the input bar has focus.\n\nTurn on `echo macro commands` in the `general` tab's `macros` row to make each press show what it sent. Group headers carry an `enabled` checkbox, and `#group <name> on|off` flips macro groups from the input bar along with matching alias and trigger groups.\n\nExample. Bind `F1` to `stand; flee` and pressing `F1` in the input bar sends both commands.\n\n`#record` builds something different. It captures the commands you type and saves them as an alias you invoke by name, not by key. Use the macros tab when you want a key, `#record` when you want a word.",
  },
  {
    id: 'automate.slash-commands',
    number: '3.7',
    title: 'Use slash commands',
    section: 'Automate',
    body: 'Slash commands drive Vosh from the input bar without opening settings. Vosh handles every line that starts with `#` locally, and it never reaches the MUD.\n\n- Type `#help` any time for the full list.\n- Manage aliases with `#alias <name> <expansion>`, `#unalias <name>`, and `#aliases`.\n- Manage variables with `#var <name> [value]`, `#unvar <name>`, and `#vars`.\n- Manage triggers with `#trigger <name> {pattern} <action>`, `#untrigger <name>`, and `#triggers`.\n- Bind prompt stats with `#prompt {regex}` using named groups like `(?<hp>\\d+)`, and clear with `#unprompt`.\n- Flip whole folders with `#group <name> on|off` and inspect them with `#groups`.\n- Tune the tick with `#tick`, `#tick interval <secs>`, `#tick warn at <secs>`, and the rest listed under `#help`.\n- Record a command sequence with `#record <name>`, finish with `#endrec`, abort with `#record cancel`.\n- Configure quick keys with `#qkey <name> <verb>` and list them with `#qkeys`.\n- Drive Lua with `#script load <name>`, `#script reload`, `#scripts`, and `#lua <code>`.\n- Snapshot with `#profile save`, `#profile load`, and `#profile reset`.\n- Import TinTin++ files with `#import-tintin <path>`.\n- Work targets with `#target <args>`, or bare `tar`, `tarn`, `tarp`, and `tarclear` with no `#` at all.\n- Switch renderers with `#nativesurface on|off|default`, applied on restart.\n\nAn unknown command echoes a pointer to `#help`, and errors come back wrapped in square brackets.',
  },
  {
    id: 'automate.lua-scripts',
    number: '3.8',
    title: 'Script Vosh with Lua',
    section: 'Automate',
    body: 'Lua scripts run inside Vosh and register automation through the global `mud` table. Script files live in the `scripts` folder under the app data directory, `~/Library/Application Support/com.aabahran.vosh/scripts/` on macOS.\n\n- Save a `.lua` file in the `scripts` folder.\n- Type `#script load <name>` to load it. Vosh appends `.lua` to a bare name.\n- Type `#scripts` to see loaded scripts and the triggers they registered.\n- After editing a file, type `#script reload` to run every loaded script again.\n- Run one liners with `#lua <code>`.\n\nScripts talk to Vosh through the global `mud` table. `mud.send(text)` goes straight to the server and `mud.input(text)` feeds back through the input pipeline. `mud.echo(text)` prints locally. `mud.alias(name, expansion)` and `mud.trigger(name, pattern, callback)` register automation, with `captures[1]` holding the full match and `captures[2]` onward the groups. `mud.on_gmcp(package, callback)` hands you server data as a table, and `mud.timer(secs, callback)` schedules work you can cancel with `mud.cancel_timer`.\n\nLoads from `#script load` last for the session. For autoload, make a plugin. Create `plugins/<slug>/` under the app data directory with a `manifest.toml` naming the plugin and its entry script, `main.lua` by default. Enabled plugin names persist in your profile TOML under `[plugins]`, and every enabled plugin loads at launch. Enabling runs immediately, disabling takes effect next launch.\n\nThe sandbox strips file and process access. `require`, `io`, and `os.execute` are gone.\n\nExample. `#script load combat` loads `combat.lua` from the scripts folder, and `#lua mud.echo("hello")` prints a line locally.',
  },
  {
    id: 'shape.arrange-panels',
    number: '4.1',
    title: 'Arrange the panels',
    section: 'Shape the window',
    body: "The `panels` tab in the settings window controls where every panel sits. Column widths resize by dragging in the main window. The eight panels are `map`, `group`, `vitals`, `roomstrip`, `chat`, `affects`, `combat`, and `imm`.\n\n- Click the gear in the top bar, titled `settings`, and open the `panels` tab.\n- Pick a home for each panel with its `zone` select. Zones are `top`, `bottom`, `left`, `right`, and `hidden`.\n- Set `align` for a left or right panel. `top` panels stack down from the column top and `bottom` panels stack up from the floor. The map hides this select because it always fills the leftover column height.\n- Reorder panels within a zone with the up and down chevrons on the layout map chips.\n- Tick `side panels span full height (input lives under terminal only)` to run the side columns to the window's bottom edge.\n- Resize a column from the main window by dragging the 8px channel between the terminal and the column. A small ember tick fades in on the handle when you hover. Tab to a handle and press an arrow key to nudge it 16px, or `Shift` plus an arrow for 64px.\n\nChanges save live. `reset to defaults` restores the stock layout.Panels render as raised cards with 8px of exposed ground at every seam, so the layout stays clean when you move one.\n\nThe command palette on `⌘K` (`Ctrl+K` off macOS) lists a `show <id> pane` or `hide <id> pane` entry for every panel. A panel you show again returns to its last visible zone.",
  },
  {
    id: 'shape.use-the-map',
    number: '4.2',
    title: 'Use the map',
    section: 'Shape the window',
    body: 'The map pane draws the server map. It fills the right column by default, and its header names the current area once room data arrives.\n\n- Move the map with its `zone` select in the `panels` tab. It accepts `left`, `right`, or `hidden` only, since a horizontal map at full width is unusable.\n- Toggle it with the map button in the top bar. The button flips the map between hidden and its last visible spot.\n- Read the header. It says `map` until the first room push arrives, then `map · <area>` with the area name from `Room.Info`.\n- Click the sliders button labeled `map controls` to open the controls row. Vosh remembers whether you left it open.\n- Switch draw modes with `squares`, `glyphs`, or `tileset`.\n- Zoom with `−` and `+`, or hold `Ctrl` and wheel over the map. The readout shows the percent and `⤺` resets it.\n- Read the status text. `radius N` means live map data. `waiting for server map` means none has arrived yet.\n\nTileset mode adds a bar with `load tileset` and `clear` buttons for your own tile art.',
  },
  {
    id: 'shape.chat-pane',
    number: '4.3',
    title: 'Use the chat pane',
    section: 'Shape the window',
    body: "The chat pane collects channel talk in its own buffer with a tab per channel. It ships hidden and its home is the bottom strip.\n\n- Show it from the palette. Press `⌘K` and run `show chat pane`.\n- Lines arrive on their own. `Comm.Channel` GMCP feeds the pane automatically. Each line renders as `[pane] text` in its channel color, with the speaker shown as `Name: `.\n- Click a tab in the header to filter. `all` sits first, then one tab per channel name seen in the buffer. The count beside them reads `visible`, or `visible/total` while filtered.\n- Route trigger output in. Add a `route` effect to a trigger in the `triggers` tab and enter a pane name. Those lines land in the chat pane under their own tab.\n- Drag the pane's handle to resize it. Click the `×` labeled `hide chat` to put it away.\n\nThe buffer holds a rolling 500 lines, survives closing and reopening the pane, and clears only on disconnect. The pane sticks to its tail. Scroll up to read back, and it sticks again once you come within 24px of the bottom.",
  },
  {
    id: 'shape.read-vitals',
    number: '4.4',
    title: 'Configure the vitals readout',
    section: 'Shape the window',
    body: "The vitals readout shows hp, mana, and moves. Configure it in the `vitals` tab in Settings, where changes save automatically and you can drag the live preview's bars to scrub the numbers.\n\n- Pick a layout. The options are `ember`, `stacked`, and `inline`. Ember draws a sidebar pane with a `vitals` head, the tick countdown beside it, and three fixed thin bars with mono current and max numbers.\n- Outside ember, choose columns with the `bar`, `percent`, `numeric`, and `delta` pills, set `bar style` to `solid`, `ramped`, or `track`, and pick bar glyphs and width.\n- Open the `advanced` disclosure to recolor hp, mana, and moves, or turn on `drain through red as bars empty`.\n- Tick `pulse red vignette under 30% hp` to pulse a red vignette when hp drops under 30%.\n- Turn on `custom template (overrides layout)` to write the readout yourself with tokens like `%hp`, `%pct_hp`, `%bar_hp`, `%tick`, and `%time`. Any `Char.Vitals` or `Char.Worth` field resolves as `%fieldname`, and `%%` prints a literal percent.\n\nPlace the bar in any zone from the `panels` tab. It ships in the right column, listed as `vitals (hp bar)`. Tracked affects also live in the `panels` tab, not here.\n\n`reset vitals` restores the stock config.",
  },
  {
    id: 'shape.group-affects',
    number: '4.5',
    title: 'Watch your group and affects',
    section: 'Shape the window',
    body: "The group pane shows member health while grouped and your worth while solo. The affects pane counts down spell durations. The group pane sits at the top of the right column, and its header shows `group` plus a member count, or `solo`.\n\n- Read the group rows. While grouped, each member gets a row with a mono name, a 44px hp mini bar, and the percent. The row tone drops through three tiers, healthy at 67% and up, warning down to 34%, danger below.\n- While solo, the pane shows your worth instead. The fields are tnl, exp, gold, bank, trains, and prac.\n- Configure affects in the `panels` tab under `tracked affects`. Enter the server's affect name, add an optional label, and press `add`. Matching is case insensitive, and the affects pane renders nothing until you list at least one name.\n- Place `affects` in a side zone for full rows, each with a mono name, a duration mini bar filled by the fraction of a day remaining, and a countdown. In the top or bottom strip it compresses to pills, and absent tracked affects render dim with `—`.\n\nGroup data arrives from `Group.Info` and worth from `Char.Worth`. Affects come from `Char.Affects`, and duration color shifts with urgency. With no group the pane says so and suggests `follow <name>` to start one.",
  },
  {
    id: 'shape.room-strip',
    number: '4.6',
    title: 'Read the room strip',
    section: 'Shape the window',
    body: 'The room strip is a one line summary of your location. It runs along the top of the window and shows the area, the room name with vnum, the terrain, and the exit list in `N E S W U D` order.\n\n- Read the `here` chips for who is in the room. Character chips color NPCs and players differently, stacks collapse to `(N) name`, and your current target carries a `▶` marker.\n- Read the `items` chips for what is on the ground. Each colors by type, so money, weapons, armor, potions, and food read apart at a glance.\n- Move the strip from the `panels` tab, where it lists as `room strip (area info)`. In a left or right column it switches to a column variant that wraps onto multiple lines instead of scrolling sideways.\n\nGMCP feeds everything. `Room.Info` names the room, `Map.Tiles` colors the area, `Room.Chars` and `Room.Items` fill the chips, and the target marker follows the backend target.\n\nIn the top strip, overflowing content fades out at the right edge. An empty slot holds its height so the layout never jumps.',
  },
  {
    id: 'shape.split-the-well',
    number: '4.7',
    title: 'Split the well',
    section: 'Shape the window',
    body: 'Splits divide the terminal well into three panes, the main session, channel chat, and a raw log tail. These are workspace panes, separate from the scrollback split.\n\n- Open them from the palette. Press `⌘K` and run `open splits`, or right click the terminal and pick `open splits` from the menu.\n- Read the pane chips. The main terminal takes `1` plus a session name drawn from the host, `theforsakenlands` on the default world, or `session` before you connect.\n- Watch channel talk in the `2 chat` pane, colored per channel with dim `HH:MM` timestamps.\n- Watch raw session output tail through the `3 log · raw` pane below it.\n- Read the status bar. While splits are open its center lists `1 session`, `2 chat`, and `3 log`, with the active pane marked.\n- Run `close splits` from the same palette entry or menu item to fold back to a single well.\n\nThe choice persists across launches. Toggling never remounts the terminal, so the session never flickers. Both side panes stick to their bottom edge. Before any traffic the chat pane reads `no channel chat yet` and the log pane reads `quiet — raw session output tails here`.',
  },
  {
    id: 'shape.imm-board',
    number: '4.8',
    title: 'Use the imm board',
    section: 'Shape the window',
    body: 'The imm panel lists staff queues sorted worst first. Its home is the right column, and it fills only on an immortal login.\n\n- Show the panel with `⌘K` and `show imm pane`, or place `imm` from the `panels` tab, where it lists as `imm (staff queues)`.\n- Log in on an immortal. The server lights the panel at login with an `Imm.Queues` push. On a mortal it reads `no staff feed`.\n- Read top down. Only queues with work appear, overdue sorts above nearing, and bigger counts rise. The queues are `dcheck`, `applications`, `journals`, `votes`, `notes`, `bugs`, `penalties`, `ideas`, and `typos`.\n- Read the chips. Rows trail `N overdue` and `N nearing`, applications add `N unread`, and journals add `N unawarded`.\n\nThe header sums the board as `N overdue`, `N nearing`, or `clear`. A count flashes when it grows. An empty board after the feed reads `all clear`.',
  },
  {
    id: 'tick.tick-timer',
    number: '5.1',
    title: 'Configure the tick timer',
    section: 'Tick and target',
    body: "The tick timer counts down to the next tick and renders a chip at the right edge of the input row. Configure it in the `tick & chips` tab in Settings, and changes apply live.\n\n- In the `timer` row, tick `enabled`. Add `sound on fire` to play a sound when the tick lands.\n- Set `interval` in seconds, anywhere from 1 to 3600.\n- Put a command in `auto-fire` to send it on every tick. Leave it blank for none.\n- Give `reset on` a regex.Give `reset on` a regex. Every line that matches resets the countdown, so the MUD's own tick message keeps the timer accurate.\n- Enable `warn` and set how many seconds of lead you want, 5 by default. Fill `warn text` and `color` to restyle the warning. The color takes an ANSI name, `#rrggbb` hex, or a 256 palette index, and blank keeps the defaults.\n- Pick a chip style under `input row chip`. `value only` is just the number, `caption + value` adds labels, and `icon + value` swaps them for compact icons.\n- Position the moons in the `status strip` section. `right edge`, `before the clock`, and `after the clock` place the Aabahran moon phases.\n\nThe ember vitals layout repeats the countdown in its pane head.",
  },
  {
    id: 'tick.track-target',
    number: '5.2',
    title: 'Track a target with quick keys',
    section: 'Tick and target',
    body: 'A set target shows in the status bar, the room strip, and the combat pane. Quick keys are name and verb pairs listed beside it in the status bar.\n\n- Read the status bar. Once a target is set, the left block shows `tar` plus the target name, then your quick keys as name and verb pairs separated by `·`.\n- Find the target in the room strip. Its character chip carries a `▶` marker.\n- Place the `combat` pane for a dedicated readout. It ships hidden, which renders it inline inside the vitals bar. Move it to a zone in the `panels` tab and it becomes a standalone pane with the target name over an hp track bar. It collapses when no target exists.\n- Park `combat` and `vitals` together in the bottom zone and combat shrinks to a chip attached to the vitals block.\n- Use a quick key by typing its name as the first word of a command. Vosh suppresses its own echo, because the backend echoes the expansion instead.\n\nQuick keys live in the running session. They reset to the stock `gg`, `xx`, `zz`, and `tt` slots on restart, so set your verbs again with `#qkey` after each launch.',
  },
  {
    id: 'make-it-yours.switch-themes',
    number: '6.1',
    title: 'Switch themes',
    section: 'Make it yours',
    body: 'Themes recolor the whole app, chrome and terminal alike. They live in the settings window under `themes`.\n\n- Click the gear button in the main window top bar to open the settings window.\n- Pick the `themes` tab in the left rail. It sits under the `appearance` group beside `typography`.\n- Browse the catalog. Cards for the stock themes come first, then any custom themes tagged `custom`, then the dashed `+ new from active` card. Each card shows a state dot, the theme name, and three swatches for the surface, accent, and warn colors.\n- Click a card. The theme activates on the spot, and the header note reads `changes save automatically`.\n\nA theme changes both layers of the app. The chrome layer covers surfaces, text, borders, the accent pair, and the semantic warn, danger, info, and success colors. The terminal layer covers background, foreground, cursor, selection, and all sixteen ANSI colors. Some themes also turn on terminal tint by default. Ember ships with its pastel ANSI on.\n\nStock themes stay locked. A custom theme shows an `×` on its card while it is inactive, and clicking it deletes the theme immediately with no confirm dialog. If the active theme ever disappears, Vosh falls back to the default theme.',
  },
  {
    id: 'make-it-yours.build-your-own-theme',
    number: '6.2',
    title: 'Create a custom theme',
    section: 'Make it yours',
    body: 'A custom theme forks an existing theme and recolors it slot by slot. The editor lives in the settings window under `themes`.\n\n- Open the settings window and pick the `themes` tab.\n- Activate the theme you want as the starting point.\n- Click the dashed `+ new from active` card at the end of the catalog. Vosh forks the active theme, names the copy after the base theme with `(custom)` appended, and switches to it.\n- Set the `label` and `description` fields in the editor that appears under the catalog.\n- Recolor the `chrome` section. Its groups are surfaces, text, borders, accent, and semantic.\n- Recolor the `terminal` section. Its groups are terminal surfaces, selection, `ANSI 0-7`, and `ANSI 8-15 (bright)`.\n- Adjust any slot with the color picker beside it, or type a value into its text field.\n\nEvery edit applies live in the main window, and Vosh saves automatically a moment after each change. The text field commits a value only when CSS can render it. It accepts rgba values, and the picker strips them to hex for its own display.\n\nThe editor renders only while the active theme is a custom one. Switching to a stock theme hides it until you activate the custom theme again.',
  },
  {
    id: 'make-it-yours.control-terminal-colors',
    number: '6.3',
    title: 'Control terminal colors',
    section: 'Make it yours',
    body: "The terminal palette, and the two accent colors Vosh draws over it, live in the settings window under `themes`.\n\n- Open the settings window and pick the `themes` tab.\n- Find the `tint` section and its `terminal tint` row.\n- Check `tint output with theme` to recolor server output with the theme's own ANSI set, or uncheck it to show the base palette. Each theme picks its own default, and Ember ships its pastel ANSI on.\n- Edit the `terminal base palette` section to change the base palette itself. Its sixteen slots are the ANSI 0 to 15 colors used while `tint output with theme` is off. Change any slot with its picker or text field. Touching one slot turns all sixteen into a custom list.\n- Click `reset to canonical` to discard the custom list and return to the canonical chart. The button shows only while the palette is custom.\n- Set `sent command color` to recolor the local echo of every command you send. The row holds a color picker, a text field, and a `clear` button.\n- Set `split scrollback divider` the same way to recolor the line between the history and live panes. Its text field placeholder reads `theme default (#rrggbb, rgba, named)`.\n\nBoth accent rows apply live the moment you commit a value, and `clear` returns either one to the theme default.",
  },
  {
    id: 'make-it-yours.pick-your-fonts',
    number: '6.4',
    title: 'Set the terminal font',
    section: 'Make it yours',
    body: 'The terminal typeface and size live in the settings window under `typography`.\n\n- Open the settings window and pick the `typography` tab.\n- In the `terminal face` section, click one of the quick chips. The chips are `BerkeleyMono`, `JetBrainsMono`, `Menlo`, `Monaco`, and `Courier New`. The first two ship inside Vosh, so they work on every machine.\n- Or type a full CSS stack into the family field, following the placeholder shape `"BerkeleyMono Bundled", Menlo, monospace`.\n- Set the size with the number input. It accepts 9 to 32 px and defaults to 14.\n- Check `bright text as bold` to render SGR bright colors 8 to 15 in the heavier cut. This applies on the native renderer only.\n- To use a font installed on your machine, click into the `system fonts` filter field. Vosh loads the installed list on first focus, and the placeholder tells you when it is ready. Keep `monospace only` checked to hide proportional faces. The list shows up to 200 families, each row drawn in its own face.\n- Click a family in the list. Vosh sets your stack to that family with Menlo and monospace as fallbacks.\n- Check the `preview` section. It renders a fixed sample line at your chosen family and size.\n\nChanges save automatically about a quarter second after you stop typing, and a `saved.` note confirms it.',
  },
  {
    id: 'characters-and-data.profiles',
    number: '7.1',
    title: 'Manage profiles',
    section: 'Characters and data',
    body: "A profile carries its own aliases, triggers, macros, and variables, and auto match rules pick the right profile when you connect. Manage profiles in Settings under `profiles`.\n\n- Click the gear button in the top bar to open Settings, then pick `profiles` under `characters` in the left rail.\n- Type a name in the create row and click `+ new`. Names take letters, digits, `-`, `_`, and spaces.\n- Click `auto-match` on a row to attach matching rules. Enter the `host`, an optional `port`, and `characters` as a list separated by commas. Any listed name matches. Click `save`.\n- Click `switch` on a row to change the active profile. Everything on this tab saves automatically.\n\nThe `scope` section decides what travels with a profile. Flip the pill from `global` to `profile` on any of `theme`, `font`, `dock layout`, `keep last command`, or `auto check updates`. Global keeps one value across every profile. Profile moves the value with the active profile, and the `font` row covers family and size as one toggle.\n\n`duplicate` copies a profile's whole setup but deliberately leaves the auto match rules behind. You cannot delete the active profile, so switch away first.\n\nExample. A profile named `aabahran-erelei` with a `characters` list of `Erelei, Akletus, Vanek` matches a login on any of those three names.\n\nFrom the input bar, `#profile save` and `#profile load` write and reload the active profile's file on demand.",
  },
  {
    id: 'characters-and-data.loadouts',
    number: '7.2',
    title: 'Set up loadouts',
    section: 'Characters and data',
    body: 'Loadouts flip whole groups of aliases, triggers, and macros on and off from one shared catalog. Loadout mode starts with a one time migration from per profile files.\n\n- Open Settings, pick `import` under `tools`, and find the `migrate between scopes` section. Click `preview migration`.\n- Review the plan. The wizard shows how your profiles would merge into a single shared catalog with one generated loadout per source profile. The preview writes nothing.\n- Apply the migration. Vosh writes the catalog and parks your old per profile files in `profiles/legacy/`. Loadout mode waits for the next launch, so click `quit Vosh` in the wizard and reopen the app. The profile you were on becomes the sole active loadout.\n- Reopen Settings and pick the `loadouts` tab, which now appears under `characters`. Check the boxes for the loadouts you want live. The runtime enables the union of their groups across every active loadout.\n\nClick `deactivate all` to park the catalog dormant. Dormant disables every grouped alias, trigger, and macro, and it survives restarts and profile switches. Items without a group always stay live.\n\nWhen no active loadout declares any enabled groups, the loadouts impose nothing and your durable checkbox state from the automation tabs stands.\n\nActivation is the only edit the `loadouts` tab makes. Author or reshape loadouts by editing `loadouts.toml` in the app data folder, or run the migration wizard again.',
  },
  {
    id: 'characters-and-data.tintin-import',
    number: '7.3',
    title: 'Import a TinTin++ file',
    section: 'Characters and data',
    body: 'The `#import-tintin` command reads aliases and variables out of a TinTin++ `.tin` file and loads them into the live profile. It runs from the input bar.\n\n- Type `#import-tintin <path>` and point it at the `.tin` file. `~` expands in the path.\n- Read the echo. It prints `imported <path>` and a count line like `12 aliases, 4 vars`.\n- Check the `skipped (unsupported)` line. It tallies directives Vosh does not model by name, so you can port them by hand.\n- Check the `unparsed` count. It flags alias or variable lines the parser could not read.\n\nThe importer handles `#alias {name} {expansion}` and `#variable {name} {value}`, with `#var` accepted as a short form. Nested braces and escaped braces inside the values parse correctly. The importer silently skips `#nop` lines and comments starting with `;`. Imported aliases overwrite existing aliases with the same name. Variables land at profile scope, so they persist with the profile.\n\nExample. `#import-tintin ~/aabahran.tin` imports the file from your home folder, and a skip line of `event=2 ticker=1` reports two `event` directives and one `ticker` directive left behind.\n\nFiles from other clients go through Settings instead. Open the `import` tab, pick or paste a MUSHclient, Mudlet, GMUD, or `CMUD / zMUD` export, leave the format on `auto-detect`, and hit `apply`. The summary lists counts plus anything rejected, unsupported, or unparsed.',
  },
  {
    id: 'characters-and-data.search-logs',
    number: '7.4',
    title: 'Search session logs',
    section: 'Characters and data',
    body: 'Vosh logs every session automatically and searches the store with regular expressions. The search lives in Settings under `logs`.\n\n- Open Settings and pick `logs` under `tools` in the left rail.\n- Pick a scope in the sessions list on the left. `all sessions` searches everything, and clicking one session narrows the search to it. Each row shows the host, port, date, and line count.\n- Type a pattern in the search box and click `search`. Patterns are regular expressions.\n- Tick `case` for case sensitive matching. Tick `show all` to lift the 500 result cap and return every match.\n\nEach hit shows its timestamp, the host and port when you search across sessions, and the line in its original colors when the raw bytes are on record. Changing the session scope or either checkbox reruns the current search on its own. A new pattern needs another click on `search`.\n\nExample. The pattern `dragon|wyvern` finds lines containing either word.\n\nThe `copy` button on a session row copies that whole session to your clipboard as plain text. There is no file download yet. The store is `logs.sqlite` in the app data folder and it fills on every connection, so logging needs no setup.',
  },
  {
    id: 'characters-and-data.stay-updated',
    number: '7.5',
    title: 'Check for updates',
    section: 'Characters and data',
    body: "Vosh checks for new builds and installs them in place. The controls live in Settings on the `general` tab.\n\n- Open Settings. The `general` tab opens by default. Scroll to the `app` section and find the `updates` row.\n- Click `check now`. The status reads `checking…`, then `up to date` when nothing is newer.\n- When a build is available, click the `install v<version> + restart` button that appears. The status shows `installing…`, then Vosh relaunches on the new build.\n- Tick `check on launch` to run the check at every start. It is off by default. With it on, a banner appears in the main window when an update is waiting.\n\nUpdates download from the project's GitHub releases, and Vosh checks every build's signature before installing.\n\n`auto check updates` is one of the five scope rows on the `profiles` tab. It defaults to global, so one setting covers every profile. Flip it to profile when one character should check on launch while the others stay quiet.",
  },
  {
    id: 'fix-it.terminal-renderer',
    number: '8.1',
    title: 'Switch terminal renderers',
    section: 'Fix it',
    body: 'Vosh ships two terminal renderers. The native GPU surface is the default on macOS and the xterm renderer is the default on Windows and Linux. The `#nativesurface` command switches between them from the input bar.\n\n- Type `#nativesurface off` to force the xterm renderer everywhere.\n- Type `#nativesurface on` to force the native surface everywhere.\n- Type `#nativesurface default` to return to the platform default.\n- Restart Vosh. The switch applies only on restart, and the echo reminds you with `restart Vosh to apply`.\n\nThe command runs entirely in the frontend and stores your choice locally under the key `vosh.nativesurface`. A bad argument echoes `usage #nativesurface on | off | default (takes effect on restart)`.\n\nIf the text renders in the wrong typeface, open Settings and pick the `typography` tab. The default family stack starts with `BerkeleyMono Nerd Font` and falls through `JetBrains Mono`, `Fira Code`, `Menlo`, and `Consolas` before generic monospace. Your machine renders the first family in that stack it has installed, so install the font you want or move it to the front. Font size defaults to 14.\n\nFonts follow profile scope. In Settings under `profiles`, the `font` row covers family and size as one toggle. Set it to `global` for one look everywhere or `profile` to let each profile carry its own.\n\nOn the xterm renderer the right click menu offers `clear buffer`. The native surface hides that item because its grid has no clear command.',
  },
  {
    id: 'fix-it.reconnect',
    number: '8.2',
    title: 'Recover a bad connection',
    section: 'Fix it',
    body: 'The session chip in the top bar holds the connection controls. Its status dot shows idle, connecting, connected, or error, and while live the chip also shows your character name and `host:port`.\n\n- Click the chip. A dropdown form opens with `host`, `port`, a `tls` checkbox, and a submit button that reads `connect` or `disconnect` depending on state.\n- Press `disconnect` and wait for the dot to go idle.\n- Confirm the address. The defaults are host `play.theforsakenlands.com` and port `1848` with `tls` off.\n- Press `connect`.\n\nThe `tls` checkbox wraps the connection in TLS. Match it to what the server offers on that port. The default port `1848` expects it off.\n\nDisconnecting has side effects. Session scoped variables clear when the next connection opens, so anything set with `#var` never carries into the new session, while profile variables survive. The chat pane buffer clears at disconnect. On reconnect, Vosh matches the host and port against your profiles and switches to the best match automatically, and it picks up a character pinned profile after login.\n\nTwo other paths reach the same controls. Right click the terminal and pick `disconnect`, the item that appears at the bottom of the menu only while connected. Or press `Cmd+K` on macOS or `Ctrl+K` elsewhere and run the palette entry `connect` or `disconnect`.',
  },
  {
    id: 'fix-it.data-on-disk',
    number: '8.3',
    title: 'Find your data on disk',
    section: 'Fix it',
    body: 'Vosh keeps all of its data in one app data folder named `com.aabahran.vosh`.\n\n- On macOS, open `~/Library/Application Support/com.aabahran.vosh`.\n- On Linux, open `~/.local/share/com.aabahran.vosh`.\n- On Windows, open `%APPDATA%\\com.aabahran.vosh`.\n\nInside that folder.\n\n- `profiles.toml` indexes your profiles and names the active one.\n- `profiles/<name>.toml` holds each profile snapshot with connection defaults, aliases, variables, triggers, tick config, and macros.\n- `global.toml` holds cross profile UI preferences.\n- `catalog.toml` and `loadouts.toml` appear once loadout mode is active.\n- `logs.sqlite` stores session logs, with `-wal` and `-shm` sidecars alongside.\n- `scrollback.txt` persists the last 10,000 terminal lines across restarts.\n- `maps.sqlite` stores map data.\n- `scripts/` holds Lua files for `#script load`.\n- `plugins/` holds plugin folders, each with a `manifest.toml`.\n\nEvery TOML save is safe by design. Vosh renames the old file to `<file>.bak.<timestamp>` with a millisecond timestamp, writes a temp file, swaps it in atomically, and keeps the ten newest backups. To roll back a bad profile edit, copy the backup you want over the live file.\n\nA leftover `profile.toml` at the root is the legacy single profile file. Vosh migrates it to `profiles/default.toml` on the first multi profile launch.',
  },
  {
    id: 'reference.slash-commands',
    number: '9.1',
    title: 'Slash commands',
    section: 'Reference',
    body: 'This is every slash command Vosh understands today.\n\n- `#help` prints the command summary.\n- `#alias <name> <expansion>` defines, `#unalias <name>` removes, `#aliases` lists.\n- `#var <name> [value]` sets or shows a session variable, `#unvar <name>` removes it from both scopes, `#vars` lists.\n- `#trigger <name> {pattern} <action> [args]` defines, `#untrigger <name>` removes, `#triggers` lists by priority.\n- `#prompt {regex}` binds named captures to prompt vars, `#unprompt` removes it.\n- `#group <name> on|off` toggles a group, `#group <name>` shows state, `#groups` lists.\n- `#tick`, `#tick interval <secs>`, `#tick reset`, `#tick on {pattern}`, `#tick off`, `#tick fire <command>`, `#tick nofire`, `#tick sound on|off`, `#tick disable`, `#tick enable` drive the tick timer.\n- `#tick warn`, `#tick warn at <secs>`, `#tick warn message <text>`, `#tick warn color <name>`, `#tick warn off` shape the tick warning.\n- `#script load <name>` loads a Lua file, `#script reload` reruns loaded scripts, `#scripts` lists them.\n- `#lua <code>` evaluates Lua inline.\n- `#profile save`, `#profile load`, `#profile reset` manage the profile snapshot. In loadout mode all three become notices.\n- `#import-tintin <path>` imports TinTin++ aliases and variables.\n- `#record <name>` starts recording, `#record` shows status, `#record cancel` discards, `#endrec` saves the recording as an alias.\n- `#qkey <name> <verb>` configures a quick key, `#qkey clear <name>` clears, `#qkeys` lists.\n- `#target <args>` mirrors `tar`, with `#target clear|next|prev`, `#tarn`, `#tarp`, `#tarclear` as slash forms.\n- `#nativesurface on|off|default` forces the renderer, applied on restart.\n\nTargeting also works bare with no `#`. Type `tar` to list, `tar <N>` or `tar <substr>` to pick, `tarn` and `tarp` to cycle, `tarclear` to clear.\n\nAn unknown command points you at `#help`. Errors echo wrapped in square brackets.',
  },
  {
    id: 'reference.keyboard-shortcuts',
    number: '9.2',
    title: 'Keyboard shortcuts',
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
