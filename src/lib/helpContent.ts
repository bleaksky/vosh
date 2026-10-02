// In-client help content. Source of truth for both the Help window
// (src/HelpApp.tsx) and the standalone HELP.md document at the repo
// root. Keep them in sync; the markdown file mirrors this catalog
// one-to-one. Every topic is a task walkthrough in the project
// writing style. Bodies use the lightweight format the Help window
// parses:
//   - Paragraphs separated by a blank line (\n\n).
//   - Lines starting with "- " render as bullet list items.
//   - A block whose lines all start with "|" renders as a table, its
//     first row the head and its row of dashes left out.
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

/** One row of the prompt design codes: the codes as you write them, and
 *  what they do in one sentence (section 7.1 of the prompt build spec). */
export interface PromptDesignCode {
  codes: string[];
  text: string;
}

/** The codes a prompt design is written in, for the Reference topic. */
export const PROMPT_DESIGN_CODES: readonly PromptDesignCode[] = [
  { codes: ['%hp', '%mana', '%move'], text: 'Your current Health, Mana or Moves.' },
  { codes: ['%maxhp', '%maxmana', '%maxmove'], text: 'The most you can have.' },
  { codes: ['%pct_hp'], text: 'Health as a percent with no sign. Add %% for the sign.' },
  {
    codes: ['%{hp:pct:game}'],
    text: 'Health as a percent with no sign, rounded down as the game does, so 37.5 reads 37.',
  },
  { codes: ['%hp_bar:10:auto'], text: 'A bar ten cells wide, colored by how full it is.' },
  { codes: ['%{gold:grouped}'], text: 'Any value from the picker, in any of its forms.' },
  { codes: ['%{gold:thousands}'], text: 'Gold in thousands with one decimal, as 12.3K.' },
  {
    codes: ['%{hour:ampm}'],
    text: 'The game hour as 3PM, with 12AM for midnight and 12PM for noon.',
  },
  { codes: ['%{tick:since}'], text: 'The seconds since the last tick, as 16s.' },
  { codes: ['%c_green', '%c_hp'], text: "A theme color, or Health's color by how full it is." },
  {
    codes: ['%{c:hp:steps}'],
    text: 'Colors by how full Health is in eleven steps from red to green, one for each tenth.',
  },
  {
    codes: ['%{c:#80c8ff}', '%{c:128,200,255}'],
    text: 'Any color you choose, as hex or as red, green and blue.',
  },
  {
    codes: ['%bg_blue', '%{bg:#3b4252}'],
    text: 'The ground behind the text, in any form a text color takes.',
  },
  { codes: ['%c_default'], text: 'Back to the terminal text color. Bold and italic stay on.' },
  { codes: ['%c_reset'], text: 'Back to plain text with every color and style off.' },
  {
    codes: ['%s_italic', '%s_bold', '%s_underline', '%s_off'],
    text: 'Turns a style on, or every style off.',
  },
  {
    codes: ['%s_strike', '%s_dim', '%s_inverse'],
    text: 'Strikes the text through, dims it, or swaps its color and ground.',
  },
  { codes: ['%s_blink'], text: 'Makes the text blink.' },
  {
    codes: ['%s_double', '%s_curly', '%s_dotted', '%s_dashed'],
    text: 'Underlines with two lines, a wave, dots or dashes.',
  },
  {
    codes: ['%{ul:#bf616a}', '%{ul:default}'],
    text: 'Colors the underline, or gives it the text color again.',
  },

  { codes: ['%nl'], text: 'Starts a new line.' },
  { codes: ['%{right}'], text: 'Pushes the rest of its line to the right edge of the terminal.' },
  {
    codes: ['%{if:fight}', '%{ifnot:fight}', '%{end}'],
    text: 'Shows what sits between them only in a fight, or only out of one.',
  },
  { codes: ['%{raw}'], text: 'Your prompt exactly as the game sent it.' },
  { codes: ['%%'], text: 'A percent sign.' },
];

/** The codes as a table in the help body format, each column padded to
 *  its widest cell as Prettier sets the same table in HELP.md. */
function promptCodesTable(): string {
  const rows = [
    ['Code', 'What it does'],
    ...PROMPT_DESIGN_CODES.map((row) => [row.codes.map((c) => `\`${c}\``).join(' '), row.text]),
  ];
  const widths = [0, 1].map((i) => Math.max(...rows.map((row) => row[i].length)));
  const line = (cells: string[]) => `| ${cells.map((c, i) => c.padEnd(widths[i])).join(' | ')} |`;
  const [head, ...body] = rows;
  return [line(head), line(widths.map((w) => '-'.repeat(w))), ...body.map(line)].join('\n');
}

export const HELP_TOPICS: HelpTopic[] = [
  {
    id: 'get-connected.connect',
    number: '1.1',
    title: 'Connect to a world',
    section: 'Get connected',
    body: 'You connect from the session button, centered in the title band over the terminal. While you are not connected it reads `Not connected` beside a status dot.\n\n- Click the session button and choose the `Connect to` row, or press `Cmd+R` on macOS or `Ctrl+R` elsewhere. Vosh dials the saved world, `play.theforsakenlands.com` on port `1848` until you save another.\n- To play somewhere else, choose `New connection…` instead. Fill in `Host` and `Port`, turn on `Use TLS` when your server offers TLS, and click `Connect`. Vosh saves that world as the one Connect dials from then on.\n- Watch the dot shift from connecting to connected.\n- Type your character name at the login prompt and press `Enter`.\n- When the server asks for a password, the command line swaps to a masked field with the placeholder `password`. Nothing you type shows on screen, echoes to the terminal, lands in command history, or reaches the session log. Vosh sends it exactly as typed, with no aliases, variables, or `#` commands applied. Press `Enter` to submit. `Shift+Enter` submits here too instead of adding a line.\n\nWhile connected, the button shows your character name and the world. If a saved profile matches the host and port you dialed, Vosh switches to that profile before connecting. A profile set to log in as your character takes over right after login, when the server reports who you are.\n\nTo disconnect, click the session button and choose `Disconnect`.',
  },
  {
    id: 'get-connected.reconnect',
    number: '1.2',
    title: 'Reconnect',
    section: 'Get connected',
    body: 'The session button reports the connection through its status dot. The dot turns to its error state when the connection fails, and the reason shows in the terminal in square brackets and when you point at the button. It goes back to idle when the session closes cleanly.\n\n- Click the session button and choose the `Connect to` row, or press `Cmd+R` on macOS or `Ctrl+R` elsewhere, to dial the same world again.\n- Scroll up or press `PageUp` to read output from before the drop. The terminal scrollback survives a disconnect, and nothing clears it unless you choose `Clear scrollback` yourself.\n- To stage commands while offline, type the first command, press `Shift+Enter` to stack more lines under it, and leave the block in the command line. After you reconnect, press `Enter` once and each line submits separately, in order.\n\nTwo things reset on a disconnect. The chat pane buffer empties the moment the session drops, and session variables set with `#var` clear when the next connection opens, so they never carry into a new session. Aliases, triggers, macros, and profile variables stay loaded because they live in your profile, not in the connection.\n\n`Disconnect` lives in three places. The session button while connected, the Session menu in the macOS menu bar, and the `Cmd+K` palette.',
  },
  {
    id: 'get-connected.profile-save',
    number: '1.3',
    title: 'Save your profile',
    section: 'Get connected',
    body: '`#profile save` writes the current client state to the active profile file, and the profile loads again on startup with no extra step. The file is a TOML snapshot under `~/Library/Application Support/com.aabahran.vosh`.\n\n- Set up the client state you want to keep. Aliases, triggers, macros, variables, and tick settings all count.\n- Type `#profile save` in the command line. Vosh writes the snapshot to the active profile TOML.\n- Or choose `Save profile` in the Session menu of the macOS menu bar or in the `Cmd+K` palette. It sends the same command.\n\nThe snapshot covers connection defaults, aliases, profile variables, triggers, tick configuration, macros, ui settings, enabled plugins, and the groups you disabled.\n\nVariables set with `#var` live in session scope. They clear when the next connection opens and never reach the file. A lasting value belongs in the `profile_vars` table of your profile file at `profiles/<name>.toml` in the app data folder. Edit it there while Vosh is closed, or set the value from Lua with `mud.set_profile_var`.\n\n`#profile load` pulls the saved file back into the live session. In loadout mode the profile commands become notices instead, because loadout mode saves your changes automatically.',
  },
  {
    id: 'play.send-commands',
    number: '2.1',
    title: 'Send commands',
    section: 'Play',
    body: 'The command input sends lines to the server. It handles single commands, chained commands, multi line blocks, and pastes.\n\n- Type a command and press `Enter` to send it.\n- Chain commands on one line with `;`. Each piece goes out as its own command. Type `\\;` for a literal semicolon.\n- Press `Shift+Enter` to add a line without sending. The box grows and a line number gutter appears once it holds two or more lines. Press `Enter` and every line submits separately, in order, with blank lines dropped.\n- Press `Enter` on an empty box to send a bare line. Many MUD prompts advance on that. It echoes as a grey `›` on its own line, or as a blank line with `Mark your commands` off, so you see each one go out.\n- Paste multi line text straight into the input. A single line submits immediately. Two or more lines become a paste burst, sent one line every 500 ms by default, with a `paste N/M esc cancels` counter in the command line.\n- Press `Esc` during a burst to cancel every line that has not gone out yet. Starting a new paste also cancels the old burst.\n\nWith `Keep last command` on under Input, then Command line, in Settings, a sent command stays in the box fully selected. Press `Enter` again to resend it, or start typing to replace it.\n\nEach command you send echoes in the text after a grey `›`, so your commands stand apart from the lines the game sends. Turn off `Mark your commands` under Input, then Command line, in Settings, to echo them bare. A quick key and a macro echo the same way.\n\nSet the delay in `Wait between pasted lines` under Input, then Advanced, in Settings, anywhere from 0 to 10000 ms.',
  },
  {
    id: 'play.reuse-history',
    number: '2.2',
    title: 'Recall command history',
    section: 'Play',
    body: 'Command history records every line you send during a session and replays it from the command line.\n\n- Press `ArrowUp` in an empty input to step back through sent commands, newest first.\n- Type a few characters before pressing `ArrowUp` to turn recall into a prefix search. Only lines starting with that text cycle past.\n- Press `ArrowDown` to step toward newer matches. One step past the newest restores exactly what you had typed before the search began.\n- Edit the recalled line at any point. Editing ends the search, and the next `ArrowUp` starts a fresh one from whatever is now in the box.\n\nHistory skips consecutive duplicates and never records anything you type in password mode.\n\nWith `Keep last command` on, the line you just sent stays in the box fully selected. `Enter` resends it and typing anything replaces it.\n\nIn a multi line compose the arrows do their normal job first. `ArrowUp` moves the caret up a line unless you are already on the first line, and `ArrowDown` moves it down unless you are on the last, so history recall fires only from the edges of the block.\n\nExample. Type `tell` and press `ArrowUp` to cycle through only the lines that start with `tell`.',
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
    body: 'Scrollback opens in a split above the live terminal, so old output stays readable while new output keeps flowing underneath.\n\n- Scroll the mouse wheel up over the terminal. The first notch opens the split with history above and the live tail below, and further scrolling walks the history line by line.\n- Or press `PageUp` to open the split and page upward, then `PageDown` to page back down. A Mac keyboard produces these with `Fn+Up` and `Fn+Down`.\n- Or press `Cmd+\\` on macOS or `Ctrl+\\` elsewhere to open the split, and press it again to close it. The View menu and the palette list it as `Split terminal`.\n- Read the `↑ N / max` count at the top right of the history to see how far back you are.\n- Drag the divider between the history and the live tail to resize the split. Its color lives in Settings under Layout, then Split terminal.\n- Return to live three ways. Scroll or page down until history reaches its bottom and the split closes itself. Press `Esc`. Or middle click the terminal.\n\nThe live tail never scrolls away while the split is open. New output keeps landing there, and the lines you type show in the history too, so the record stays continuous.\n\nWith the xterm renderer the divider snaps to whole terminal rows. It also answers the keyboard, arrow keys nudge it 16px and `Shift` with an arrow jumps 64px. The native renderer splits its own grid, and a middle click at the live tail opens the split a page up.',
  },
  {
    id: 'play.find-text',
    number: '2.5',
    title: 'Find text',
    section: 'Play',
    body: 'The find bar searches the whole session scrollback. It floats over the top right of the terminal.\n\n- Press `Cmd+F` on macOS or `Ctrl+F` elsewhere. The find bar opens even while you are typing in the command line, and pressing it again puts the caret back in its field.\n- Type your query into the `Find in scrollback` field.\n- Press `Enter` for the next match and `Shift+Enter` for the previous one. The up and down arrow buttons do the same jobs.\n- Read the count beside the field. It shows `2 of 6` while you step through matches and `No matches` when the query finds nothing.\n- Narrow the query with the three toggles. `Match case` makes it case sensitive, `Whole word` matches whole words only, and `Regular expression` treats the query as a regular expression.\n- Press `Esc` or the close button to close the bar, clear every highlight, and return focus to the command line.\n\nWith the xterm renderer, a match above the visible screen opens the scrollback split with the match near the top of the history. A match already on screen closes any open split instead. The native renderer scrolls its own grid to each match.\n\nThe find bar also opens from `Find in scrollback…` in the terminal right click menu, the Edit menu, and the `Cmd+K` palette.',
  },
  {
    id: 'play.copy-text',
    number: '2.6',
    title: 'Copy terminal text',
    section: 'Play',
    body: 'Terminal text copies to the system clipboard through a drag selection.\n\n- Drag across the output you want. An active text selection stops the usual click from refocusing the command line, so the selection stays put.\n- Press `Cmd+C` on macOS or `Ctrl+C` elsewhere.\n- Or right click the terminal and choose `Copy`. The menu shows its shortcut beside it.\n- To copy everything, press `Cmd+A` on macOS or `Ctrl+A` elsewhere while the command line is empty, or choose `Select all` in the right click menu. It selects the whole terminal, scrollback included, and `Cmd+C` then copies it.\n\nOne priority rule. When the command line itself holds a selection, `Cmd+C` copies that selection rather than the terminal. Clear its selection, or choose `Copy` in the right click menu, when the terminal text is what you want.\n\n`Paste` in the right click menu inserts the clipboard into the command line without sending anything. Edit the line as needed, then press `Enter` yourself.\n\nWith the xterm renderer the right click menu also offers `Clear scrollback`, which wipes the terminal buffer.',
  },
  {
    id: 'play.palette',
    number: '2.7',
    title: 'Use the command palette',
    section: 'Play',
    body: 'The command palette runs Vosh commands from the keyboard. It covers the View and Session commands, the Settings pages, your prompt, and your aliases.\n\n- Press `Cmd+K` on macOS or `Ctrl+K` elsewhere, or click the search button at the right end of the title band. The same shortcut closes it again.\n- With nothing typed it lists the last few commands you ran under Recent, then View and Session.\n- Type a few letters to search every command. Entries whose title starts with your text rank first, then titles that hold it anywhere, then the other words each entry answers to.\n- Move the selection with the arrow keys and press `Enter` to run the highlighted entry. A row with a list behind it, like `Choose theme`, opens the list on `Enter` or `ArrowRight`, and `ArrowLeft` or `Backspace` steps back out.\n- Press `Esc` to step out of a list, or to close the palette without running anything.\n\nThe palette sorts what it finds into four sections.\n\n- Input. `Customize prompt…`, `Draw your prompt`, and `Edit prompt as text…`.\n- View. `Show panel`, `Split terminal`, `Choose theme`, a row for each pane like `Show map`, the rows that pick where your prompt shows, `Reset panel layout`, `Find in scrollback…`, `Open help`, `Open settings`, and a row for each Settings page, like `Open trigger settings`.\n- Aliases. Every alias that is on. One that takes no arguments runs the moment you pick it. One that takes arguments puts its name in the command line instead, so you finish the line and press `Enter`.\n- Session. `Save profile`, then the `Connect to` row or `Disconnect`. Disconnect sits last, and the palette never opens with it selected.',
  },
  {
    id: 'play.right-click-menu',
    number: '2.8',
    title: 'Use the right click menu',
    section: 'Play',
    body: "The terminal right click menu collects the terminal's everyday actions in one place.\n\n- Right click anywhere on the terminal to open it.\n- `Customize prompt…` opens Customize prompt over your prompt, where you design how Vosh draws it.\n- `Copy` copies the current selection, and `Paste` inserts the clipboard into the command line. Nothing sends until you press `Enter` yourself.\n- `Select all` selects the whole terminal, scrollback included.\n- `Find in scrollback…` opens the find bar.\n- `Settings` opens a list beside the menu. `Triggers`, `Aliases`, `Macros`, and `Timers` open Settings under Automation on that list. `General`, `Appearance`, `Layout`, `Input`, `Automation`, and `Characters` open that page of Settings. `Help` opens the Help window.\n- `Clear scrollback` wipes the terminal. The item appears only with the xterm renderer, since the native grid has no clear command.\n\nItems with a shortcut show it on the right, and `Settings` shows an arrow. The arrow keys move through the menu and `Enter` picks an item. `ArrowRight` or `Enter` on `Settings` opens its list on the first row, and `ArrowLeft` steps back out. Pointing at `Settings` opens the list too. `Esc` closes the list first, then the menu. The menu also closes on a click anywhere outside it, or the instant you pick an item. It keeps itself inside the window, so a right click near a corner never opens it half off screen. Near the right edge the Settings list opens on the left of the menu, and near the bottom it rises from its row.",
  },
  {
    id: 'automate.first-alias',
    number: '3.1',
    title: 'Create an alias',
    section: 'Automate',
    body: 'Aliases expand a short name into one or more commands. They live in Settings under Automation, then Aliases, and the command line defines them too.\n\n- Open Settings, choose Automation, and pick `Aliases` in the switcher at the top.\n- Click `New alias` in the bar at the bottom.\n- Enter a name in `Name`.\n- Enter the expansion in `Expansion`. `;` splits the expansion into separate commands and `\\;` keeps a literal semicolon.\n- Click `Save`. The bar shows `Saved`.\n\nCaptures pull words from the line you typed. `%1` through `%9` pull the first through ninth word after the alias name. `%0` pulls the whole tail, `%1-` pulls word one through the end with spacing intact, and a missing word expands to nothing. `%%` gives a literal percent.\n\nGive related aliases a shared name in `Group` to turn them on and off together with `#group <name> on|off`. Under `Advanced`, `Run Lua instead` runs a Lua script in place of the expansion, with the words you typed in its captures table.\n\nExample. An alias named `kk` with the expansion `kick %1; backstab %1` turns `kk dragon` into `kick dragon` followed by `backstab dragon`.\n\nThe command line defines aliases too. `#alias gc get all corpse` sets one and echoes `alias gc set`, `#aliases` lists every alias, and `#unalias gc` removes one. Setting an alias again, with `#alias`, `#endrec`, or `mud.alias` in Lua, keeps it in its group.',
  },
  {
    id: 'automate.first-trigger',
    number: '3.2',
    title: 'Create a trigger',
    section: 'Automate',
    body: 'Triggers watch incoming lines and run actions when a pattern matches. They live in Settings under Automation, then Triggers, and a trigger pairs one visual with any number of effects.\n\n- Open Settings and choose Automation, then Triggers.\n- Click `New trigger`.\n- Enter a name and a pattern. Patterns are regexes, so escape literal punctuation. Under `Advanced`, `Add pattern` in More patterns adds another, and the trigger fires when any pattern that is on matches.\n- Leave `Priority` under `Advanced` at `5`, the default for a new trigger, or raise it to run before other triggers. Higher priority triggers run first. Leave `Match` on `Lines`.\n- Pick `Room` in `Match` to match only the things and people a room lists after its exits line. The game sends a `Room.Chars` packet with each look, and Vosh counts the people lines from it, so a say or an arrival after the look stays a plain line.\n- Pick a `Style`. The choices are `None`, `Highlight`, `Wash`, `Replace`, and `Hide`.\n- Put a command in `Then send`. `Send to pane` and `Lua script` sit under `Advanced`. Send and replace templates reach capture groups with `$1` through `$9` or `${name}`, and `;` splits a send into separate commands.\n- Click `Save`. Vosh shows `Saved` in the bar at the bottom.\n\nExample. The pattern `(\\w+) is DEAD!` with a send of `get all corpse` loots each kill as the death line arrives.\n\nThe command line builds triggers too. `#trigger name {pattern} send command` creates one at priority 0 on the `line` target, `#triggers` lists everything by priority, and `#untrigger name` removes one. Vosh rejects an invalid regex and names the broken pattern.',
  },
  {
    id: 'automate.highlight-lines',
    number: '3.3',
    title: 'Highlight lines',
    section: 'Automate',
    body: 'A highlight trigger restyles every line that matches a pattern. Define one from the command line with `#trigger` or in Settings under Automation, then Triggers.\n\n- Type `#trigger <name> {pattern} highlight <color> [styles]`. Every line matching the pattern renders in that color and style.\n- Add `wash` to the style list to tint the whole line instead of restyling the text alone.\n- Type `#triggers` to confirm the pattern and action. Defining a trigger under an existing name replaces it.\n\nA plain highlight restyles the matched words, and the rest of the line keeps the colors the game sent. A wash marks the whole line. The line text takes the highlight color, a dim field in that color fills the row edge to edge, and an accent bar marks the left edge. The field and the bar follow your theme palette, so a washed line sits with the colors around it instead of fighting them.\n\nColors take the sixteen ANSI names. `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, and `white`, plus a `bright_` variant of each. `purple` maps to magenta and `gray` to `bright_black`. Stack `bold`, `underline`, and `inverse` freely, and add `bg:<color>` for a background.\n\nExample. `#trigger tell-glow {tells you} highlight bright_yellow bold` renders every tell bright yellow and bold. `#trigger tell-glow {tells you} highlight bright_yellow wash` replaces it with a full line wash.\n\nThe Triggers editor under Automation in Settings offers the same options. Pick `Highlight` or `Wash` in `Style`, then open `Advanced` to set `Text color` and `Background`, with `Bold`, `Underline`, and `Inverse` beside them.\n\nThe `Room, time and weather colors` preset colors a room look, the clock and the weather. The exits line turns green, the things and people the room lists turn yellow, the day and night messages turn blue, and the WiZNET tag turns bold magenta. Each one is a terminal color from your theme, so a theme switch carries them along. A change in the weather, such as `It starts to rain.` or `A thick fog rolls in, shrouding the area.`, turns pale blue. That blue is `#8fa7d9`, a color of its own that stays apart from the blue and cyan of your theme. It holds on every built in dark theme, and `Keep highlight colors readable` darkens it on a light theme until it reads. The exits and room colors fill only the text the game left uncolored, so an aura, a red `[AFK]` and the red `+` of a trap you see keep their own colors. The magenta covers the WiZNET tag alone, so the message after it keeps its colors too. A say or a tell that quotes the same words stays as it was. Vosh turns the preset on for every profile, once, unless you had turned every preset off. Turn it off in Settings under Automation, then Presets.',
  },
  {
    id: 'automate.route-chat',
    number: '3.4',
    title: 'Route lines to a pane',
    section: 'Automate',
    body: "A route effect sends matching lines to a named pane. Build one on a trigger in Settings under Automation, then Triggers.\n\n- Open Settings, choose Automation, then Triggers, and click `New trigger`.\n- Enter a name.\n- Enter a pattern. Under `Advanced`, `Add pattern` in More patterns adds another. The trigger fires when any pattern that is on matches.\n- Under `Advanced`, enter the pane's name in `Send to pane`, like `chat`.\n- Click `Save`.\n\nRoute is an effect, so it stacks with anything else on the trigger. Pair it with the `Highlight` style to color the line, or put a command in `Then send` beside it.\n\nMatching lines land in the Chat pane, tagged with the pane name you gave. Show the Chat pane with `Add a pane` in the title band or `Show chat` in the View menu, and pick that name in its channel select to read those lines alone.\n\nExample. A trigger named `chat-feed` with the patterns `tells you '` and `gossips '` and a route to `chat` collects tells and gossip in the chat pane.\n\nThe inline form is `#trigger chat-feed {tells you '} route chat`. It creates a single pattern trigger, so build multi pattern feeds in Settings under Automation.",
  },
  {
    id: 'automate.variables',
    number: '3.5',
    title: 'Set and use variables',
    section: 'Automate',
    body: 'Variables store values you reference in commands as `$name`. Set them from the command line with `#var`, and Vosh expands them in the lines you type before they leave.\n\n- Type `#var <name> <value>` to set a variable. Vosh echoes `var <name> set`.\n- Reference it in any command as `$name`. The line expands before it leaves, so the server receives the value.\n- Type `#var <name>` to check a value, `#vars` to list them all, and `#unvar <name>` to remove one.\n\n`$name` works when the name ends at whitespace or punctuation. Wrap the name in braces, as `${name}`, when letters follow immediately. `$$` sends a literal dollar sign, and unknown names pass through untouched, so `$100` reaches the server as typed.\n\nInterpolation runs on the line you type, before alias expansion, and Vosh does not interpolate alias output again. Put variables in the line you type, or resolve them in a Lua script body instead.\n\n`#var` writes session scope, which clears when the next connection opens, so a session value never survives into a new session. Profile variables persist across restarts in your profile TOML under `profile_vars`, and a session value shadows a profile value of the same name.\n\nVosh also fills session variables on its own. GMCP binds `hp`, `maxhp`, `char_name`, `room_name`, `target_name`, and more, and setting a target with `tar` mirrors it into `$target`.\n\nTrigger send templates use `${name}` for regex capture groups, not this store, and trigger sends skip interpolation entirely.\n\nExample. `#var potion yellow` followed by `quaff $potion` sends `quaff yellow` to the server. With a target set, `cast dispel $target` aims at your current mark.',
  },
  {
    id: 'automate.macros',
    number: '3.6',
    title: 'Bind keys to macros',
    section: 'Automate',
    body: 'Macros bind a key to a command that fires while the command line has focus. They live in Settings under Automation, then Macros.\n\n- Open Settings, choose Automation, and pick `Macros` in the switcher at the top.\n- Click `New macro` in the bar at the bottom.\n- Click the `Key` field. It reads `Press a key` until you press one.\n- Press the key you want. The field records its canonical name. Capture accepts function keys, modifier combos like `Ctrl+N`, numpad keys like `Numpad7`, and plain printable keys.\n- Enter the command in `Command`. `;` chains several commands.\n- Put a name in `Group` to turn the macro on and off with others, then click `Save`.\n\nA macro fires only while the command line has focus. On macOS the `Cmd` shortcuts belong to Vosh and `Ctrl` belongs to your macros.\n\nTurn on `Show the commands your macros send` under Input, then Command line, to make each press show what it sent. `#group <name> on|off` turns a whole group of macros on and off from the command line, along with matching alias and trigger groups.\n\nExample. Bind `F1` to `stand; flee` and pressing `F1` in the command line sends both commands.\n\n`#record` builds something different. It captures the commands you type and saves them as an alias you invoke by name, not by key. Use Automation, then Macros when you want a key, `#record` when you want a word.',
  },
  {
    id: 'automate.slash-commands',
    number: '3.7',
    title: 'Use slash commands',
    section: 'Automate',
    body: "Slash commands drive Vosh from the command line without opening Settings. Vosh handles every line that starts with `#` locally, and it never reaches the MUD.\n\n- Type `#help` any time for the full list, or `#help <words>` to open Help on those words.\n- Manage aliases with `#alias <name> <expansion>`, `#unalias <name>`, and `#aliases`.\n- Manage variables with `#var <name> [value]`, `#unvar <name>`, and `#vars`.\n- Manage triggers with `#trigger <name> {pattern} <action>`, `#untrigger <name>`, and `#triggers`.\n- Tell Vosh how to read your prompt with `#prompt game {setting}` and `#prompt fight {setting}`, the codes you type in the game, or with `#prompt {regex}`, each named group like `(?<hp>\\d+)` a value. `#prompt` alone says how Vosh reads it, and `#unprompt` stops.\n- Turn drawing your design on or off with `#prompt draw on|off`. With drawing off you see the game's own prompt, and your design stays.\n- Pick where your prompt shows with `#prompt show text|lifted|pinned`.\n- Use Vosh's default prompt design with `#prompt default`. It takes the place of the design in this profile, and Vosh keeps yours as an earlier design.\n- Flip whole folders with `#group <name> on|off` and inspect them with `#groups`.\n- Tune the tick with `#tick`, `#tick interval <secs>`, `#tick warn at <secs>`, and the rest listed under `#help`.\n- Record a command sequence with `#record <name>`, finish with `#endrec`, abort with `#record cancel`.\n- Configure quick keys with `#qkey <name> <verb>` and list them with `#qkeys`.\n- Drive Lua with `#script load <name>`, `#script reload`, `#scripts`, and `#lua <code>`.\n- Snapshot with `#profile save`, `#profile load`, and `#profile reset`.\n- Import TinTin++ files with `#import-tintin <path>`.\n- Clear old passwords out of your session log with `#logs forget-passwords`, then `#logs forget-passwords now`.\n- Work targets with `#target <args>`, or bare `tar`, `tarn`, `tarp`, and `tarclear` with no `#` at all.\n- Switch renderers with `#nativesurface on|off|default`, applied on restart.\n\nAn unknown command echoes a pointer to `#help`, and errors come back wrapped in square brackets.",
  },
  {
    id: 'automate.lua-scripts',
    number: '3.8',
    title: 'Script Vosh with Lua',
    section: 'Automate',
    body: 'Lua scripts run inside Vosh and register automation through the global `mud` table. Script files live in the `scripts` folder under the app data directory, `~/Library/Application Support/com.aabahran.vosh/scripts/` on macOS.\n\n- Save a `.lua` file in the `scripts` folder.\n- Type `#script load <name>` to load it. Vosh appends `.lua` to a bare name.\n- Type `#scripts` to see loaded scripts and the triggers they registered.\n- After editing a file, type `#script reload` to run every loaded script again.\n- Run one liners with `#lua <code>`.\n\nScripts talk to Vosh through the global `mud` table. `mud.send(text)` goes straight to the server and `mud.input(text)` feeds back through the input pipeline. `mud.echo(text)` prints locally. `mud.alias(name, expansion)` and `mud.trigger(name, pattern, callback)` register automation, with `captures[1]` holding the full match and `captures[2]` onward the groups. `mud.on_gmcp(package, callback)` hands you server data as a table, and `mud.timer(secs, callback)` schedules work you can cancel with `mud.cancel_timer`.\n\nLoads from `#script load` last for the session. For autoload, make a plugin. Create `plugins/<slug>/` under the app data directory with a `manifest.toml` naming the plugin and its entry script, `main.lua` by default. To turn a plugin on, add its name to `enabled` under `[plugins]` in your profile file while Vosh is closed, like `enabled = ["vitals_alert"]`. Every plugin on that list loads at launch, and removing a name turns that plugin off from the next launch.\n\nThe sandbox strips file and process access. `require`, `io`, and `os.execute` are gone.\n\nExample. `#script load combat` loads `combat.lua` from the scripts folder, and `#lua mud.echo("hello")` prints a line locally.',
  },
  {
    id: 'shape.arrange-panels',
    number: '4.1',
    title: 'Arrange the panels',
    section: 'Shape the window',
    body: "The panel on the right holds your panes, the map over your affects at first, with your vitals pinned at its foot. You arrange it in the window itself, and Vosh keeps the arrangement for each character.\n\n- Show or hide the panel with the panel button at the right end of the title band, with `Cmd+Shift+L` on macOS or `Ctrl+Shift+L` elsewhere, or with `Show panel` in the View menu or the palette. While it is hidden your vitals move to the status line.\n- Add a pane with `Add a pane`, the plus button in the title band. It lists the panes the panel does not show yet, and the one you pick lands at the bottom. The panes are Map, Affects, Group, Chat, and Staff queues, which joins the list once the game sends it.\n- Open a pane's menu with the more button in its header. `Split right` and `Split down` put the first pane the panel does not show beside or under it. `Show here instead` swaps in another pane, and `Close pane` takes it out. Closing a pane loses nothing.\n- Drag the line between two panes to share the space between them. Tab to a line and the arrow keys move it 8 points, or 32 with `Shift`.\n- Drag the panel's left edge to change its width, from 200 to 800 points, and double click the edge to go back to 300. Tab to the edge and the arrow keys move it 8 points. Settings has the same `Width` under Layout, then Panel.\n- Show or hide one pane with its row in the View menu or the palette, like `Show map`.\n\nTo start over, choose `Reset panel layout` in the View menu or the palette, or `Reset to default` under Characters, then Panel layout, in Settings. The panes go back to the map over your affects, and the panel keeps its width and whether it shows.\n\nSettings under Characters draws each character's panel under Panel layout, so you can see how each one is arranged.",
  },
  {
    id: 'shape.use-the-map',
    number: '4.2',
    title: 'Use the map',
    section: 'Shape the window',
    body: 'The Map pane draws the map the game sends. It sits at the top of the panel at first, and its header names the area you are in.\n\n- Show or hide it with `Show map` in the View menu or the palette, or add it with `Add a pane` in the title band.\n- Read the rows under the drawing. The first names the room you stand in and its exits. The rest list the people here, with a count beside a name more than one of them shares, and when more people are here than fit, the last row counts the others. A short pane gives up rows of people before the room.\n- Point at the drawing and click the sliders button in its bottom right corner to open the map menu.\n- Pick `Squares`, `Glyphs`, or `Tileset` to change how the map draws.\n- Choose `Zoom in` or `Zoom out`, or hold `Cmd` or `Ctrl` and turn the wheel over the map. `Actual size` shows the zoom and goes back to 100%.\n- In Tileset, choose `Load tileset…` to use your own tile art and `Clear tileset` to drop it.\n\nVosh remembers the style, the zoom, and the tileset. Until the game sends `Map.Tiles` the pane says the map appears when your MUD sends it. `Room.Info` names the room, its exits, and the area, and `Room.Chars` lists the people.',
  },
  {
    id: 'shape.chat-pane',
    number: '4.3',
    title: 'Use the chat pane',
    section: 'Shape the window',
    body: "The chat pane collects channel talk in its own buffer, one line per message. Add it with `Add a pane` in the title band, or pick `Show here instead` in any pane's menu.\n\n- Lines arrive on their own. `Comm.Channel` GMCP feeds the pane automatically.\n- Read a line as `[tell] Tolliver: meet at the bank`. The tag names the channel and the speaker is bold, even a name of several words like `a Blackwatch villager`. Wrapped lines hang two cells in, so the tags run down the left edge.\n- Each line takes the color the game prints that channel in, from your theme's terminal colors. Say is bright yellow, tell green, gtell bright magenta, yell cyan, pray bright white, cabal bright blue, clan bright cyan, faction yellow, newbie bright green, immortal bright red, and imp bright cyan. Switch themes and the chat follows. A color too faint to read on the pane goes lighter or darker, with its hue kept, until it reads clearly. The terminal still shows the theme's own color.\n- Recolor a channel from the pane's menu. Choose `Channel colors`, then the channel, then `Default` or one of your theme's 16 terminal colors. The pane follows at once, each profile keeps its own picks, and a theme switch carries them along. `Reset all` gives every channel its default again.\n- Point at a message to see when it arrived.\n- Filter with the channel select beside the pane's name. `All` shows every channel. Each chat pane keeps its own filter, so you can split one off for tells alone.\n- Route trigger output in. On a trigger under Automation, then Triggers, put a name in `Send to pane` under `Advanced`. Those lines land in the chat pane under that name, in their own words.\n- See the tells you send. The game sends no GMCP for them, so the `Tells you send` preset routes the line the game prints for each one. Vosh turns it on for every profile, once, unless you had turned every preset off. Each one reads `[tell] to Tolliver: text`, the tells a telepath projects too. The pane skips the `You tell your group` line, because your gtell already arrives over GMCP. Turn the preset off in Settings under Automation, then Presets.\n\nThe buffer holds a rolling 500 lines, survives closing and reopening the pane, and clears only on disconnect. The pane sticks to its tail. Scroll up to read back, and it sticks again once you come within 24px of the bottom.",
  },
  {
    id: 'shape.read-vitals',
    number: '4.4',
    title: 'Configure the vitals readout',
    section: 'Shape the window',
    body: "The vitals sit at the bottom of the panel, under the panes. Health, Mana, and Moves each show the value with a thin meter under it. The meters stay quiet until a vital runs low. Under 20% its value and meter turn red, and they stay red until it climbs back to 25%. In a fight your opponent gets a row on top with its health.\n\n- Open Settings and choose Layout.\n- Under Vitals, set `Density` to `Rows` for one row per vital, or to `One line` to fit Health, Mana, and Moves on a single row.\n- Set `Values` to `Current and max` to read `186 / 1020`, to `Current` to read `186`, or to `Percent` to read `18%`.\n- Set `Meter` to `Line` for the thin meter, to `Bar` for a thicker one you can read at a glance in a fight, or to `None` to keep only the numbers on tighter rows.\n- Turn on `Warn before you run low` and a vital turns yellow under two thirds and red under one third, the way the Group pane shows your group's health.\n- Leave `Hide vitals while your prompt is pinned` on and the panel drops its vitals while `Where your prompt shows` is `Pinned`, so the panes take their room. Turn it off to keep them, or pick another place for your prompt, and they come back at once. They also come back while you have prompts off in the game, since the band then has no prompt to show.\n\nEach default draws the panel you already know, so nothing changes until you pick something. One line drops the Health, Mana, and Moves labels only when they no longer fit beside the values, under about 360 pt with four digit health, and keeps the values and meters. `Current` and `Percent` keep the labels even on a narrow panel. A panel too narrow for even the values stacks them in rows.\n\nTurn off `Show the panel` under Layout and your vitals move to the status line. There they follow `Values` and `Warn before you run low` but never draw a meter. When the target you set is the one you are fighting, its health follows its name in yellow.\n\nWhen the game hides your vitals, as it does under lamented tears, every value reads `?` in dim text over an empty meter, in the panel and on the status line alike. Nothing turns yellow or red while they stay hidden. Your numbers come back with the next update the game sends. In a fight the opponent row reads `?` the same way when the game hides its health, and the status line drops the health of your target.",
  },
  {
    id: 'shape.group-affects',
    number: '4.5',
    title: 'Watch your group and affects',
    section: 'Shape the window',
    body: "The Group pane shows the health of everyone in your group. The Affects pane shows what affects you and the hours each has left. Add either one with `Add a pane` in the title band.\n\n- Read the Group rows. Each member gets a row with the name, a `lead` tag on the leader, a thin health meter, and the percent. The meter and the percent stay quiet at 67% and up, turn yellow down to 34%, and turn red below that. The header counts the members.\n- Pick the affects you track in Settings under Characters, then Tracked affects. Choose `Add affect…` and pick one of the affects on you now, or type its name. Matching ignores case and extra spaces. Under Advanced you can give a tracked affect a short label like `sanc` to show in its place, and set the order of your slots.\n- Pick how the affects pane draws in Settings under Layout, then Affects, or from the pane's own menu under `Style` and `Marker`. Each character keeps its own. `Timers first` is the default. `Countdown` lists every affect by the hours it has left. `Grouped chips` puts what to recast first.\n- In `Timers first`, read the affects pane in two columns. Each entry shows the hours left, then the name exactly as the game sends it. `+` means permanent and `-` means you do not have it, the same marks the game uses in its own affects bar. A pane narrower than about 360 pt shows one column.\n- Your tracked affects fill the top rows in your order and keep their places as the hours change. The dot beside each agrees with its hours. Green is up, yellow has two hours left, and red has one hour or none. A hollow red ring and a red name mean you are missing it.\n- Everything else sits under a thin line. Harmful affects like `faerie fire` come first, with a red diamond and a red name. The rest follow by hours left, down the left column and then down the right.\n- In `Countdown`, the tracked affects you are missing come first, then every affect by the hours left, down the left column and then down the right. A thin meter under each drains as its hours run down.\n- In `Grouped chips`, `Recast` holds the tracked affects you are missing and the ones running out, `Tracked` the rest you track, and `Other` everything else. A missing affect is a dashed red chip. A tracked chip is filled, and the fill drains from the left as its hours run down. One running out turns yellow at two hours and red at one or none. Other affects sit in outlined chips, and harmful ones in red.\n- The meters and the fills measure each affect against the most hours Vosh has seen for it since you last cast it, and Vosh remembers that for each character between logins. An affect Vosh first sees partway through starts full, and a permanent one stays full.\n- `Marker` sets the mark beside each affect you track in `Timers first` and `Countdown`. Pick a dot, a square, plus and minus, or none. Plus and minus shows a plus while you have the affect and a minus while you miss it. The color shows the state in every shape, and with none the hours and the red names still do.\n- Turn on `Tint what to recast` to put a missing affect on a red wash and one about to drop on yellow or red, in `Timers first` and `Countdown`. `Grouped chips` always marks what to recast.\n- The hours follow the game. One hour or none reads in bold red, and two in yellow. The header counts the tracked affects you are missing and the ones running out.\n- The pane shows whole rows only. When some do not fit, the last entry says how many more there are, like `5 more`. Click it to scroll to them, and point away to scroll back.\n\nGroup data arrives from `Group.Info`. Affects come from `Char.Affects`, which the game sends when you log in, whenever an affect changes, and every tick. With no group the pane says your group appears when you join one.\n\nWhen the game hides your affects or your group, as it does under lamented tears, the pane says so in place of its rows. The affects pane marks no tracked affect missing, and the group pane shows no member health from before. Each fills in again with the next update the game sends.",
  },
  {
    id: 'shape.imm-board',
    number: '4.6',
    title: 'Watch the staff queues',
    section: 'Shape the window',
    body: 'The Staff queues pane lists the staff queues that need you, worst first. The game sends them to immortals alone.\n\n- Log in on an immortal. The game sends `Imm.Queues` at login, and `Show staff queues` joins the View menu, the palette, and `Add a pane`.\n- Add the pane with `Add a pane` in the title band, or choose `Show staff queues`. Until the queues arrive it says they appear when you log in as an immortal.\n- Read top down. Only queues with work show. Anything past its deadline sorts first, then anything in the last quarter before it, then the rest, the bigger backlog first. The queues are Description checks, Applications, Journals, Votes, Notes, Bugs, Penalties, Ideas, and Typos.\n- Read the asides. A row says how many are overdue or nearing their deadline, Applications adds how many are unread, and Journals adds how many are unawarded. Point at a row to read what it counts and its deadline.\n\nThe header sums the overdue items, or else the nearing ones. With nothing waiting the pane says no staff queue needs you right now.',
  },
  {
    id: 'shape.prompt-show',
    number: '4.7',
    title: 'Choose where your prompt shows',
    section: 'Shape the window',
    body: 'Once Vosh reads your prompt, you choose where it shows. Open Settings, choose Input, and pick a place under `Where your prompt shows` in the Prompt section.\n\n- `In the text` shows each prompt where the game sends it. The terminal reads as it always has.\n- `Lifted` keeps every prompt in the text on a raised band in the selected row color of your theme, scrollback included. A prompt that ends on a character gains one space after its band, so your echo never touches it.\n- `Pinned` takes your prompts out of the text and shows your latest one on a band above the command line. The band is only as tall as your prompt. When a fight adds a row, the text above gives up its top line to make room and gets it back when the fight ends, so one blank line always sits between your newest line and the band, as the game leaves one before each prompt. Every prompt still reaches the session log and your Prompts triggers. While your prompt is pinned, the panel hides its vitals and gives their room to the panes. Turn off `Hide vitals while your prompt is pinned` under Layout, then Vitals, to keep them.\n\nFrom the command line, `#prompt show lifted` picks the same place, and `text` or `pinned` in its place picks the others. `#prompt` alone also says where your prompt shows.\n\nThe choice needs Vosh to read your prompt. Until it does, the row stays off and says what to do first, and `#prompt show` tells you to type `#prompt game` with your prompt setting in braces. While you have prompts off in the game, the pinned band says so and shows nothing else.\n\nWith the xterm renderer, the newest 1000 prompts keep their bands and older ones show plain. The split history pane shows them plain too. The native renderer keeps a band on every prompt in the scrollback.\n\nThe choice saves in the `[prompt]` table of your profile as `show`. An older version of Vosh ignores it and shows your prompt in the text. When that version saves your profile, the choice is gone, so pick it again here.',
  },
  {
    id: 'tick.tick-timer',
    number: '5.1',
    title: 'Configure the tick timer',
    section: 'Tick and target',
    body: "The tick timer shows the game's tick in the status line under the command line, with the game time and the moons beside it. The game's own tick decides when it fires. Vosh knows the game ticked when the game hour moves, which Aabahran advances once a tick, or when a line matches your `Reset on` pattern. When the tick lands, the count restarts, the sound plays, and your `Send each tick` command goes out, once per tick. Configure it in Settings under Automation, then Timers, where `Tick` sits at the top of the list, and click `Save` to apply your changes.\n\n- Turn on `Enabled`. Every connection starts the tick, and switching characters keeps it running until you turn it off. `Play a sound` under `Advanced` plays a sound when the tick lands.\n- Set `Every` in seconds, anywhere from 1 to 3600. It is how long you expect a tick to take. Aabahran picks each tick between 25 and 35 seconds, so once the game ticks, the timer waits for the game instead of firing at `Every`.\n- Put a command in `Send each tick` to send it on every tick. Leave it blank for none.\n- Give `Reset on` a regex. A line that matches is the tick. A signal within 2 seconds of a tick counts as that tick, so a matching line and the game hour moving together fire once.\n- Before the game's first tick in a session, and on a game that never tells Vosh when it ticks, the timer fires on its own every `Every` seconds. When the game goes quiet for twice `Every`, the timer fires once on its own and keeps its own time until the next tick comes in.\n- The tick in the status line turns the warn color on a soft ground in its last 5 seconds, and stays that way while the game runs late. Turn on `Warn before it fires` to also print a warning line in the terminal, and set `Warn at` to how many seconds of lead you want. The status line then follows the same lead. The terminal prints the warning once per tick. Fill `Warning text` and `Warning color` to restyle the warning line the terminal prints. The color takes an ANSI name, `#rrggbb` hex, or a 256 palette index, and blank keeps the defaults.\n- While the game runs late, the tick pulses gently in the warn color until the tick lands. With Reduce motion on in your system settings, it holds still in the warn color.\n- Pick which way the tick counts in Settings under Layout, then Status line, in the Tick counts row. `Up` shows the seconds since the last tick and keeps counting past `Every` while the game runs late, like `31s`. `Down` shows the seconds left until the tick, from `Every` right after one down to `1s` in its last second, and waits at `0s` when the game runs late. `Down past 0` counts down the same way and keeps counting below zero until the tick lands. An early tick restarts either count at once.\n- Pick how the status line shows the tick, the time, and the moons in the Tick and time row just above it. `Value` shows each value alone, like `14s` and `8:42`. `Caption` puts Tick, Time, and Moons before them. `Icon` puts a ring before the tick. Counting up it fills clockwise as the seconds pass, closes when the tick is due, and stays closed while the game runs late. Counting down it shows the time left and empties clockwise toward the top, and only the faint ring shows while the game runs late. Before the time it draws the sun on its path over the horizon. The sun rises on the left, stands highest at midday, and sets on the right, and after dark it drops under the horizon as an open dot.\n\nThe game time takes a tint from your theme for the part of the day. Each moon in the sky shows as a small icon of its phase in its own color. Each moon takes the color the game gives its name from your theme, so Lysenties draws in the theme's bright white, Nercuros in its bright cyan, and Dyphrities in its red. On a light theme the icons are ink on paper, like a printed calendar, with the dark part filled in, so a new moon is a solid disc and a full moon an open ring. Hover a moon to read its name and phase, like `Nercuros, nearly full and still growing`. During an eclipse, the triad, or a near alignment, one word in the warn color follows the moons. A dormant moon stays hidden, and the moons leave the line while you are not connected.",
  },
  {
    id: 'tick.track-target',
    number: '5.2',
    title: 'Track a target with quick keys',
    section: 'Tick and target',
    body: 'Set a target with `tar` and Vosh keeps it in the status line. Quick keys pair a short name with a verb, so typing the name acts on your target.\n\n- Type `tar` to list the people in the room, and `tar 2` or `tar drag` to pick one by number or by part of the name. `tarn` and `tarp` step to the next or previous person, and `tarclear` clears the target.\n- Read the status line under the command line. Once a target is set it shows `Target` and the name. While you fight that target with the panel hidden, its health follows the name in yellow.\n- Read the vitals at the foot of the panel. In a fight your opponent gets a row on top with its health.\n- Set a quick key with `#qkey <name> <verb>`, like `#qkey gg backstab`. Then type `gg` as the first word of a command and Vosh sends `backstab` and your target. Vosh skips its own echo, because the backend echoes the expansion instead.\n- Type `#qkeys` to list them and `#qkey clear <name>` to clear one.\n\nSetting a target with `tar` also fills `$target`, so `cast dispel $target` aims at your current mark. Quick keys live in the running session. They reset to the stock `gg`, `xx`, `zz`, and `tt` slots on restart, so set your verbs again with `#qkey` after each launch.',
  },
  {
    id: 'make-it-yours.switch-themes',
    number: '6.1',
    title: 'Switch themes',
    section: 'Make it yours',
    body: "Themes recolor the whole window, the terminal included. They live in Settings under Appearance, in the Theme section.\n\n- Open Settings and choose Appearance.\n- Click a theme in the gallery. Each one draws in its own colors with its name under it, and your own themes follow the built in ones. The theme applies at once and saves. The arrow keys move the pick too.\n- Turn on `Follow system appearance` to switch between the `Light theme` and the `Dark theme` you pick under it whenever your system does.\n- Click `Import…` to read a Ghostty, iTerm2, Kitty, or Alacritty theme file. Vosh adds it to your own themes and switches to it.\n- Or choose `Choose theme` in the View menu or the palette, which lists every theme.\n\nA theme sets both layers of the window. The window layer covers the grounds, the text, the separators, the accent, and the warn, danger, and success colors. The terminal layer covers the background, the text, the cursor, the selection, and all sixteen ANSI colors. MUD text takes the theme's sixteen colors while `Use the theme's colors for MUD text` is on under Terminal text, which it is for every theme until you turn it off.\n\nObsidian Ember is the default theme. The built in themes stay as they are, so start a custom theme from one to change it. If the theme you use ever disappears, Vosh falls back to the default theme.",
  },
  {
    id: 'make-it-yours.build-your-own-theme',
    number: '6.2',
    title: 'Create a custom theme',
    section: 'Make it yours',
    body: 'A custom theme starts as a copy of the theme you see and changes any of its colors. The editor lives in Settings under Appearance, then Advanced.\n\n- Open Settings, choose Appearance, and switch to the theme you want to start from.\n- Open `Advanced` at the bottom of the page and click `New custom theme`. Vosh copies the theme you see, names the copy after it with `copy` at the end, and switches to it.\n- Pick the theme you are changing in `Theme to edit`, and set its `Name` and `Description`.\n- Change its colors in four groups. Accent and status holds Accent, Danger, Warning, and Success. Terminal holds the background, the text, the cursor, the text under the cursor, the selection, and the selected text. Normal colors and Bright colors hold the sixteen ANSI colors.\n- Press a swatch to pick a color, or type one into the field beside it.\n\nEvery change applies at once and saves. Your custom themes join the gallery after the built in ones. To remove one, pick it in `Theme to edit`, click `Delete…`, and confirm.',
  },
  {
    id: 'make-it-yours.control-terminal-colors',
    number: '6.3',
    title: 'Control terminal colors',
    section: 'Make it yours',
    body: "The colors MUD text draws in live in Settings under Appearance.\n\n- Open Settings and choose Appearance.\n- Under Terminal text, turn on `Use the theme's colors for MUD text` to draw what the game sends in the theme's own sixteen colors, or turn it off to keep the exact colors your MUD sends. It is on for every theme until you turn it off.\n- Leave `Keep highlight colors readable` on under Terminal text, and Vosh darkens or lightens a color your triggers set when the theme would make it faint. It is on until you turn it off.\n- Open `Advanced` and change `Base palette`, the sixteen colors MUD text uses while the theme's colors are off. Change any color with its swatch or by typing a hex color. The first change keeps all sixteen as your own list.\n- Click `Reset` beside Base palette to go back to the stock chart. It stays off until you change a color.\n\nKeep highlight colors readable covers the exact colors a trigger or a preset paints text in, such as `{#8fa7d9}` or `{fg:244}` in `Replace with`. Vosh measures each one against the terminal background. When one reads too faint, Vosh keeps its hue and moves it darker on a light theme or lighter on a dark one until it reads. A color that already reads stays as you picked it, and your trigger keeps the color you saved. The game's own colors and the theme's sixteen colors never change. A theme switch reaches the lines that arrive after it, and earlier lines keep the color they were drawn in.\n\nTwo more colors sit with the rows they belong to. `Sent command color` under Input, then Command line, recolors the local echo of every command you send, and the `›` before it stays grey. `Divider color` under Layout, then Split terminal, recolors the line between the history and the live tail. Press the swatch to pick a color or type a hex color like `#fffc41`. Each applies live, and emptying the field returns it to the theme default.",
  },
  {
    id: 'make-it-yours.pick-your-fonts',
    number: '6.4',
    title: 'Set the terminal font',
    section: 'Make it yours',
    body: 'The terminal font lives in Settings under Appearance, then Terminal text.\n\n- Open Settings and choose Appearance.\n- Pick a font in `Font`. JetBrains Mono ships inside Vosh, so it works on every machine. The rest of the list holds the monospace fonts installed on your computer. If you own Berkeley Mono, install it and pick it there.\n- Pick a size in `Size`, from 11 to 18 pt. The default is 14.\n- Pick `Compact`, `Default`, or `Loose` in `Line height`.\n- To set a whole list of fonts, open `Advanced` and type it in `Font stack`, like `"Fira Code", "JetBrainsMono Bundled", monospace`. Vosh uses the first font in the list that you have.\n- Turn on `Bright text in bold` under Advanced to draw bright colors in the bold weight of your font. It works on macOS.\n- Turn off `Blinking text` under Advanced to keep text that your MUD or your prompt sets to blink still. It starts off when your system reduces motion.\n\nEach change applies at once and saves. Under General, `Font and size` in Keep the same for every character decides whether every character shares one font.',
  },
  {
    id: 'characters-and-data.profiles',
    number: '7.1',
    title: 'Manage profiles',
    section: 'Characters and data',
    body: "A profile carries its own aliases, triggers, macros, and variables, its tracked affects, and its panes. Vosh picks the right profile when you connect and again when you log in. Manage profiles in Settings under Characters.\n\n- Open Settings and choose Characters. Your profiles list on the left, with a dot on the one in use and each one's world beside it.\n- Click `New profile` under the list, type a name, and press `Enter`. Names take letters, numbers, spaces, hyphens, and underscores.\n- Select a profile to edit it. Selecting one never switches the session you are playing.\n- Pick its `World`, then turn on `Use this profile when you log in`. The row names your character once Vosh has seen you log in. Turning it on takes that character from any other profile on the same world, and Vosh says so under the list.\n- Open a profile's more menu to `Switch to this profile`, or to choose `Rename…`, `Duplicate…`, `Export to Downloads`, or `Delete…`.\n\n`Duplicate…` copies a profile's whole setup but leaves its world and login behind. You cannot delete the profile in use, so switch away first.\n\nSome settings can stay the same for every character. Under General, Keep the same for every character holds `Theme`, `Font and size`, `Keep last command`, and `Check for updates`. With a switch on, every character shares one value. Turn it off and each character keeps its own.\n\nFrom the command line, `#profile save` and `#profile load` write and reload the active profile's file on demand.\n\nEach profile reads your prompt on its own. Vosh moves the capture trigger that `#prompt` made into each profile that draws your own prompt, turns the trigger off, and tells you once at launch. Profiles that draw nothing then show the game's prompt. On The Forsaken Lands the moved pattern switches to your prompt codes the first time the game shows them, when you log in or when you type `prompt`. From then on Vosh follows each prompt you set in the game and keeps your design and the draw switch as they are. When a color code runs into a code in that prompt, or when the pattern fills a value under a name no prompt code fills, such as `health`, the pattern stays and `#prompt` says why. A pattern you set with `#prompt {regex}` never switches. An older version of Vosh shows the game's prompt in every profile until you turn `prompt-capture` on again under Automation. Back in this version, Vosh moves the capture into your profiles again and turns the trigger off.",
  },
  {
    id: 'characters-and-data.loadouts',
    number: '7.2',
    title: 'Set up loadouts',
    section: 'Characters and data',
    body: 'Loadouts flip whole groups of aliases, triggers, and macros on and off from one shared catalog. Loadout mode starts with a one time migration from per profile files.\n\n- Open Settings, choose Automation, click `Import…`, and find the `Shared catalog` section. Click `Preview…`.\n- Review the plan. The wizard shows how your profiles would merge into a single shared catalog with one generated loadout per source profile. The preview writes nothing.\n- Apply the migration. Vosh copies each profile file to `profiles/legacy/`, writes the catalog and the loadouts, and takes the aliases, triggers, and macros out of each profile file. Every other setting stays with its profile except the presets. Loadout mode keeps one list of presets that are on, and every character shares it. The list starts with every preset that any profile file had on, and the preview names each character that gains or loses a preset. Loadout mode waits for the next launch, so click `quit Vosh` in the wizard and reopen the app. Every loadout starts off, so each profile keeps on the items it had on, at launch and when you switch.\n- Reopen Settings and choose Automation, then Loadouts, which now appears after Presets. Turn on the loadouts you want live and click `Save`. The runtime enables the union of their groups across every active loadout.\n\nThe catalog keeps your folder names where it can. Each alias, trigger, and macro lands in a group that is on for exactly the profiles that had it on, so a folder two characters filled differently can become more than one group. `combat` holds what most characters kept in their combat folder, `combat (Healer)` holds the combat items only the Healer had, and `(Healer)` holds the items the Healer had outside any folder. Each profile file remembers which groups its folders became, so `#group combat on` and `#group combat off` still turn on and off exactly what that profile had in its combat folder.\n\nA trigger two characters had in different versions keeps each version, and the second one takes a name that adds its characters, such as `greet (Healer)`. An alias or a macro keeps the one version you pick in the wizard, since its name is what you type or press. Triggers keep the order each character had them in, since every trigger that matches a line fires in that order. Where two characters had the same triggers in different orders, one of them gets its own copy of a trigger, named the same way.\n\nClick `Turn all off`, then `Save`, to park the catalog dormant. Dormant disables every grouped alias, trigger, and macro, and it survives restarts and profile switches. Items without a group always stay live.\n\nWhen no active loadout declares any enabled groups, the loadouts impose nothing and each group stays on or off as you left it.\n\nActivation is the only edit Loadouts makes. Author or reshape loadouts by editing `loadouts.toml` in the app data folder while Vosh is closed. The migration wizard runs once. It will not build a new catalog while `catalog.toml` or `loadouts.toml` sits in the app data folder, while `profiles/legacy/` holds the copies from an earlier run, or while an earlier run waits to finish at the next launch.\n\nAfter the move, `catalog.toml` holds your aliases, triggers, and macros, with every one you add or change later. Each file in `profiles/legacy/` is a backup of its profile as it was before the move. Never copy a backup back while `catalog.toml` sits in the app data folder. Vosh would lay the old aliases, triggers, and macros of the backup over the catalog, turn on for that character items that only other characters had, and at the next save put the old versions in the catalog for every character. To keep your items, leave `catalog.toml` where it is and change them in Automation settings. To build a new catalog from the backups, quit Vosh first, since Vosh saves `catalog.toml` again as it quits. Then move `catalog.toml` and `loadouts.toml` out of the app data folder, copy the files in `profiles/legacy/` back over the ones in `profiles/`, and move `profiles/legacy/` out too. Each profile comes back as it was before the move, every setting included, and loses every change you made to its settings since. Your items as they are now stay in the `catalog.toml` you moved out. Then open Vosh again and run the wizard.',
  },
  {
    id: 'characters-and-data.tintin-import',
    number: '7.3',
    title: 'Import a TinTin++ file',
    section: 'Characters and data',
    body: 'The `#import-tintin` command reads aliases and variables out of a TinTin++ `.tin` file and loads them into the live profile. It runs from the command line.\n\n- Type `#import-tintin <path>` and point it at the `.tin` file. `~` expands in the path.\n- Read the echo. It prints `imported <path>` and a count line like `12 aliases, 4 vars`.\n- Check the `skipped (unsupported)` line. It tallies directives Vosh does not model by name, so you can port them by hand.\n- Check the `unparsed` count. It flags alias or variable lines the parser could not read.\n\nThe importer handles `#alias {name} {expansion}` and `#variable {name} {value}`, with `#var` accepted as a short form. Nested braces and escaped braces inside the values parse correctly. The importer silently skips `#nop` lines and comments starting with `;`. Imported aliases overwrite existing aliases with the same name. Variables land at profile scope, so they persist with the profile.\n\nExample. `#import-tintin ~/aabahran.tin` imports the file from your home folder, and a skip line of `event=2 ticker=1` reports two `event` directives and one `ticker` directive left behind.\n\nFiles from other clients go through Settings instead. Choose Automation and click `Import…`. Choose a MUSHclient, Mudlet, GMUD, or `CMUD or zMUD` export with `Choose file…`, or paste it into `Contents`. Leave `Format` on `Detect automatically` and click `Import`. The summary lists counts plus anything rejected, not supported, or unreadable.',
  },
  {
    id: 'characters-and-data.search-logs',
    number: '7.4',
    title: 'Search session logs',
    section: 'Characters and data',
    body: 'Vosh logs every session automatically and searches the store with regular expressions. The search lives in Settings under General, then Session logs.\n\n- Open Settings, choose General, and click `Search logs…` in the Session logs section. The row counts your saved sessions and lines.\n- Type a pattern in the search field. Patterns are regular expressions, and the view searches as you type.\n- Click `Aa` for case sensitive matching.\n- Pick a session in the menu at the right to search only that one. `All sessions` searches everything.\n\nThe view shows the newest 500 matches under day headings, oldest first, so it reads like the terminal. The count beside the pattern reads like `Newest 500 of 2,423 lines`, and earlier matches load as you scroll up. Each line keeps its original colors, and your matches are marked the way the find bar marks them. With no pattern the view shows the newest lines. Click `General` in the breadcrumb to go back.\n\nExample. The pattern `dragon|wyvern` finds lines containing either word.\n\nWith one session picked, the copy button beside the menu copies that whole session to your clipboard as plain text. Sessions to `127.0.0.1` and `localhost` stay out of the view and the counts. The store is `logs.sqlite` in the app data folder and it fills on every connection, so logging needs no setup.\n\nThe log keeps what the game sent and each line you sent, marked `> `. Lines you type at a password prompt are not saved. Each one shows as `> (hidden)` in its place. Older versions of Vosh saved those lines in full, so a session you logged before updating can still show your password after a `> `. The game also shows two kinds of password as you type them, the one you set for a new character and any you give a command like `password <old> <new>`, and the log saves those in full in every version.\n\nType `#logs forget-passwords` to count the lines that hold a password. Vosh says how many it found and in how many sessions, and it never shows the lines themselves. Type `#logs forget-passwords now` to blank them. Each one then reads `> (hidden)`, and Vosh rewrites `logs.sqlite` so the old text is gone from the disk too. On a large log this takes a few seconds, and new game text waits until it finishes. The rewrite needs free disk space about the size of `logs.sqlite`. When Vosh cannot finish it, the lines stay blanked, Vosh says so, and the next `#logs forget-passwords now` finishes the rewrite. A backup of your disk, like Time Machine, keeps its own copy of the old file. If you copied or shared one of those sessions, change your password in the game.',
  },
  {
    id: 'characters-and-data.stay-updated',
    number: '7.5',
    title: 'Check for updates',
    section: 'Characters and data',
    body: "Vosh checks for new builds and installs them in place. The controls live in Settings under General, in the Updates section.\n\n- Open Settings. General opens by default. The Updates heading reads `You have Vosh <version>.` beside a `Check now` button.\n- Click `Check now`. The line reads `Checking for updates…`, then `Vosh is up to date.` when nothing is newer.\n- When a build is ready, the line reads `Vosh <version> is ready.` and the button turns into `Install and restart`. Click it and Vosh installs the build and relaunches on it.\n- Turn on `Check for updates when Vosh opens` to run the check at every start. It is off by default. With it on, a banner appears in the main window when an update is waiting.\n\nUpdates download from the project's GitHub releases, and Vosh checks every build's signature before installing.\n\n`Check for updates` is one of the four switches under Keep the same for every character, also in General. It is on by default, so one setting covers every character. Turn it off when one character should check on launch while the others stay quiet.",
  },
  {
    id: 'fix-it.terminal-renderer',
    number: '8.1',
    title: 'Switch terminal renderers',
    section: 'Fix it',
    body: 'Vosh ships two terminal renderers. The native GPU surface is the default on macOS and the xterm renderer is the default on Windows and Linux. The `#nativesurface` command switches between them from the command line.\n\n- Type `#nativesurface off` to force the xterm renderer everywhere.\n- Type `#nativesurface on` to force the native surface everywhere.\n- Type `#nativesurface default` to return to the platform default.\n- Restart Vosh. The switch applies only on restart, and the echo reminds you with `restart Vosh to apply`.\n\nThe command runs entirely in the frontend and stores your choice locally under the key `vosh.nativesurface`. A bad argument echoes `usage #nativesurface on | off | default (takes effect on restart)`.\n\nOn Windows and Linux, Settings under General, then Advanced, holds `GPU rendering`, which draws the xterm renderer with your graphics card. Turn it off when the terminal draws wrong, then restart Vosh.\n\nIf the text renders in the wrong typeface, open Settings and choose Appearance, then Terminal text. The default font is JetBrains Mono, which ships inside Vosh and works on every machine. Vosh no longer ships Berkeley Mono. A font list that names it uses the copy installed on your computer, and JetBrains Mono when you have none. Install the font you want or pick it in `Font`. The size defaults to 14.\n\nUnder General, `Font and size` in Keep the same for every character decides whether every character shares one font. Turn it off to let each character keep its own.\n\nWith the xterm renderer the right click menu offers `Clear scrollback`. The native surface hides that item because its grid has no clear command.',
  },
  {
    id: 'fix-it.reconnect',
    number: '8.2',
    title: 'Recover a bad connection',
    section: 'Fix it',
    body: 'The session button in the title band holds the connection controls. Its dot shows idle, connecting, connected, or an error. After a failed connection the button reads `Not connected`, and pointing at it shows why.\n\n- Click the session button and choose `Disconnect`, then wait for the dot to go idle.\n- Choose `Edit connection…` to check the address. The form holds `Host`, `Port`, and `Use TLS`, and the defaults are `play.theforsakenlands.com` on port `1848` with TLS off. Click `Save`.\n- Choose the `Connect to` row, or press `Cmd+R` on macOS or `Ctrl+R` elsewhere.\n\n`Use TLS` wraps the connection in TLS. Match it to what the server offers on that port. The default port `1848` expects it off. Settings under General, then Connection, edits the same saved world with its `World`, `Host and port`, and `Use TLS` rows.\n\nDisconnecting has side effects. Session scoped variables clear when the next connection opens, so anything set with `#var` never carries into the new session, while profile variables survive. The chat pane buffer clears at disconnect. On reconnect, Vosh matches the host and port against your profiles and switches to the best match automatically, and it picks up the profile set to log in as your character after login.\n\nTwo other paths reach the same controls. On macOS the Session menu in the menu bar holds the `Connect to` row, `Edit connection…`, `New connection…`, and `Disconnect`. And the `Cmd+K` palette runs the `Connect to` row or `Disconnect`.',
  },
  {
    id: 'fix-it.data-on-disk',
    number: '8.3',
    title: 'Find your data on disk',
    section: 'Fix it',
    body: 'Vosh keeps all of its data in one app data folder named `com.aabahran.vosh`.\n\n- On macOS, open `~/Library/Application Support/com.aabahran.vosh`.\n- On Linux, open `~/.local/share/com.aabahran.vosh`.\n- On Windows, open `%APPDATA%\\com.aabahran.vosh`.\n\nInside that folder.\n\n- `profiles.toml` indexes your profiles and names the active one.\n- `profiles/<name>.toml` holds each profile snapshot with connection defaults, aliases, variables, triggers, timers, tick config, and macros. In loadout mode the aliases, triggers, and macros live in `catalog.toml` instead, and the profile file keeps the rest.\n- `profiles/legacy/` holds a copy of each profile file as it was when the loadouts migration ran.\n- `global.toml` holds cross profile UI preferences.\n- `catalog.toml` and `loadouts.toml` appear once loadout mode is active.\n- `logs.sqlite` stores session logs, with `-wal` and `-shm` sidecars alongside.\n- `scrollback.txt` persists the last 10,000 terminal lines across restarts.\n- `maps.sqlite`, if you have one, holds rooms that older builds recorded. Vosh no longer reads or writes it.\n- `affect_full.toml` remembers the most hours Vosh has seen for each affect, for each character.\n- `scripts/` holds Lua files for `#script load`.\n- `plugins/` holds plugin folders, each with a `manifest.toml`.\n\nEvery TOML save is safe by design. Vosh writes the new text to a temp file, copies the old file to `<file>.bak.<timestamp>` with a millisecond timestamp, swaps the temp file in atomically, and keeps the ten newest backups. A save that fails leaves the old file in place. To roll back a bad profile edit, copy the backup you want over the live file.\n\nA leftover `profile.toml` at the root is the legacy single profile file. Vosh migrates it to `profiles/default.toml` on the first multi profile launch.',
  },
  {
    id: 'reference.slash-commands',
    number: '9.1',
    title: 'Slash commands',
    section: 'Reference',
    body: "This is every slash command Vosh understands today.\n\n- `#help` prints the command summary, and `#help <words>` opens Help on those words.\n- `#alias <name> <expansion>` defines, `#unalias <name>` removes, `#aliases` lists.\n- `#var <name> [value]` sets or shows a session variable, `#unvar <name>` removes it from both scopes, `#vars` lists.\n- `#trigger <name> {pattern} <action> [args]` defines, `#untrigger <name>` removes, `#triggers` lists by priority.\n- `#prompt game {setting}` and `#prompt fight {setting}` read your prompt in this profile from the codes of your PROMPT and fight prompt, `#prompt {regex}` reads it with a pattern, `#prompt` says how Vosh reads it, and `#unprompt` stops reading it.\n- `#prompt draw on|off` draws your design in place of your prompt in this profile, or shows the game's own prompt.\n- `#prompt show text|lifted|pinned` shows your prompt in this profile in the text, lifted on a band in the text, or pinned above the command line.\n- `#prompt default` puts Vosh's default design in place of the design in this profile and keeps yours as an earlier design.\n- `#group <name> on|off` toggles a group, `#group <name>` shows state, `#groups` lists.\n- `#tick`, `#tick interval <secs>`, `#tick reset`, `#tick on {pattern}`, `#tick off`, `#tick fire <command>`, `#tick nofire`, `#tick sound on|off`, `#tick disable`, `#tick enable` drive the tick timer.\n- `#tick warn`, `#tick warn at <secs>`, `#tick warn message <text>`, `#tick warn color <name>`, `#tick warn off` shape the tick warning.\n- `#script load <name>` loads a Lua file, `#script reload` reruns loaded scripts, `#scripts` lists them.\n- `#lua <code>` evaluates Lua inline.\n- `#profile save`, `#profile load`, `#profile reset` manage the profile snapshot. In loadout mode all three become notices.\n- `#import-tintin <path>` imports TinTin++ aliases and variables.\n- `#logs forget-passwords` counts the lines in your session log where you sent a password, and `#logs forget-passwords now` blanks them.\n- `#record <name>` starts recording, `#record` shows status, `#record cancel` discards, `#endrec` saves the recording as an alias.\n- `#qkey <name> <verb>` configures a quick key, `#qkey clear <name>` clears, `#qkeys` lists.\n- `#target <args>` mirrors `tar`, with `#target clear|next|prev`, `#tarn`, `#tarp`, `#tarclear` as slash forms.\n- `#nativesurface on|off|default` forces the renderer, applied on restart.\n\nTargeting also works bare with no `#`. Type `tar` to list, `tar <N>` or `tar <substr>` to pick, `tarn` and `tarp` to cycle, `tarclear` to clear.\n\nAn unknown command points you at `#help`. Errors echo wrapped in square brackets.",
  },
  {
    id: 'reference.keyboard-shortcuts',
    number: '9.2',
    title: 'Keyboard shortcuts',
    section: 'Reference',
    body: 'This is every built in key Vosh binds, grouped by where it works. On macOS the window shortcuts use `Cmd`, since `Ctrl` belongs to your macros there. Windows and Linux use `Ctrl`.\n\nAnywhere in the main window.\n\n- `Cmd+K` toggles the command palette.\n- `Cmd+F` opens the find bar, and pressed again puts the caret back in it.\n- `Cmd+R` connects to the saved world while you are not connected.\n- `Cmd+,` opens Settings.\n- `Cmd+/` opens Help.\n- `Cmd+Shift+L` shows or hides the panel.\n- `Cmd+\\` opens or closes the scrollback split.\n\nIn the command line.\n\n- `Enter` submits. `Shift+Enter` inserts a newline for multi line compose, and in password mode it submits instead.\n- `Tab` and `Shift+Tab` cycle tab completion through your history words, room characters, and recently seen names.\n- `ArrowUp` and `ArrowDown` recall history, filtered by whatever prefix you already typed.\n- `PageUp` and `PageDown` page the scrollback. On macOS press `Fn+Up` and `Fn+Down`.\n- `Escape` cancels an in flight paste burst, closes the scrollback split, and snaps the terminal to its tail.\n- `Home` and `End` jump the caret, also reachable as `Cmd+Left` and `Cmd+Right` or `Fn+Left` and `Fn+Right` on macOS. Add `Shift` to extend the selection.\n- `Cmd+A` on an empty command line selects the whole terminal, scrollback included.\n- `Cmd+C` with nothing selected in the command line copies the terminal selection.\n\nIn the find bar. `Enter` finds the next match, `Shift+Enter` the previous, `Escape` closes and clears.\n\nIn the command palette. `ArrowUp` and `ArrowDown` move the selection, `Enter` runs the entry, `ArrowRight` opens a list like Choose theme, `ArrowLeft` or `Backspace` steps back out of it, and `Escape` steps back or closes.\n\nIn the terminal menu. `ArrowUp` and `ArrowDown` move through the items, `Enter` picks one, `ArrowRight` opens the Settings list, `ArrowLeft` steps back out of it, and `Escape` closes the list, then the menu.\n\nIn Settings and Help. `Cmd+F` puts the caret in the search, `ArrowUp` and `ArrowDown` move through the results, and `Escape` clears the search. In Help, `Enter` steps to the next match in the topic you read and `Shift+Enter` to the previous one.\n\nMouse on the terminal. Wheel up opens the scrollback split. Middle click closes the split and snaps to the live tail. Right click opens the terminal menu.\n\nBind your own keys as macros in Settings under Automation, then Macros. Canonical names look like `F1`, `Ctrl+N`, `Shift+F5`, and `Ctrl+Alt+Numpad7`.',
  },
  {
    id: 'reference.prompt-codes',
    number: '9.3',
    title: 'Prompt design codes',
    section: 'Reference',
    body: `Your own prompt is a design of text and codes. Customize prompt writes the codes for you as you click the parts of your prompt and pick values. Choose \`Edit as text\` there to read them or type your own.\n\n${promptCodesTable()}\n\nEvery value in \`Insert value…\` has codes of its own, and the picker shows them beside each form. Your tick, the time and the date keep counting while your prompt sits idle. Vosh draws it again each second they change, and waits while you select text or read back. Pinned, the band keeps counting through both. A line with \`%{right}\` ends on the last column of your terminal, and Vosh draws it again when the window changes width.`,
  },
];

/** One block of a help body. */
export type HelpBlock =
  | { kind: 'paragraph'; text: string }
  | { kind: 'list'; items: string[] }
  | { kind: 'table'; head: string[]; rows: string[][] };

/** The cells of a table line, `| a | b |` as `a` and `b`. */
function tableCells(line: string): string[] {
  return line
    .trim()
    .replace(/^\|/, '')
    .replace(/\|$/, '')
    .split('|')
    .map((cell) => cell.trim());
}

/** Read a help body into its blocks, in the format at the top of this
 *  file. */
export function parseHelpBody(body: string): HelpBlock[] {
  return body
    .split(/\n\n+/)
    .map((b) => b.trim())
    .filter((b) => b.length > 0)
    .map((block): HelpBlock => {
      const lines = block.split('\n');
      if (lines.every((l) => l.startsWith('- '))) {
        return { kind: 'list', items: lines.map((l) => l.slice(2)) };
      }
      if (lines.every((l) => l.startsWith('|'))) {
        const rows = lines.filter((l) => !/^\|[\s|:-]+\|?$/.test(l)).map(tableCells);
        return { kind: 'table', head: rows[0] ?? [], rows: rows.slice(1) };
      }
      return { kind: 'paragraph', text: block };
    });
}

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
