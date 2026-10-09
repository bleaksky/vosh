# Vosh help

This file is the help. The Help window reads it when Vosh is built, so the window shows what you read here. To open the Help window, press `Cmd+/` on macOS or `Ctrl+/` on Windows and Linux. You can also choose Help in the menu bar, Settings in the right click menu of the terminal, or Open help in the command palette.

---

## Get connected

### 1.1 Connect to a world

<!-- id: get-connected.connect -->

You connect from the session button. It sits in the middle of the title band, over the terminal. While you aren't connected, it reads `Not connected` beside a status dot.

- Click the session button and choose the `Connect to` row. You can also press `Cmd+R` on macOS or `Ctrl+R` on Windows and Linux. Vosh dials the world this session keeps. Until you save another world, that is `play.theforsakenlands.com` on port `1848`.
- To play on another world in this session, first choose `Edit connection…`. Type the `Host` and the `Port`. Turn on `Use TLS` when your server offers TLS. Click `Save`. From then on, the session dials that world.
- To play in a second session next to this one, choose `New session…` instead. It opens a new row with a form. The form starts from the world you saved last.
- Watch the dot change from connecting to connected.
- At the login prompt, type your character name and press `Enter`.
- When the server asks for a password, the command line changes to a masked field. The pill at its start reads `Password`. What you type doesn't show on screen. It doesn't echo to the terminal, go into command history, or go into the session log. Vosh sends it exactly as you type it, with no aliases, variables, or `#` commands. Press `Enter` to send it. Here `Shift+Enter` also sends it and doesn't add a line.

While you're connected, the button shows your character name and the world. If a saved profile matches the host and port you dialed, Vosh changes to that profile before it connects. A profile set to log in as your character takes over right after login, when the server tells Vosh who you are.

To disconnect, click the session button and choose `Disconnect`.

### 1.2 Reconnect

<!-- id: get-connected.reconnect -->

The status dot on the session button shows the state of the connection. When the connection fails, the dot changes to its error state. The reason shows in the terminal in square brackets, and also when you point at the button. When the session closes cleanly, the dot goes back to idle.

- To dial the same world again, click the session button and choose the `Connect to` row. You can also press `Cmd+R` on macOS or `Ctrl+R` on Windows and Linux.
- To read the output from before the drop, scroll up or press `PageUp`. The terminal scrollback stays after a disconnect. Only `Clear scrollback` clears it, and only when you choose it.
- To prepare commands while you're offline, type the first command. Press `Shift+Enter` to add more lines under it. Leave the block in the command line. After you reconnect, press `Enter` once. Vosh sends each line separately, in order.

When the connection drops while you play, Vosh dials the same world again on its own. The first try comes 3 seconds after the drop. The next tries come 6, 12, 24, 48, and 60 seconds after the try before. That makes 8 tries in about five minutes.

For each try that fails, the terminal shows a `[reconnect]` line with the reason. Vosh stops at the first try that connects. It sends nothing there, so the game waits at its prompt for about two minutes for you to log in.

Some events start no new tries. Vosh never dials again after your `Disconnect`, after a `quit` you typed, or after a line from the game that ends your visit. An example of that line is `You have escaped from the Forsaken Lands.` A drop at the account menu or at the login prompt also starts nothing. When another session logs in as the character this one plays, the game closes this connection. Vosh then leaves it closed.

While Vosh dials again, a notice at the bottom right shows the progress for the session in front. It counts down to each try, such as `Reconnecting in 6s` with `Try 2 of 8`. To dial at once, click `Reconnect now`, press `Cmd+R` on macOS or `Ctrl+R` on Windows and Linux, or choose the `Connect to` row. `Cancel` or `Disconnect` stops the tries. While a try dials, the notice reads `Connecting`. When all 8 tries fail, it reads `Vosh stopped after 8 tries`, and `Try again` dials the world one more time.

Through every try, the status dot keeps its error ring.

When the connection drops and Vosh won't dial again, a notice reads `Vosh will not reconnect`. It tells you why, such as `you quit`, `the game banned this account`, or `another session took Orla`.

To stop the automatic reconnect for a profile, turn off `Reconnect when the link drops` in Settings under General, then Connection. A drop then shows only `Connection lost`. You can also turn on the `Connection` alert preset, in Get alerts at 3.9. Vosh then gets your attention when the connection drops, when a reconnect gets to the login, and when Vosh stops trying.

Two things reset between connections. The chat pane empties when you choose `Disconnect` or connect to another world. A drop keeps it, so your tells are still there when Vosh reconnects. Session variables set with `#var` clear when the next connection opens, so they never last longer than a connection. Aliases, triggers, macros, and profile variables stay loaded, because they are part of your profile and not of the connection.

`Disconnect` is in three places. They are the session button while you're connected, the Session menu in the macOS menu bar, and the command palette (`Cmd+K`).

### 1.3 Save your profile

<!-- id: get-connected.profile-save -->

`#profile save` writes the current client state to the file of the profile your session plays. The profile loads again at startup with no more steps. The file is a TOML snapshot under `~/Library/Application Support/com.aabahran.vosh`.

- Set up the client state you want to keep. Aliases, triggers, macros, variables, and tick settings all count.
- Type `#profile save` in the command line. Vosh writes the snapshot to the TOML file of that profile.
- Or choose `Save profile` in the Session menu of the macOS menu bar, or in the command palette (`Cmd+K`). It sends the same command.

The snapshot holds the connection defaults, aliases, profile variables, triggers, tick configuration, macros, ui settings, and enabled plugins. It also holds the groups you turned off.

Variables set with `#var` have session scope. They clear when the next connection opens, and they never go into the file. To keep a value, put it in the `profile_vars` table of your profile file. That file is `profiles/<name>.toml` in the app data folder. Edit it there while Vosh is closed, or set the value from Lua with `mud.set_profile_var`.

`#profile load` reads the saved file back into the profile. `#profile reset` sets the profile back to its defaults. Both commands reach every session that plays the profile. Each of the other sessions prints a line that names the session where you typed the command, such as `Tolliver loaded this profile from its file.` In loadout mode the profile commands only show notices, because loadout mode saves your changes automatically.

### 1.4 Play in more than one session

<!-- id: get-connected.sessions -->

Each session is one connection to a game. It has its own terminal, command line, and command history. While two or more sessions are open, the sessions sidebar shows on the left of the window, with one row for each session. With one session, the sidebar hides on its own.

Each row has two lines. The first line starts with a mark that shows the state of the session. Then comes its name. At the end is a count when something waits for you there. The second line tells what the session is doing.

- Click a row to bring its session to the front. The terminal, the command line, the title band, and the panes show that session at once.
- Press `Cmd+1` to `Cmd+9` to bring the first nine rows to the front. Press `Cmd+Shift+]` for the next row and `Cmd+Shift+[` for the row before. After the last row, they go round to the first. Hold `Cmd` for a moment and each row shows its key. On macOS the Session menu also has `Next session` and `Previous session`.
- The sessions behind keep playing. Their triggers, timers, and Lua run as usual. Only the drawing waits until you look.
- To hide the sidebar in this window, click the sidebar button at the top left of the window. On macOS it sits just after the window buttons. Click it again to show the sidebar. The button never moves, and it is brighter while the sidebar shows. The sidebar stays hidden when you open another session.
- On macOS `Ctrl+Cmd+S` also hides and shows the sidebar. On Windows and Linux the key is `Ctrl+Shift+S`. `Hide sessions` and `Show sessions` in the command palette (`Cmd+K`) do the same. On macOS they are also in the View menu.
- Right click a row to open its menu. It has `Rename session…`, `Edit connection…`, `Disconnect` while the session is connected, and `Close session`. Each item acts on the session of that row. `Rename session…` and `Edit connection…` first bring that session to the front.

`Sessions` is the heading of the list. It shows how many sessions are open, such as `Sessions 3`. When there are more rows than fit, they scroll under the heading. A thin line shows under the heading when a row has scrolled under it.

To move a row, drag it up or down. The other rows make room, and an accent line shows where it goes. `Cmd+1` to `Cmd+9` follow the new order, and so does your next launch.

To make the sidebar wider or narrower, drag the line at its right edge. The width goes from 180 to 320 pixels. Double click the line to go back to 220. Vosh keeps the width for your next launch.

Sometimes the window is too narrow to hold the sidebar, a terminal 320 pixels wide, and the panel. Then the panel first gets narrower, down to its narrowest. After that, the sidebar folds away. When you make the window wider, it comes back.

Until then, the sidebar button slides the sidebar in over the terminal. It slides away when you choose a row, click the button again, or press `Escape`. The caret then goes back to the command line.

While the sidebar is folded, in a narrow window or after you hide it, the menu of the session button lists every session. They are at its top under `Sessions`. Each session shows the same two lines as its row. The session in front has a check. A session where something waits shows its count, and any other session shows its key, such as `Cmd+2`.

Click a session to bring it to the front. To close it, point at it and click the cross. The session button adds up what waits in your other sessions, such as `2` after the arrow. The number goes away when you have looked at each session.

To open a session, press `Cmd+T`. You can also choose `New session…` from the session button, the command palette (`Cmd+K`), or the Session menu on macOS. Or click `New session`, the plus at the top of the sidebar. Vosh adds a row that reads `New session`. It brings the row to the front and opens its form under the title.

- `Host` and `Port` start from the world you saved last or dialed last from a form. The caret waits in `Port`, so you can type the build port and keep the host.
- `Profile` starts on a profile pinned to that host and port. If there is none, it starts on a profile that claims the host on any port. If there is none, it starts on the profile you were playing. It chooses again when you change the address, until you choose a profile yourself. The window takes the layout of the profile it shows.
- Click `Connect` to dial. The session plays the profile the form shows, and no other profile matches first. The session keeps the address as its own.
- Click `Cancel` or press `Escape` to close the new row. Vosh writes nothing.

The line under `Profile` tells you why that profile shows. `Build is pinned to The Forsaken Lands 1825.` names the pin. When another session plays the same profile, the line names that session, such as `Tolliver's session plays Default too. An edit in either reaches both.` Two sessions on one profile share its aliases, triggers, and settings. An edit in either session saves one time. Each session keeps its own connection, command history, and Lua.

Another session can be connected to the own port of the world, such as `1848` for The Forsaken Lands. When you dial that port too, a note quotes the game, such as `Tolliver is connected to this world. HELP MULTI lists “Having more than one character logged on at once.”` The build port, `1825`, never shows this note. Vosh still connects when you click `Connect`, and the game decides what happens next.

Each row names its session by the first of these that it has.

- A name you gave it, such as `Builder`.
- The character it plays, such as `Tolliver`. The row keeps this name through a drop, every try to dial again, and a disconnect. It keeps it until you connect again yourself.
- Before you log in, the world it plays, such as `The Forsaken Lands`.
- `New session` while it has no world yet.

A port that isn't the own port of the world shows in grey beside the name. So Orla on the build port reads `Orla` with `1825` beside it. A row named by its world puts the port in its name instead, as `The Forsaken Lands 1825`. When the row is too short, the world ends in an ellipsis and the port stays. On a host that Vosh doesn't know by name, the port shows only while another session plays on the same host.

The second line tells what the session is doing. While you play, it shows the room, such as `Thickening Woods`. During a fight it shows who you fight, such as `Fighting a Blackwatch guard`. Your health shows at the right, such as `91%`. Health turns red when it gets low.

In some places the game hides your vitals, and then the line shows no health. At other times, the line tells what happened.

- `Waiting for your login` while the game waits for you to log in.
- `Connecting…` while Vosh dials, and `Reconnecting, try 2 of 8` while it dials again after a drop.
- `Couldn’t connect` when the first dial fails.
- `Dropped 4 min ago` when the connection dropped and Vosh doesn't dial again.
- The world it dials, such as `The Forsaken Lands`, while the session isn't connected.

When you named a row, its second line starts with the character it plays. So you always see who plays it.

Point at a row for half a second, and a card opens beside it with more about the session. It names the world and the profile, such as `The Forsaken Lands 1825, profile Build`. Then it shows the room, the area, who you fight, and your health, mana, and moves. It also shows how long you have been online and what waits for you.

A session that isn't connected shows when it last played. When you point at the next row, the card moves there at once. The card ends with `Double click the name to rename`. It closes when the pointer leaves the rows, when you click, and when you press a key.

To name a session, double click its name in the row. When a row has the keyboard focus, `Enter` or `F2` also does this. A row gets the focus when `Tab` or the arrow keys bring you into the sidebar. On the command line, `F2` stays yours.

You can also right click the row and choose `Rename session…`. It shows `F2` beside it when the row has the focus. The session button, the command palette (`Cmd+K`), and the Session menu on macOS have `Rename session…` for the session in front.

The name then changes to a field with its text selected. The mark stays, and the second line reads `Return saves, Esc cancels`. To keep what you typed, press `Enter` or click somewhere else. To leave the row as it was, press `Escape`.

If you clear the field, the row shows the character again. A name helps you tell apart two rows that play one character. An example is `Tolliver` on the play port and `Builder` for Tolliver on the build port.

With one session there is no sidebar. So `Rename session…` opens a form under the title with one `Name` field. Click `Save` to keep what you typed.

The name shows in place of the character in the row, the title band, the window title, and the command palette. It also shows in the questions before a close. The card beside the row still names the character with the world, such as `Tolliver on The Forsaken Lands 1825, profile Build`. The name stays with the session through a reconnect, another character, and your next launch.

The selected row is the filled one. The title band and the window title follow it. They add the port after the world in the same way, so the band reads `Orla` and `The Forsaken Lands 1825`.

A row tells you when something happens in a session you aren't looking at. Its name gets brighter when the game prints a new line there. A prompt alone doesn't count. A count in an accent pill shows at the right of the first line when something for you happens there. It counts each tell, each line with your name, and each fight that starts on you. It counts them whether or not you turned on their alerts.

Low health counts one time, however often it falls. Past nine, the count reads `9+`. A drop adds nothing, because the mark shows it. An alert from a trigger or a script also adds nothing. When you bring the session to the front, the count and the bright name clear.

Every row shows one mark at its left, and so does the row in front.

- A green dot while you play.
- A ring while the session isn't connected. Its name also turns grey.
- A spinner while Vosh dials, and through every try when it dials again after a drop.
- A hand while the game waits for you to log in, until you play. It shows on The Forsaken Lands, and on any world after Vosh dials again.
- A triangle when the first dial fails, or when the connection dropped and Vosh doesn't dial again. Connect again to continue.

A row shows one mark at a time. The triangle comes first, then the hand, then the spinner.

The tick sound plays only for the session in front. The `Connected`, `Connection lost`, and reconnect notices also speak only for it.

Each session keeps these things of its own.

- Its terminal and scrollback, and its command line with its history and the text you left in it. Also the world it dials.
- Its target, its quick keys, and the count of its tick.
- Its session variables, set with `#var`.
- Its recorder, so `#record` takes only the commands you type in that session.
- Its Lua, with the plugins its profile turns on, the scripts you load with `#script load`, and the aliases its plugins make. When Vosh stops the Lua of a trigger or an alias, it stays off only in that session.

The sessions on one profile share everything the profile holds. That is its aliases, triggers, macros, and timers, and its groups and profile variables. It is also its tick settings, its prompt design, its loadouts, and its panes. A change from any of these sessions reaches the others at once and saves one time. So `#group combat off` turns that group off in every session on the profile, and so does its switch in Settings.

Each `#tick` command that changes a setting reaches all the sessions on the profile. But `#tick reset` restarts the count of its own session only. `#profile load` and `#profile reset` also reach all of them. Each of the other sessions prints a line that names the session where you typed the command.

When you log in as a character that another profile claims, the session moves to that profile. The other sessions stay where they are. If a session already plays that profile, the two sessions share it from then on.

When you open Vosh again, your sessions come back in their order with their names. None of them is connected. The session you left in front is in front again. A session opens its profile and its Lua the first time you bring it to the front. Its terminal shows the lines it kept.

Settings edits the profile of the session in front. With two or more sessions open, the Settings header names that session at the right. Then it shows the profile that the session plays in grey, such as `Orla` and `Build`. When another session plays the same profile, the header adds it, such as `Also in Tolliver`. This is because an edit reaches both sessions.

Sometimes a list under Automation has unsaved changes when you bring a session on another profile to the front. Settings then stays on the profile you were editing. Its header keeps the name of that session and profile and reads `Save or discard to follow Orla`. Click `Save` or `Discard`, and Settings moves to the profile that Orla plays. Each change you make in Settings saves to the profile where you made it. It doesn't matter which session is in front when the change saves.

In Settings under General, then Connection, you edit where the session in front connects. Use its `World`, `Host and port`, and `Use TLS` rows. Each session keeps its own values. The `Reconnect when the link drops` row belongs to the profile, so it reaches every session on the profile. A session on a port that isn't the own port of the world shows in `World` as its row reads, such as `The Forsaken Lands 1825`. When you choose `The Forsaken Lands`, the port changes to `1848`.

To close a session, point at its row and click the cross that shows in place of the count. You can also press `Cmd+W`. Or choose `Close session` from the right click menu of the row, the Session menu on macOS, or the command palette (`Cmd+K`).

While the session is connected, Vosh asks first, such as `Close Orla's session?`. `Cancel` keeps the session. A session that isn't connected closes at once. Its row goes away, and the next row down comes to the front. When you close your last session, the window closes.

When you close the window, every session ends and Vosh quits. While a session is connected, Vosh asks first. It asks when you press `Cmd+Shift+W`, choose `Close window` in the Session menu on macOS, or click the close button at the top of the window. The question names each connected session, such as `Two sessions are connected, Tolliver on The Forsaken Lands and Orla on The Forsaken Lands 1825.`

On macOS, `Quit Vosh` and `Cmd+Q` ask first only while two or more sessions are connected. With one connected session, Vosh quits at once. A quit from the Dock, or a quit when you log out, can't ask.

### 1.5 Get started

<!-- id: get-connected.get-started -->

Get started is a short list of things to turn on in Vosh. Each thing has a line that tells what it does. The list opens on its own the first time you start Vosh, and you can open it again here.

[Open Get started](vosh:get-started)

A new install starts with every preset off. The list suggests the presets for the world you connect to, each with a sample. A switch turns a preset on at once.

| What                          | Where it lives                              |
| ----------------------------- | ------------------------------------------- |
| Connect to The Forsaken Lands | The session button, or `Cmd+R`              |
| Color what the game prints    | Settings, Automation, Presets               |
| Add Chat and Group            | Add a pane, the plus in the title band      |
| Track the affects you keep up | Settings, Characters, Tracked affects       |
| Customize your prompt         | The first row when you right click the text |
| Read back while you play      | Scroll up, `Cmd+\` or a middle click        |

On macOS, choose Get started in the Help menu. On any system, press `Cmd+K` on macOS or `Ctrl+K` on Windows and Linux, and type get started.

## Play

### 2.1 Send commands

<!-- id: play.send-commands -->

The command line sends lines to the game. It sends single commands, chained commands, blocks of more than one line, and pastes.

- Type a command and press `Enter` to send it.
- To chain commands on one line, put `;` between them. Each part goes out as its own command. Type `\;` for a literal semicolon.
- To add a line without sending, press `Shift+Enter`. The command line gets taller. When it holds two or more lines, a gutter with line numbers shows. Press `Enter` to send every line separately, in order. Vosh drops blank lines.
- Press `Enter` on an empty command line to send a bare line. Many MUD prompts go on when they get one. It echoes as your mark on its own line, or as a blank line when the mark is Off. So you see each one go out. After a prompt that ends in `>`, it ends that row and adds nothing.
- Paste text with more than one line into the command line. A single line sends at once. Two or more lines become a paste burst. By default Vosh sends one line every 500 ms. A `paste N/M esc cancels` counter shows in the command line.
- To cancel every line of a burst that hasn't gone out yet, press `Esc`. A new paste also cancels the old burst.

When `Enter` does something else, the command line names it in a pill where your mark sits. At a password prompt the pill reads `Password`. At the pager of the game it reads `More`, and `Enter` shows the next page. During a walk it reads `Walking` with the steps left, and `Esc` stops the walk. In the editor of the game it names your text, as Write your description shows.

When `Keep last command` is on in Settings under Input, then Command line, a command you send stays in the command line, fully selected. Press `Enter` again to send it again. Or start to type to replace it.

Each command you send echoes in the text after a grey `›`. So your commands are easy to tell apart from the lines the game sends. To choose another mark, use `Mark before your commands` in Settings under Input, then Sent commands. Choose `>`, your own text of up to four characters such as `you:`, or Off to echo your commands with no mark.

When `Use the same mark in the command line` is on, the line you type in starts with the same mark. It is on at first. When the mark is Off, the line starts with your text.

Turn on `Dim sent commands` to draw your commands faint, so the lines of the game are easier to see. The mark keeps its color. A quick key and a macro echo in the same way. Right after a prompt that ends in `>`, such as `Account name>` at login, a command echoes without its mark. This is true for every mark, because the prompt already marks the command.

To change the line you type in, go to Settings under Input, then Command line. `Caret blinks` is on at first. When Reduce motion is on in your system settings, the caret doesn't blink in any case. `Caret color` changes the color of the caret. Until you choose a color, the caret has the accent color of your theme. `Text color` changes the color of what you type.

`Background` has three choices. It can keep the band of the theme. It can give the band a `Slight tint` of the accent color of your theme, so the line is easy to tell apart from the game. Or it can take `Your own` color. `Size` starts at `Same as terminal`. A size you choose changes only the command line.

Turn on `Color commands as you type` to color each line by its first word. An alias and a Vosh `#` command color that word. A chat line colors whole, from the same list that spell check uses. A `#` command that Vosh doesn't know turns red. Vosh colors only what it knows for sure, so game commands stay plain. The selection, spell check, and the caret work as before.

To set the delay, use `Wait between pasted lines` in Settings under Input, then Advanced. It goes from 0 to 10000 ms.

### 2.2 Recall command history

<!-- id: play.reuse-history -->

Command history records every line you send during a session. You can get the lines back in the command line.

- In an empty command line, press `ArrowUp` to go back through the commands you sent, newest first.
- To search by prefix, type a few characters, then press `ArrowUp`. Only lines that start with that text come up.
- Press `ArrowDown` to go toward newer matches. One step past the newest match puts back exactly what you typed before the search started.
- You can edit the recalled line at any point. An edit ends the search. The next `ArrowUp` starts a new search from the text that is now in the command line.

History skips a line that is the same as the line before it. It never records what you type in password mode.

When `Keep last command` is on, the line you just sent stays in the command line, fully selected. `Enter` sends it again, and when you type, your text replaces it.

When you write more than one line, the arrows do their usual job first. `ArrowUp` moves the caret up a line, unless you are on the first line. `ArrowDown` moves it down a line, unless you are on the last line. So history recall starts only from the first or last line of the block.

Example. Type `tell` and press `ArrowUp`. Only the lines that start with `tell` come up.

### 2.3 Complete names with Tab

<!-- id: play.tab-complete -->

Tab completion finishes a word you started to type in the command line. It uses names that Vosh already knows.

- Type the first letters of the word, anywhere in the command line.
- Press `Tab`. Vosh completes the word at the caret with its best match.
- Press `Tab` again to go to the next candidate, or `Shift+Tab` to go back. After the last candidate, the list starts again.
- When you type or press any other key, the cycle resets. The current completion stays in place.

On an empty line, `Tab` moves to the panel and `Shift+Tab` moves back to the terminal.

Candidates come from three sources, in this order.

- Words from commands you typed, most recent first.
- Characters in your room, when the server sends `Room.Chars` over GMCP.
- Names with a capital letter that Vosh saw in the output during the last 30 minutes.

A candidate matches when it starts with the letters you typed, in any case. Vosh shows each candidate one time. It skips a candidate that is the same as what you already typed. Completion works on the word at the caret, so you can edit the middle of a line and keep the rest.

### 2.4 Scroll back through history

<!-- id: play.scroll-back -->

Scrollback opens in a split above the live terminal. You can read old output while new output continues to come in below it.

- Scroll the mouse wheel up over the terminal. The first notch opens the split, with history above and the live tail below. When you scroll more, the history moves line by line.
- Or press `PageUp` to open the split and go up a page. Press `PageDown` to go down a page. On a Mac keyboard these keys are `Fn+Up` and `Fn+Down`.
- Or press `Cmd+\` on macOS or `Ctrl+\` on Windows and Linux to open the split. Press it again to close the split. The View menu and the command palette call it `Split terminal`.
- To see how far back you are, read the count at the top right of the terminal, such as `54 / 78`.
- To change the size of the split, drag the divider between the history and the live tail. To set its color, go to Settings under Layout, then Split terminal.
- There are three ways back to live. Scroll or page down to the bottom of the history, and the split closes on its own. Or press `Esc`. Or middle click the terminal.

While the split is open, the live tail never scrolls away. New output continues to come in there. The lines you type also show in the history, so the record has no gaps.

The terminal keeps 10,000 lines of scrollback. To keep more or fewer, open Settings and choose Logs. Then choose a size from 1,000 to 100,000 lines in `Scrollback size`.

Both renderers follow this size. The scrollback that Vosh restores at your next launch also follows it. A smaller size drops the oldest lines. Each character keeps its own size.

Times stay in the session log, in Settings under Logs, then `Search logs…`.

Turn on `Collapse repeated lines` in Settings under Appearance, then Terminal text. A line that the game sends again and again then takes one row. A line that is exactly the same as the line above it, colors included, joins it. The row shows a gray count in front, such as `(3) You are hungry.` The count goes up in place as more lines come in.

Any other line ends the run, and so does a blank line. The lines you type end it too, and so do a reply from Vosh and a prompt that stays in the text. A pinned prompt leaves the text, so a run continues past it.

In Aabahran, type `compact` to drop the blank line before each prompt. A run then continues from one round to the next. Your session log keeps every line, and your triggers fire on each line. This setting is off until you turn it on.

Two rows under it set what collapses during a fight. `In a fight` covers every line that comes in while you fight. It starts on `Collapse`. Choose `Show every line` to give each line of a fight its own row.

`Attack lines` covers each hit and miss that the game shows you, in a fight or not. That is your hits and misses, the ones on you, and the ones you watch. It starts on `Show every line`. So two blows show as two lines, and never as `(2) Your slash DISMEMBERS a Blackwatch guard!`, where the count is easy to miss. While `In a fight` shows every line, attack lines also show every line, and the row tells you so.

With the xterm renderer, the divider snaps to whole terminal rows. You can also move it with the keyboard. The arrow keys move it 16px, and `Shift` with an arrow moves it 64px. The native renderer splits its own grid. A middle click at the live tail opens the split a page up.

### 2.5 Find text

<!-- id: play.find-text -->

The find bar searches all of the session scrollback. It floats over the top right of the terminal.

- Press `Cmd+F` on macOS or `Ctrl+F` on Windows and Linux. The find bar opens even while you type in the command line. When you press the key again, the caret goes back to its field.
- Type your query in the `Find in scrollback` field.
- Press `Enter` for the next match and `Shift+Enter` for the previous match. The up and down arrow buttons do the same.
- Read the count beside the field. It shows `2 of 6` while you go through the matches. It shows `No matches` when the query finds nothing.
- To make the query narrower, use the three toggles. `Match case` makes it case sensitive. `Whole word` matches only whole words. `Regular expression` reads the query as a regular expression.
- To close the bar, press `Esc` or the close button. This clears every highlight and puts the focus back in the command line.

With the xterm renderer, a match above the visible screen opens the scrollback split. The match shows near the top of the history. A match that is already on screen closes an open split instead. The native renderer scrolls its own grid to each match.

You can also open the find bar from `Find in scrollback…` in the right click menu, the Edit menu, and the command palette (`Cmd+K`).

### 2.6 Copy terminal text

<!-- id: play.copy-text -->

To copy terminal text to the system clipboard, select it with a drag.

- Drag across the output you want. While text is selected, a click doesn't put the focus back in the command line, so the selection stays.
- Press `Cmd+C` on macOS or `Ctrl+C` on Windows and Linux.
- Or right click the terminal and choose `Copy`. The menu shows its shortcut beside it.
- To copy everything, press `Cmd+A` on macOS or `Ctrl+A` on Windows and Linux while the command line is empty. Or choose `Select all` in the right click menu. This selects the whole terminal, scrollback included. Then `Cmd+C` copies it.

One rule decides what copies. When the command line itself holds a selection, `Cmd+C` copies that selection and not the terminal. When you want the terminal text, clear that selection first, or choose `Copy` in the right click menu.

`Paste` in the right click menu puts the clipboard in the command line and sends nothing. Edit the line if you need to, then press `Enter` yourself.

The right click menu also has `Clear scrollback`. It empties what you can scroll back through, now and at your next launch. Your session log keeps every line.

### 2.7 Use the command palette

<!-- id: play.palette -->

The command palette runs Vosh commands from the keyboard. It has the View and Session commands, the Settings pages, your prompt, your aliases, and your sessions.

- Press `Cmd+K` on macOS or `Ctrl+K` on Windows and Linux. You can also click the search button at the right end of the title band. The same shortcut closes the command palette again.
- With nothing typed, it lists the last few commands you ran under Recent. Then it lists View and Session.
- Type a few letters to search every command. Entries with a title that starts with your text come first. Then come titles that hold your text anywhere. Then come the other words each entry answers to.
- Move the selection with the arrow keys. Press `Enter` to run the highlighted entry. Some rows have a list behind them, such as `Choose theme`. `Enter` or `ArrowRight` opens the list. `ArrowLeft` or `Backspace` goes back out.
- Press `Esc` to go out of a list, or to close the command palette and run nothing.

The palette sorts what it finds into five sections.

- Input. `Customize prompt…`, `Draw your prompt`, and `Edit prompt as text…`. Also a row for each text that the writing card takes, such as `Write a note…`, `Report a bug…`, and `Edit your description…`.
- View. `Show panel`, `Split terminal`, `Choose theme`, and a row for each pane, such as `Show map`. Also the rows that choose where your prompt shows, `Reset panel layout`, `Find in scrollback…`, `Open help`, `Get started`, and `Open settings`. Also a row for each Settings page, such as `Open trigger settings`.
- Aliases. Every alias that is on. An alias that takes no arguments runs as soon as you choose it. An alias that takes arguments puts its name in the command line instead. You then finish the line and press `Enter`.
- Session. `New session…`, then `Next session` and `Previous session` while two or more sessions are open. Also `Close session`, and `Hide sessions` or `Show sessions` with two or more sessions. Also `Save profile`, and the `Connect to` row or `Disconnect`. Disconnect is always last, and the command palette never opens with it selected.
- Go to. Every open session by the name its row shows, while two or more are open. A character shows with the world beside it. The session in front has a check, and the first nine show their keys, `Cmd+1` to `Cmd+9`. Choose a session to bring it to the front. Or type a name or a port to find it.

### 2.8 Use the right click menu

<!-- id: play.right-click-menu -->

The right click menu of the terminal holds the everyday actions of the terminal in one place.

- Right click anywhere on the terminal to open it.
- `Customize prompt…` opens Customize prompt over your prompt. There you design how Vosh draws your prompt.
- `Write` opens a list of the things you can write. They are a note, a journal entry, an application, an idea, a bug or typo report, your description, and your history. Each one opens the writing card, as Write your description at 2.9 and Write in the game at 2.10 show.
- `Copy` copies the current selection. `Paste` puts the clipboard in the command line. Nothing goes out until you press `Enter` yourself.
- `Select all` selects the whole terminal, scrollback included.
- `Find in scrollback…` opens the find bar.
- `Save a scene…` opens Save a scene in Settings on the newest log of the session in front, with its last 15 minutes. It does nothing while `Log sessions` is off for the profile, because that profile saves nothing to share.
- `Settings` opens a list beside the menu. `Triggers`, `Aliases`, `Macros`, and `Timers` open Settings under Automation on that list. `General`, `Appearance`, `Accessibility`, `Layout`, `Vitals`, `Prompt`, `Input`, `Automation`, `Scripts`, `Logs`, and `Characters` open that page of Settings. `Help` opens the Help window.
- `Clear scrollback` empties what you can scroll back through. Vosh restores none of it at your next launch. Your session log keeps every line.

Items with a shortcut show it on the right, and `Settings` shows an arrow. The arrow keys move through the menu, and `Enter` chooses an item. `ArrowRight` or `Enter` on `Settings` opens its list on the first row. `ArrowLeft` goes back out. When you point at `Settings`, the list also opens. `Esc` closes the list first, then the menu.

The menu also closes when you click anywhere outside it, and at once when you choose an item. It always stays inside the window, so a right click near a corner never opens it half off the screen. Near the right edge, the Settings list opens on the left of the menu. Near the bottom, it opens upward from its row.

### 2.9 Write your description

<!-- id: play.write-description -->

The writing card of Vosh helps you write the description that others see when they look at you. It also sends the description to the game for you.

- Right click the terminal and choose `Write`, then `Your description…`. Or type `desc` in the command palette (`Cmd+K`) and choose `Edit your description…`.
- When there is no draft, the card reads your description from the game when the prompt of the game shows. The answer of the game prints in the terminal under the card.
- The box keeps every line to 75 columns, as help description asks. A paragraph flows as you type, and a break you make with `Return` stays. Text past column 75 shows in red. While the caret is on that line, the footer offers `Rewrap paragraph`.
- The footer counts your lines with text against the ten to thirty lines that the help asks for. It counts the empty lines separately.
- A paste wraps each long line at 75. It changes curly quotes, long dashes, and the ellipsis to the plain ones that the game keeps. The footer tells you what changed, and `Undo` puts it back.
- `Guide` shows the reminders of the help beside your text. `Read help description` asks the game for the help itself, and the card folds to its header.
- `Send to game` sends your text through the editor of the game, one line at a time. It checks off each line that the game takes. Vosh then checks what the game holds and corrects each line that is different. Then it leaves the editor and reads your description back.
- While Vosh sends, your triggers, timers, Lua, and `#walk` wait, and a chip at the command line counts them. A line you type still goes at once, so you can act in a fight.
- Your description doesn't need approval to change. Send it to the game as often as you want. When you're ready, choose `Send for approval…` in the `⋯` menu of the card. It sends `dcheck` after you confirm. The game takes one check at a time.
- A check that waits keeps the text you sent with it. So when you send again, the footer reminds you. The game doesn't always tell you when the immortals decide, so the footer says it with an if.
- A werebeast of level 15 and up gets a `Beast` switch beside the title for the beast description.
- To put the card anywhere in the window, drag it by its header. To put it back over the terminal, double click the header or choose `Put the card back` in the `⋯` menu.
- To make the box taller or shorter, drag the grip on the edge of the card. The grip is along the top while the card is over the terminal. It is along the bottom after you move the card. Double click the grip, and the box grows with your text again.
- To make the box taller or wider, drag the grip in its bottom right corner. It works as in any text area. The box never gets narrower than 75 columns. Text past column 75 still shows at any width. Double click the grip, and the box goes back to 80 columns and grows with your text.
- The pin beside `Close` moves the card into a Writing pane in the panel, so you can see the whole terminal. Click the pin again to float the card over the terminal. Vosh remembers where you put the card, the size of the box, and whether you pinned it.

Vosh never writes, rewrites, or suggests a word. The red underlines come from the spell check of your system. `Check spelling` in the `⋯` menu of the card turns them off.

Your drafts are in writing.toml in your data folder, one for each character on each world. They save as you type. You can close the card at any time, and your draft waits for you. With no connection the card still opens, and `Send to game` waits for a session that plays the character.

When you type `description edit` yourself, the game opens its own editor as usual. A notice then offers `Write this in Vosh?`. `Open in Vosh` leaves the editor of the game with no change and opens the card on your text. `Keep typing` keeps you in the editor of the game, where each line you type goes out as you type it.

The command line then names your text and the line you're on, such as `Description · 4 of 30`. Past thirty, it shows in amber. The tick still marks column 75, and text past it gets a wash. Click the name to choose `Open in the writing card` or `Finish`. `Open in the writing card` opens the card on your text. `Finish` sends `@` and closes the editor of the game.

You can also type `@` on a blank line to finish. To turn the notice off, use `Offer the card when the game's editor opens` in Settings under Input.

### 2.10 Write in the game

<!-- id: play.write-in-the-game -->

The writing card also writes on the boards of the game. Notes, journal entries, applications, ideas, and bug and typo reports all open in it. So do your history, personality, and purpose.

- Right click the terminal and choose `Write`, then the kind you want. Or find it in the command palette, such as `Write a note…` or `Report a bug…`. The title of the card opens your drafts and every other kind.
- A note has `To` and `Subject` above its text. A journal entry, idea, bug, or typo goes to the immortals, so `To` reads `Immortal`.
- A bug or typo report names the room you stand in. The game records it when you post. So post the report from the place where the bug happened.
- `Write in a language` in the `⋯` menu adds a `Language` row to a note. The game decides whether you know the language well enough.
- The game never rewraps a note, so readers see your lines as you break them. The card keeps them to 75 columns. For a custom race application it keeps them to 70, as help qrace asks.
- The guide of an application reads your subject as the game does. It shows the help for that subject, such as help psi requirements. For a custom race, choose `Custom race application` in the `⋯` menu.
- `Post…` asks first, because you can't change a note after you post it. To skip the question, turn on `Don't ask again` there, or turn off `Ask before you post` in Settings under Input. The button then reads `Post` and posts at once. A bug or typo report that you started in another room still asks, because the game records the room you stand in now.
- When you post, Vosh sets `To` and `Subject` and sends your text through the editor of the game. Then it reads the text back. It posts only when the game holds the text as you wrote it. When the game says no, its reason prints under the card and your draft stays.
- The game holds one note at a time. When you started a note in the game yourself, Vosh keeps it in your drafts. It asks before it clears it.
- If you're disconnected during a send, the card shows how far it got when you're back. `Post again` starts again from the beginning. When the drop came during the post, Vosh checks the list of the board first. It never posts again on its own.
- Just before it posts, Vosh lists your own notes on that board, so you see the list in the terminal. If your connection drops at that moment, Vosh compares the board with that list. So an older note with the same subject is never taken for the new one.
- Your history, personality, and purpose share one card, with a switch beside its title. `Send to game` saves each one in the game. `Send for review…` in the `⋯` menu sends your history to the immortals, one time. When you send your history again later, the footer reminds you of one thing. A check that still waits reads the history you sent at that time.

You can keep as many notes open as you want, and each one saves as you type. Each post moves to `Sent`. There Vosh keeps your last 20 posts for each character. So you can still read a bug report that the game won't show you again.

For any kind, you can move the card, change the size of its box, and pin it to the panel, as Write your description shows.

When you type `note edit`, `history edit`, or another opener yourself, you get the same `Write this in Vosh?` notice as for your description. When you keep typing in a note, the command line reads like `Note · line 4`, because a note has no line limit.

The card doesn't take tomes, cabal votes, paper, or the description of your pet yet. When you open one of them with `scribe text`, `vote edit`, `write edit`, or `petedit desc`, no notice comes. The command line still marks column 75, and reads like `Tome · line 4` as you type.

### 2.11 Walk to a place

<!-- id: play.walk -->

`#walk` moves you along a string of directions, one step at a time. Vosh waits for the game to show each new room before it sends the next step. So a move that fails stops the walk where you stand.

- Type `#walk 3n2e` to go north three times, then east two times.
- Use `n` `e` `s` `w` `u` and `d`. The game has no diagonal exits, so `ne` walks north, then east.
- To do a direction more than one time, put a count from 1 to 99 before it. You can put spaces between the parts.
- The walk stops when a move fails, when a fight starts, when you stop standing, or when you send the game a command. To stop it yourself, press `Esc` or type `#walk stop`.
- During a walk, the command line shows `Walking · 2 steps left` beside the chip of the map. Other `#` commands don't stop the walk.
- An alias or a macro can run `#walk`. So `#alias bank #walk 3n2e` walks you there by name. Commands after `#walk` in the same alias wait until you arrive. If the walk stops early, Vosh drops them.
- To walk to a room on the map, click it. Vosh first shows the steps as a `#walk` string. A new click or `#walk` during a walk takes over when the current step is done.

| You type     | Vosh sends                                                        |
| ------------ | ----------------------------------------------------------------- |
| `#walk 3n2e` | `n n n e e`, each after the room before it shows                  |
| `#walk 2w u` | `w w u`                                                           |
| `#walk ne`   | `n e`                                                             |
| `#walk 3x`   | Nothing. Vosh says it cannot read `x`.                            |
| `#walk`      | Nothing. Vosh says how many steps are left, and the walk goes on. |

## Automate

### 3.1 Create an alias

<!-- id: automate.first-alias -->

An alias expands a short name into one or more commands. Aliases are in Settings under Automation, then Aliases. You can also make them in the command line.

- Open Settings, choose Automation, and choose `Aliases` in the switcher at the top.
- Click `New alias` in the bar at the bottom.
- Type a name in `Name`.
- Type the expansion in `Expansion`. `;` splits the expansion into separate commands. `\;` keeps a literal semicolon.
- Click `Save`. The bar shows `Saved`.

Captures take words from the line you typed. `%1` through `%9` take the first through ninth word after the alias name. `%0` takes all the words after the name. `%1-` takes word one through the end, with the spaces kept. A missing word expands to nothing. `%%` gives a literal percent.

To turn related aliases on and off together, give them the same name in `Group`. Then use the switch on the heading of their group, or `#group <name> on|off`. Under `Advanced`, `Run Lua instead` runs a Lua script in place of the expansion. The words you typed are in its captures table.

Triggers, Aliases, Macros, and Timers each list your items under a heading for each group. Presets lists them under a heading for each category. The items with no group are at the top, under no heading. Click a heading to fold its group, and click it again to open it. The chevron points down while the group is open. A folded heading shows how many items it holds.

When a heading has the focus, `ArrowLeft` folds it and `ArrowRight` opens it. `ArrowUp` and `ArrowDown` move through the headings and items as one list. Each list remembers the groups you fold. When you type in the filter, every folded group with a match opens until you clear the filter. When you choose an item from the matches, its group stays open.

The switch after a group heading turns the whole group on and off at once, the same as `#group`. Each item keeps its own `Enabled`. A group is on or off for its whole profile. So the switch and `#group` reach every session that plays the profile. The switch acts at once, with no `Save`. A group that you just named gets its switch after you save it.

`Tab` from a heading goes to its switch, and `Space` turns the switch on or off. A timer also takes a `Group`. A timer in a group that is off waits. When the group comes back on, the timer starts a full interval.

In loadout mode, the loadouts decide each group of triggers, aliases, and macros. This is true while an active loadout lists groups, or while you keep the catalog dormant. The switch still turns such a group on or off, and so does `#group`. A note under the heading names the loadouts that decide the group, or says every loadout is off. After you turn the group on or off yourself, the note tells you more. The loadouts set it back when you next launch Vosh, change profiles, or save Loadouts.

Example. An alias named `kk` with the expansion `kick %1; backstab %1` changes `kk dragon` into `kick dragon` and then `backstab dragon`.

You can also make aliases in the command line. `#alias gc get all corpse` sets one and echoes `alias gc set`. `#aliases` lists every alias, and `#unalias gc` removes one. When you set an alias again with `#alias`, `#endrec`, or `mud.alias` in Lua, it stays in its group.

### 3.2 Create a trigger

<!-- id: automate.first-trigger -->

A trigger watches the lines that come in and runs actions when a pattern matches. Triggers are in Settings under Automation, then Triggers. A trigger has one visual and any number of effects.

- Open Settings and choose Automation, then Triggers.
- Click `New trigger`.
- Type a name and a pattern, and choose how the pattern matches. `Text` matches a line that is exactly the pattern. `Starts with` matches any line that starts with the pattern. `Regex` reads the pattern as a regular expression.
- To add another pattern in the same mode, open `Advanced` and click `Add pattern` in More patterns. The trigger fires when any pattern that is on matches.
- Leave `Priority` under `Advanced` at `5`, the default for a new trigger. Or make it higher to run before other triggers. Triggers with a higher priority run first. Leave `Match` on `Lines`.
- Choose `Room` in `Match` to match only the armies, things, and people that a room lists after its exits line. The game sends `Room.Chars` and `Room.Items` packets with each look. Vosh counts the lines from them. So a say or an arrival after the look stays a plain line.
- Choose `Your target` in `Match` to match only the line of the one you target with `tar`. This works when a room lists them. Vosh finds that line by the place of your target in the room. That is the place that `tar` marks with `>`. So `tar 3` finds the third person even when their line gives the name in another way. When more than one person in the room fits what you gave `tar`, the first of them is your target. So one line matches.
- Choose a `Style`. The choices are `None`, `Highlight`, `Wash`, `Replace`, and `Hide`.
- Put a command in `Then send`. `Send to pane` and `Lua script` are under `Advanced`. Send and replace templates get capture groups with `$1` through `$9` or `${name}`. `;` splits a send into separate commands.
- To make the trigger get your attention when it matches, click `Banner`, `Sound`, or `Bounce` in `Alert`. Each one turns on and off by itself. `Banner` posts a system banner. `Sound` plays a chime. `Bounce` bounces the Dock icon.
- On Windows `Bounce` reads `Flash` and flashes the taskbar. On Linux it reads `Mark` and marks the window as needing you. At first an alert rings only while you aren't looking at its session.
- To tune the alert, open `Advanced` and use the four rows at the end of the card. `Sound` chooses `Chime`, `Bell`, `Knock`, or `Low`. The play button beside it plays the tone it shows. `Bounce` chooses `Once` or `Until you return`. It reads `Flash` on Windows and `Mark` on Linux.
- `Banner shows` starts on `Title only`. `Title and words` adds what was said. `Only while you are not looking at its session` starts on, so the alert rings only while you look somewhere else. Turn it off, and it also rings while you watch. A choice in `Sound` or `Bounce` turns that part on.
- For a tell, your name, a fight, low health, or the connection, use an alert preset instead. Get alerts at 3.9 shows how.
- Click `Save`. Vosh shows `Saved` in the bar at the bottom.

`Text` and `Starts with` ignore spaces at the start of the line. `Text` also ignores them at the end. So a line you copy from a look matches with or without the five spaces before it. Neither mode needs escapes, and neither fills `$1`. A `Regex` pattern needs escapes for literal punctuation. Its groups fill `$1` and on.

A new trigger starts in `Text`, and older triggers read as `Regex`. The mode covers every pattern of the trigger. In `Edit all as JSON…` a `Text` or `Starts with` row reads `text`. So if you change only its `pattern`, there or by hand in the file, nothing changes.

Example. To match the line `You feel better.`, type `You feel better.` in `Text`, `You feel better` in `Starts with`, or `You feel better\.$` in `Regex`. Choose `Regex` for the pattern `(\w+) is DEAD!`. Then a send of `get all corpse` loots each kill when the death line comes in.

A preset adds its triggers under `From presets`. You edit a preset trigger as you edit your own, every row but `Name`. Vosh finds the trigger in its preset by that name. The note at the top names the preset. Click its name to open its card in Presets.

Each row you changed says `Changed` and shows what the preset has. `Advanced` counts the rows under it that you changed. A pencil marks the trigger in the list. Vosh keeps only the rows you change, so a fix that Vosh ships for the preset still reaches the other rows. Highlight lines at 3.3 tells more.

A preset trigger has no `Delete`. To stop it, turn off `Enabled`. `Reset to preset` under the card puts back the rows of that trigger. Like every change, it waits for `Save`. A trigger of your own can't have the name of a preset trigger, whether that preset is on or off. `Edit all as JSON…` lists only your own triggers.

You can also make triggers in the command line. `#trigger name {pattern} send command` makes one with a `Regex` pattern at priority 0 on the `line` target. `#triggers` lists all of them by priority. `#untrigger name` removes one. Vosh refuses a regex that isn't valid and names the broken pattern.

### 3.3 Highlight lines

<!-- id: automate.highlight-lines -->

A highlight trigger changes the style of every line that matches a pattern. Make one in the command line with `#trigger`, or in Settings under Automation, then Triggers.

- Type `#trigger <name> {pattern} highlight <color> [styles]`. Every line that matches the pattern shows in that color and style.
- To tint the whole line and not only the text, add `wash` to the list of styles.
- To check the pattern and the action, type `#triggers`. A new trigger with the name of an existing trigger replaces it.

A plain highlight changes the style of the matched words. The rest of the line keeps the colors that the game sent. A wash marks the whole line. The text of the line takes the highlight color. A dim field in that color fills the row from edge to edge. The field follows the palette of your theme, so a washed line goes well with the colors around it.

Colors take the sixteen ANSI names. They are `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, and `white`, plus a `bright_` variant of each. `purple` gives magenta and `gray` gives `bright_black`. You can use `bold`, `underline`, and `inverse` together. Add `bg:<color>` for a background.

Example. `#trigger tell-glow {tells you} highlight bright_yellow bold` shows every tell in bright yellow and bold. `#trigger tell-glow {tells you} highlight bright_yellow wash` replaces it with a wash on the full line.

The Triggers editor in Settings under Automation has the same options. Choose `Highlight` or `Wash` in `Style`. Then open `Advanced` to set `Text color` and `Background`, with `Bold`, `Underline`, and `Inverse` beside them.

Settings under Automation, then Presets, holds colors for the lines the game prints. Each card shows a sample under `Looks like`. Vosh draws it in your theme, as the terminal draws it. While a preset for the game you connect to is off, it has a ring in the accent color. Its card names the game under `Suggested`. Turn a preset on or off and click `Save`.

A preset that colors lines shows a swatch under `Colors` for each color it uses. Each swatch has the name of what it marks, such as `The line` or `The damage verb`. A swatch colors every trigger of the preset that uses its color. Most swatches take any color. While you leave one empty, it shows the color of the preset, such as `Theme red`. A swatch with a color in a highlight takes one of the sixteen colors of your theme.

`Looks like` draws the sample again in your colors. A swatch you changed says `Back to` and shows the color of the preset under it. Click there to put that color back.

A preset you changed has a pencil beside its dot in the list. Its card ends with `Your changes`. This names each color and trigger you changed, or counts them when there are more than two. Click a trigger there to open it in Triggers. `Reset to preset` under the card takes back every color and trigger you changed in that preset. A swatch and a reset wait for `Save`, and `Discard` brings your changes back.

When you turn a preset off, your changes wait for it. So it comes back on in your colors. `Reset to preset` works while the preset is off. A description keeps the words of the preset. So a description that names a color still names the color that the preset ships.

Vosh keeps only what you change. It puts your changes over the preset each time a profile opens. So a fix that Vosh ships later still reaches the parts you didn't change. When a fix changes a row you changed, your change stays. The trigger card says that a fix changed a row you edited. The row shows what the preset now has, with `Take the fix` and `Keep mine` under it.

A swatch with a color that a fix changed has a warning ring with the same two choices. Your choice waits for `Save`. A change that the fix now matches goes away on its own. At the launch that finds a fix, a notice in the corner says `A preset fix changed a row you edited` and names the trigger. `Show` opens it in Settings. `Close` hides the notice and keeps the marks.

When a fix removes a trigger you changed, the notice says `A preset fix removed a trigger you edited`. Vosh tells you about each fix one time.

The `Room, time and weather colors` preset colors a room look, the clock, and the weather. The exits line turns green. The armies, things, and people that the room lists turn yellow. The day and night messages turn blue, and the WiZNET tag turns bold magenta. The one you target with `tar` turns bright red when the room lists them, so your target is easy to see in the room.

That red is the `room.target` trigger. To keep your target yellow, turn off its `Enabled` in Triggers. Or choose another color for `Your target` on the card of the preset. Each of these colors is a terminal color from your theme, so the colors change when you change the theme.

A change in the weather turns pale blue, such as `It starts to rain.` or `A thick fog rolls in, shrouding the area.` That blue is `#8fa7d9`. It is a color of its own, separate from the blue and cyan of your theme. It is readable on every built in dark theme. On a light theme, `Keep highlight colors readable` under Accessibility makes it darker until it is readable.

The exits, room, and target colors fill only the text that the game left with no color. So an aura, a red `[AFK]`, and the red `+` of a trap you see keep their own colors. The magenta covers only the WiZNET tag, so the message after it also keeps its colors. A say or a tell that quotes the same words stays as it was.

Vosh turns the preset on for every profile, one time, unless you had turned every preset off. A new install starts with every preset off. Turn it on or off in Settings under Automation, then Presets.

### 3.4 Route lines to a pane

<!-- id: automate.route-chat -->

A route effect sends matching lines to a named pane. Add one to a trigger in Settings under Automation, then Triggers.

- Open Settings, choose Automation, then Triggers, and click `New trigger`.
- Type a name.
- Type a pattern. To add another pattern, open `Advanced` and click `Add pattern` in More patterns. The trigger fires when any pattern that is on matches.
- Under `Advanced`, type the name of the pane in `Send to pane`, such as `chat`.
- Click `Save`.

Route is an effect, so you can use it with anything else on the trigger. Use it with the `Highlight` style to color the line. Or put a command in `Then send` beside it.

Matching lines go to the Chat pane, tagged with the pane name you gave. To show the Chat pane, use `Add a pane` in the title band or `Show chat` in the View menu. To read only those lines, choose that name in the channel select of the pane.

Example. A trigger named `chat-feed` with the patterns `tells you '` and `gossips '` and a route to `chat` collects tells and gossip in the chat pane.

In the command line, type `#trigger chat-feed {tells you '} route chat`. This makes a trigger with a single pattern. So make feeds with more than one pattern in Settings under Automation.

### 3.5 Set and use variables

<!-- id: automate.variables -->

A variable stores a value that you use in commands as `$name`. Set variables in the command line with `#var`. Vosh expands them in the lines you type, before the lines go out.

- To set a variable, type `#var <name> <value>`. Vosh echoes `var <name> set`.
- To use it in any command, type `$name`. The line expands before it goes out, so the server gets the value.
- To check a value, type `#var <name>`. To list all variables, type `#vars`. To remove one, type `#unvar <name>`.

`$name` works when the name ends at a space or at punctuation. When letters come right after the name, put it in braces, as `${name}`. `$$` sends a literal dollar sign. Unknown names go out as they are, so `$100` gets to the server as you typed it.

Vosh expands variables in the line you type, before it expands aliases. It doesn't expand variables in the output of an alias again. So put variables in the line you type, or get their values in a Lua script body.

`#var` writes in session scope. Session scope clears when the next connection opens, so a session value never lasts longer than its connection. Profile variables stay across restarts in your profile TOML under `profile_vars`. A session value hides a profile value with the same name. Each session keeps its own session variables, and the sessions on one profile share its profile variables. `#unvar` removes the name from both scopes, so the profile value goes away for every session on the profile.

Vosh also fills session variables on its own. GMCP sets `hp`, `maxhp`, `char_name`, `room_name`, `target_name`, and more. When you set a target with `tar`, Vosh also puts it in `$target`.

Trigger send templates use `${name}` for regex capture groups, not for this store. Trigger sends don't expand variables at all.

Example. `#var potion yellow` and then `quaff $potion` sends `quaff yellow` to the server. When a target is set, `cast dispel $target` aims at your current target.

### 3.6 Bind keys to macros

<!-- id: automate.macros -->

A macro binds a key to a command. It fires while the command line has the focus. Macros are in Settings under Automation, then Macros.

- Open Settings, choose Automation, and choose `Macros` in the switcher at the top.
- Click `New macro` in the bar at the bottom.
- Click the `Key` field. It reads `Press a key` until you press one.
- Press the key you want. The field records its canonical name. It takes function keys, combinations with modifiers such as `Ctrl+N`, numpad keys such as `Numpad7`, and plain printable keys.
- Type the command in `Command`. `;` chains more than one command.
- To turn the macro on and off with others, put a name in `Group`. Then click `Save`.

A macro fires only while the command line has the focus. On macOS the `Cmd` shortcuts belong to Vosh, and `Ctrl` belongs to your macros.

To show what each press sent, turn on `Show the commands your macros send` in Settings under Input, then Sent commands. `#group <name> on|off` turns a whole group of macros on and off from the command line. It also turns the alias, trigger, and timer groups with the same name on and off.

To walk with the numpad, turn on `Numpad movement` in Settings under Automation, then Presets. It adds six macros under `From presets` in Macros. There you can change only their group. `Numpad8` sends `n`, `Numpad6` sends `e`, `Numpad2` sends `s`, `Numpad4` sends `w`, `Numpad9` sends `u`, and `Numpad3` sends `d`. The game has six directions, so `Numpad7`, `Numpad1`, and `Numpad5` stay free. Vosh reads the key itself, so NumLock has no effect and the digit row still types.

When one of your macros uses a key, the key stays yours, and the macro of the preset on it waits. Both macros tell you so in Macros, where a ring marks yours. The card of the preset marks the key. The direction gets the key after you move or delete your macro.

In loadout mode, a macro of yours in a group that your character keeps off leaves the key to the preset. So a character whose `Numpad3` went down still goes down after another character brought its own `Numpad3` to the shared catalog. When you turn the preset off, Vosh removes its six macros and none of yours.

Example. Bind `F1` to `stand; flee`. When you press `F1` in the command line, Vosh sends both commands.

`#record` does something different. It records the commands you type in its session and saves them as an alias. You use the alias by its name, not by a key. The alias goes into the profile, so every session on the profile has it. Use Automation, then Macros when you want a key. Use `#record` when you want a word.

### 3.7 Use slash commands

<!-- id: automate.slash-commands -->

Slash commands control Vosh from the command line, without Settings. Vosh handles every line that starts with `#` itself, and the line never goes to the MUD.

- Type `#help` at any time for the full list, or `#help <words>` to open Help on those words.
- Manage aliases with `#alias <name> <expansion>`, `#unalias <name>`, and `#aliases`.
- Manage variables with `#var <name> [value]`, `#unvar <name>`, and `#vars`.
- Manage triggers with `#trigger <name> {pattern} <action>`, `#untrigger <name>`, and `#triggers`.
- To tell Vosh how to read your prompt, use `#prompt game {setting}` and `#prompt fight {setting}`, with the codes you type in the game. Or use `#prompt {regex}`, where each named group such as `(?<hp>\d+)` is a value. `#prompt` alone tells you how Vosh reads your prompt. `#unprompt` stops it.
- To turn the drawing of your design on or off, use `#prompt draw on|off`. With drawing off, you see the prompt of the game, and your design stays. With no design of your own, Vosh draws your prompt as the game does. It follows each change you make to the prompt in the game.
- To choose where your prompt shows, use `#prompt show text|lifted|pinned`.
- To use the default prompt design of Vosh, use `#prompt default`. It replaces the design in this profile. Vosh keeps your design as an earlier design.
- Turn whole groups on and off with `#group <name> on|off`, and see them with `#groups`.
- Tune the tick with `#tick`, `#tick interval <secs>`, `#tick warn at <secs>`, and the other commands that `#help` lists.
- To check how long the game takes to answer, use `#lag`. It also lists each stall since you connected.
- Record a sequence of commands with `#record <name>`. Finish with `#endrec`, or stop with `#record cancel`.
- Set quick keys with `#qkey <name> <verb>`, and list them with `#qkeys`.
- Control Lua with `#script load <name>`, `#script reload`, `#scripts`, and `#lua <code>`.
- Make and use snapshots with `#profile save`, `#profile load`, and `#profile reset`.
- Import TinTin++ files with `#import-tintin <path>`.
- To remove old passwords from your session log, use `#logs forget-passwords`, then `#logs forget-passwords now`.
- Work with targets with `#target <args>`. Or use `tar`, `tarn`, `tarp`, and `tarclear` with no `#`.
- On macOS, change the renderer with `#nativesurface on|off|default`. The change applies when you restart.

An unknown command echoes a pointer to `#help`. Errors come back in square brackets.

### 3.8 Script Vosh with Lua

<!-- id: automate.lua-scripts -->

Lua scripts run inside Vosh. They add automation through the global `mud` table. Script files are in the `scripts` folder in the app data folder. On macOS that is `~/Library/Application Support/com.aabahran.vosh/scripts/`.

- Save a `.lua` file in the `scripts` folder.
- To load it, type `#script load <name>`. Vosh adds `.lua` to a bare name, so `combat` and `combat.lua` load the same script.
- To see the loaded scripts and the triggers they added, type `#scripts`. Each trigger shows the script that made it.
- After you edit a file, type `#script reload`. Vosh reads every loaded script and plugin from disk again. It runs them in the order they first loaded. An error in one script stops none of the scripts after it.
- Run one line of Lua with `#lua <code>`.

Scripts talk to Vosh through the global `mud` table. `mud.send(text)` goes directly to the server. `mud.input(text)` goes through the input pipeline again. `mud.echo(text)` prints locally. `mud.alias(name, expansion)` and `mud.trigger(name, pattern, callback)` add automation. In the callback, `captures[1]` holds the full match, and `captures[2]` and on hold the groups.

`mud.on_gmcp(package, callback)` gives you server data as a table. `mud.timer(secs, callback)` schedules work that you can cancel with `mud.cancel_timer`. A plugin can draw a pane of its own with `mud.pane`, as Make a pane with Lua at 3.10 shows.

`mud.alert(title, options)` posts a banner with the title while you aren't looking at the session that runs the Lua. These are the `options`.

- `sound = 'chime'`, `'bell'`, `'knock'`, or `'low'` plays that tone.
- `attention = 'once'` bounces the Dock one time, and `'until'` bounces it until you come back. On Windows it flashes the taskbar.
- `background = false` also rings while you look.
- `words = true` with a `text` adds a line under the title.

One title from one script rings at most one time in 10 seconds. On macOS, when you turn a plugin off, Vosh removes the banners it posted. macOS shows a banner only after you let Vosh post banners. Vosh asks for this the first time you turn on a `Banner` in Settings, as Get alerts at 3.9 shows. Banners also need a signed Vosh. So a dev build that you run from the source shows none.

Each script and each plugin owns the triggers, GMCP handlers, and timers it adds. This includes those that its callbacks add later. When you load it again with `#script reload` or `#script load`, Vosh removes all of them, if the script runs with no error. So nothing doubles, and a trigger you deleted from the file goes away. A load with an error keeps what the script had. Variables it set and groups it turned on or off stay.

Two scripts can each have a trigger with the same name. A new `mud.on_gmcp` handler runs at once on the last packet of its package. So it sees your `Char.Status` and doesn't wait for your next login. A new `Comm.Channel` handler waits for the next message instead. This is because each chat packet is one message and not a state.

A script you load with `#script load` stays loaded until you close the session or quit Vosh. Only that session runs it. For a script that loads automatically, make a plugin. To see your plugins, open Settings and choose Scripts.

`New plugin` asks for a name of letters, digits, and underscores. It makes a folder with that name in `plugins` in the app data folder, with a `manifest.toml` and a `main.lua`. It turns the plugin on for the profile of the session in front and opens the page of the plugin.

The switch on each row turns a plugin on or off for that profile. The plugin starts or stops at once in each session that plays the profile. A plugin that Vosh stopped reads `Stopped` there. A plugin folder that you named by hand with other characters, such as `weather-pane`, still loads and shows there. Its row asks you to rename the folder. Until you do, its switch can only turn it off, and its page doesn't open.

Every plugin that a profile turns on loads in each session that plays the profile, when the session opens it. When you change profiles, the plugins of the next profile turn on and the others turn off. A plugin that both profiles list keeps running. A switch in one session changes the plugins of that session only.

To open the page of a plugin, click the plugin under Scripts. The editor holds the file that the plugin runs first. `Manifest` holds its version, its author, its description, and `Runs first`, the file it loads first. `Save and reload` writes both to the plugin folder. It loads the plugin again at once in every session whose profile turns it on. `Discard` puts back what you saved last.

Vosh asks before you leave the page or close Settings with changes you haven't saved. `Show in Finder` under `Manifest` opens the plugin folder. It reads `Show in Explorer` on Windows and `Show the folder` on Linux.

Each plugin row has a menu of its own. `Reload` reads the plugin from its folder again. It loads the plugin in every session whose profile turns it on. So edits you make in another editor take effect. It also ends a stop.

`Show in Finder` opens its folder. `Export to Downloads` saves the plugin as a `.zip` in your Downloads folder, so you can share it. The line under the list names the file. `Remove…` asks first. Then it deletes the plugin folder and turns the plugin off in every profile.

`Install…` takes a `.zip` that a friend shared. You can also drop a plugin folder or a `.zip` on the Scripts list. Vosh names the plugin, its version, and its author, and asks one time before it installs. A plugin can send commands to the game and read everything the game sends. So install plugins only from people you trust.

A new install of a plugin starts off for every profile. When you install over a plugin with the same name, it replaces that plugin and turns it off everywhere. So new code never runs until you turn it on. Vosh refuses a plugin with no `manifest.toml`, a plugin with a file outside its own folder, and a plugin over 5 MB or 200 files. It tells you why above the page.

Each plugin runs in its own environment. Its globals and its `mud` table are its own, so two plugins never overwrite each other. A plugin can read the standard libraries, such as `string` and `table`, but it can't change them. It starts from new globals each time it loads. A line that it gives to `mud.input` runs no `#` command except `#echo`.

An alias that a plugin makes lasts while the plugin runs, and Vosh never saves it. It works only in the session whose plugin made it. It replaces your own alias with that name until the plugin turns off. When you turn a plugin off, Vosh removes its aliases and all else it added.

Your `#lua` lines, the Lua in your triggers and aliases, and scripts from `#script load` share one set of globals in each session. An alias that they make is one you keep, and every session on the profile runs it. They get to the globals of a plugin through `plugins.<name>`. This is a view that you can read but not change. An example is `plugins.helpers.rescue("Orla")`, which calls a function that the plugin helpers defines.

Every Lua error and every `print` shows in the terminal of its session after a gray `[lua]` tag. An error names its place, such as `combat.lua:3:` for line 3 of `combat.lua`, and shows in red. What a plugin prints as it loads in a session shows after you connect or type a line there.

The Console under Scripts in Settings shows the same lines for the session in front, each with its time. It runs the Lua you type in its field in that session, as `#lua` does. `Clear` empties the Console and doesn't change the terminal.

The page of a plugin shows the lines of that plugin under `Output`. Its field runs Lua inside the plugin, where it sees the globals of the plugin and its own `mud` table. The line that its newest error or stop names since it last loaded shows tinted in the editor. Point at it to see the error.

Lua runs between the lines the game sends, so Vosh keeps each call short. It stops a call that runs longer than 100 ms. It also stops a call that uses 32 MB more than it started with, or that takes your scripts past 128 MB in total. `pcall` can't catch the stop. The time limit also works inside string patterns and the `table` functions. So a pattern that backtracks over a long line stops like a loop.

A stopped call sends nothing that it queued, and a red `[lua]` line tells you what Vosh stopped. A plugin then stays off until you save it under Scripts in Settings or restart Vosh. A script from `#script load` stays off until `#script reload`. A trigger or alias whose Lua ran too long stays off until you save it or restart Vosh.

Each one stays off only in the session where Vosh stopped it. Every other session on the profile keeps running it. The page of a plugin that Vosh stopped tells why above its editor.

Each plugin and each script from `#script load` also gets 100 ms in total for each of these. That is one game line, one packet, the last packets that its new handlers get, or one round of timers that are due together. When it has used them, Vosh skips the rest of its triggers and handlers for that line or packet. It holds the rest of its timers for a quarter second. A red `[lua]` line tells you so.

One call can queue up to 100 actions, such as sends and echoes. Vosh drops the rest and shows a line that tells you so. A line to send holds 1 KB at most, and an echo holds 64 KB. One call holds 256 KB of text in total. For one game line, packet, timer, or line you type, Vosh runs 100 `mud.input` lines at most. This includes the lines that their own Lua asks for.

The sandbox removes access to files, processes, and the environment. `require`, `io`, `os.execute`, `os.getenv`, and `os.setlocale` aren't there. Vosh refuses a `__gc` method, because it runs where Vosh can't stop it.

`#script load` reads only from the `scripts` folder. `mud.input` can't run `#script load`, `#script reload`, `#import-tintin`, or `#profile`. These run only when you type them. `mud.input` also can't set a quick key or the tick command to a `#` command.

Example. `#script load combat` loads `combat.lua` from the scripts folder. `#lua mud.echo("hello")` prints a line locally.

### 3.9 Get alerts

<!-- id: automate.alerts -->

Alert presets get your attention when the game needs you. They work while you play another session or work in another app. They are in Settings under Automation, then Presets, under `Alerts`.

- `Tells you get` rings when someone sends you a tell. The banner reads `Tell from` and their name.
- `Your name` rings when a line from the game names you, as a whole word with its capital letter. It waits until the game tells Vosh who you are after you log in. Lines that start with `You` don't ring.
- `Being attacked` rings when someone starts a fight with you. It doesn't ring when a groupmate other than you tanks the fight. It also doesn't ring when you sent a command in the 2 seconds before, because then you most likely started the fight.
- `Low health` rings when your health falls under 20 percent. It rings again only after your health goes back up to 25 percent and falls again. It never rings while the game hides your vitals.
- `Connection` rings when your connection to the game drops while you play, when a reconnect gets to the login, and when Vosh stops trying.

All five start off. To turn one on, use its switch and click `Save`. Each one starts with `Banner` on in its `Alert` row. You turn `Sound` and `Bounce` on and off as on a trigger. The rows for the parts that are on show under it.

`Banner shows` is there only for Tells you get and Your name. There, `Title and words` adds what was said or the line that named you.

`Only while you are not looking at its session` starts on. So a preset rings only while you look at another session or another app. Turn it off, and it also rings while you watch. A preset with an `Alert` row that you changed has a pencil in the list. `Reset to preset` on its card puts the row back as the preset ships.

While Vosh is in front and you look at another session, an alert from a session behind shows a notice at the bottom right. An example is `Tell from Maren` with `to Tolliver` beside it. Click `Show` to go to that session. `Close` removes the notice. The count stays on the row of that session in the sidebar until you look there.

The first time you turn on a `Banner`, on a preset or on a trigger, Vosh asks before macOS does. Click `Continue`, and macOS asks if Vosh can post banners. `Not now` keeps `Banner` on, and Vosh doesn't ask again until you close Settings.

If banners from Vosh are off in System Settings, `Banner` has a warning ring on every `Alert` row. Each alert preset also tells you so at the top of its card. `Sound` and `Bounce` still work.

`Open notification settings` takes you to the page where you turn banners back on. The ring goes away when you come back to Settings. On Windows the note names Windows Settings and `Flash`. A dev build that you run from the source shows no banners, so it asks nothing and shows no ring.

Each preset rings at most one time in 10 seconds, so a burst rings one time. Tells you get counts each sender separately. So three tells from Tolliver ring one time, and a tell from Maren still rings. Connection counts the drop, the login, and the stop separately, so each one rings.

To ring on a line that you choose, use a trigger. Turn on a part in its `Alert` row, as Create a trigger at 3.2 shows.

### 3.10 Make a pane with Lua

<!-- id: automate.lua-panes -->

A plugin can draw its own pane. You send Vosh rows, gauges, and lines, and Vosh draws them in the style of the pane. Every value shows as plain text.

```lua
-- weather_pane/main.lua
local pane = mud.pane("weather", "Weather")
local weather, state = {}, {}

local function draw()
  pane:meta(weather.region or "")
  pane:set({
    { row = { "Sky", weather.sky } },
    { row = { "Temperature", weather.temp and (weather.temp .. " " .. weather.unit) } },
    { row = { "Position", state.position } },
    { row = { "Language", state.language } },
  })
end

mud.on_gmcp("Room.Weather", function(data) weather = data; draw() end)
mud.on_gmcp("Char.State", function(data) state = data; draw() end)
```

| Call                                | What it does                                                                                                 |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| `mud.pane(id, title)`               | A pane this plugin owns, listed in Add a pane by its title. The id keeps your layout when the title changes. |
| `pane:set(blocks)`                  | Replaces what the pane shows.                                                                                |
| `{ row = { label, value } }`        | A row with a label and a value.                                                                              |
| `{ gauge = { label, value, max } }` | A row with a meter, like the Group pane.                                                                     |
| `{ line = text }`                   | Terminal font text. `{red}` and `{reset}` color it.                                                          |
| `{ rule = true }`                   | A thin line across the pane that sets the blocks apart.                                                      |
| `pane:meta(text)`                   | The words beside the pane's name.                                                                            |

## Shape the window

### 4.1 Arrange the panels

<!-- id: shape.arrange-panels -->

The panel on the right holds your panes. At first it shows the map over your affects. Your vitals are pinned at its foot. You arrange the panel in the window itself, and Vosh keeps the arrangement for each character.

- To show or hide the panel, click the panel button at the right end of the title band. You can also press `Cmd+Shift+L` on macOS or `Ctrl+Shift+L` on Windows and Linux. Or choose `Show panel` in the View menu or the command palette. While the panel is hidden, your vitals move to the status line.
- To add a pane, click `Add a pane`, the plus button in the title band. It lists the panes that the panel doesn't show yet. The pane you choose goes to the bottom. Chat stays on the list while fewer than four Chat panes show. The panes are Map, Affects, Group, Chat, and Staff queues. Staff queues joins the list when the game sends it.
- To open the menu of a pane, click the more button in its header. `Split right` and `Split down` put the first pane that the panel doesn't show beside it or under it. When the panel shows all the panes, they put in another Chat pane. On a Chat pane they put in another Chat pane.
- In the same menu, `Show here instead` puts another pane in its place. `Close pane` removes it. When you close a pane, you lose nothing.
- To share the space between two panes, drag the line between them. You can also press `Tab` to get to a line. The arrow keys then move it 8 points, or 32 points with `Shift`.
- To change the width of the panel, drag its left edge. The width goes from 200 to 800 points. Double click the edge to go back to 300. You can also press `Tab` to get to the edge. The arrow keys then move it 8 points. Settings has the same `Width` under Layout, then Panel.
- To show or hide one pane, use its row in the View menu or the command palette, such as `Show map`.

To start again, choose `Reset panel layout` in the View menu or the command palette. Or choose `Reset to default` in Settings under Characters, then Panel layout. The panes go back to the map over your affects. The panel keeps its width, and it stays shown or hidden.

Settings under Characters draws the panel of each character under Panel layout. So you can see how each one is arranged.

To open Settings, click the gear at the right end of the title band, after the panel button. You can also press `Cmd+,` on macOS or `Ctrl+,` on Windows and Linux. `Open settings` in the command palette and the `Settings` list in the right click menu of the terminal also open it.

### 4.2 Use the map

<!-- id: shape.use-the-map -->

The Map pane draws the map that the game sends. At first it is at the top of the panel. Its header names the area you are in.

- To show or hide the map, choose `Show map` in the View menu or the command palette. Or add it with `Add a pane` in the title band.
- Read the rows under the drawing. The first row names the room you stand in. The name takes the color that the terminal shows it in, from the colors of your theme. Examples are gray for a room inside, yellow for a field, and blue for a lake you can't swim.
- When the panel would make that color hard to see, the pane draws it a little lighter or darker. A few rooms take a color of their own from their area. The game leaves that color out of `Room.Info`, so the pane shows the usual color for their terrain.
- The second row names the terrain and the region, such as `Inside` and `Coastal North`. The exits show at its right. The other rows list the people here. A name that more than one person shares has a count beside it. When more people are here than fit, the last row counts the others.
- A short pane removes rows of people first, then the terrain row. It keeps the room, with its exits beside the name.
- To open the map menu, point at the drawing and click the sliders button in its bottom right corner.
- To change how the map draws, choose `Squares`, `Glyphs`, `Tileset`, or `3D`. In Squares and 3D, a short tick out of a room marks an exit that goes past the room beside it.
- To zoom, scroll or pinch over the map, in any style. Or choose `Zoom in` or `Zoom out`. `Actual size` shows the zoom and goes back to 100%.
- In Tileset, choose `Load tileset…` to use your own tile art. Choose `Clear tileset` to remove it.
- In 3D, drag the map to turn and tilt it. To put north back at the top, double click it or choose `Reset view`. You can also press `Tab` to get to the map, then turn and tilt it with the arrow keys. While the map is turned, a compass in its top right corner points north.
- In 3D, choose the floors it draws with `Your floor`, `One floor up and down`, or `Every floor`. The floors above you draw as outlines, and the floors below fade. `Every floor` numbers each floor by its steps from yours. Turn on `Terrain sprites` to paint the terrain of each room on its roof.

Vosh remembers the style, the zoom, the 3D view, and the tileset. Until the game sends `Map.Tiles`, the pane says that the map shows when your MUD sends it. `Room.Info` names the room, its exits, its terrain, its region, and the area. `Room.Chars` lists the people.

### 4.3 Use the chat pane

<!-- id: shape.chat-pane -->

The chat pane collects channel talk in its own buffer, with one line for each message. To add it, use `Add a pane` in the title band. Or choose `Show here instead` in the menu of any pane.

- Lines come in on their own. `Comm.Channel` GMCP fills the pane automatically.
- A line reads like `[tell] Tolliver: meet at the bank`. The tag names the channel, and the speaker is bold. This is also true for a name of more than one word, such as `a Blackwatch villager`. Wrapped lines start two cells in, so the tags make a column at the left edge.
- Each line takes the color that the game prints that channel in, from the terminal colors of your theme. Say is bright yellow, tell green, gtell bright magenta, and yell cyan. Pray is bright white, cabal bright blue, clan bright cyan, and faction yellow. Newbie is bright green, immortal bright red, and imp bright cyan.
- When you change themes, the chat follows. A color too faint to read on the pane gets lighter or darker, with the same hue, until it is clear. The terminal still shows the color of the theme.
- To change the color of a channel, open the menu of the pane. Choose `Channel colors`, then the channel, then `Default` or one of the 16 terminal colors of your theme. The pane changes at once. Each profile keeps its own choices, and they change when you change the theme. `Reset all` gives every channel its default again.
- Point at a message to see when it came in.
- To filter, use the channel select beside the name of the pane. A single chat pane shows `All`, or the channels you check. You can check as many as you want, and the header names them, such as `Gtell, Tell`. Each chat pane keeps its own filter, so you can split one off for only tells.
- When you add a second chat pane, it starts on `Tell`. At the same moment, your first pane changes to `Everything else`. That is the channels that no other chat pane checks. So each tell goes to one pane, and a note tells you so.
- With two or more panes, each one shows the channels you check in it, or `Everything else`, and `All` goes away. Only one pane at a time shows `Everything else`. The menus of the others say that it is in another pane.
- A later pane starts on `Tell` while no pane checks it. Next, it starts on `Everything else` while no pane shows it. Otherwise it starts with no channels, for you to choose. When you close the others, the last pane shows `All` again. The panel holds up to four chat panes.
- To send trigger output to the pane, open a trigger in Automation, then Triggers. Put a name in `Send to pane` under `Advanced`. Those lines go to the chat pane under that name, in their own words.
- To see the tells you send, use the `Tells you send` preset. The game sends no GMCP for them, so the preset routes the line that the game prints for each one. Vosh turns it on for every profile, one time, unless you had turned every preset off. A new install starts with every preset off.
- Each tell you send reads `[tell] to Tolliver: text`, and so does a tell that a telepath projects. The pane skips the `You tell your group` line, because your gtell already comes in over GMCP. To turn the preset off, go to Settings under Automation, then Presets.

The buffer holds the last 500 lines. It stays when you close and open the pane again. It empties only when you choose `Disconnect` or connect to another world.

Every chat pane reads the same buffer. The pane stays at its newest line. To read back, scroll up. When you scroll to within 24px of the bottom, it stays at the newest line again.

### 4.4 Configure the vitals readout

<!-- id: shape.read-vitals -->

The vitals are at the bottom of the panel, under the panes. Health, Mana, and Moves each show the value with a thin meter under it. The meters stay quiet until a vital gets low. Under 20%, its value and meter turn red. They stay red until the vital goes back up to 25%. In a fight, your opponent gets a row on top with its health.

- Open Settings and choose Vitals.
- Under Style, choose a style from the gallery. Each tile draws your own vitals in its style, so you see the look before you choose it. Use the arrow keys to move through the styles.
- `Rows` gives each vital a row. `One line` puts Health, Mana, and Moves on a single row. `Ledger` puts them in columns. `Gauges` fills a pill for each vital. `Pips` lights ten discs. `Bands` puts a bar over quiet bands that mark low and worn.
- `Ladders` lights a row of segments. `Blocks` draws a bar of block characters in your game font. `Traces` draws each vital over its last minute. `Dials` fills an open dial. `Rings` puts a ring for each vital inside one glyph.
- `Vials` fills a small vial. `Orbs` fills a round orb from the bottom up. `Candles` burns down like a candle, and the flame gets dim when you get low. `Text` writes the vitals with the codes of your prompt.
- To move your vitals under the terminal, set `Show your vitals in` to `Status line`. The panes then take the space at the foot of the panel.
- Leave `Hide vitals while your prompt is pinned` on, and the panel removes its vitals while `Where your prompt shows` is `Pinned`. The panes then take their space. In a fight, your opponent keeps its row at the foot of the panel.
- To keep the vitals, turn it off, or choose another place for your prompt. They then come back at once. They also come back while prompts are off in the game, because the band then has no prompt to show.
- To change the order in which every style draws your vitals, drag a vital by its grip under `Customize vitals`. With the keyboard, press Space on a grip. Move the vital with the Up and Down arrow keys. Press Space again to drop it, or Escape to put it back. To remove a vital from your vitals, turn off its switch.
- To give a vital one of the sixteen colors of your theme, or `Default`, click its swatch. The color tints the label of the vital and its mark, never its number.
- A color that looks like the color of a low vital says `Like low`. While `Warn before you run low` is on, a color that looks like its warning says `Like warn`. Vosh checks both as your `Color vision` under Accessibility sees them. You can still choose either one, because the number still changes color.
- To set where the row of your opponent shows in a fight, choose `On top` or `At the bottom` beside `Your opponent`. This works in every style. To leave the row out, turn off its switch.
- Set `Values` to `Current and max` to read `186 / 1020`. Set it to `Current` to read `186`, or to `Percent` to read `18%`.
- Set `Meter` to `Line` for the thin meter. Set it to `Bar` for a thicker meter that is easy to read in a fight. Set it to `None` to keep only the numbers, on smaller rows. Every style from `Gauges` to `Candles` draws its own mark, so it takes no meter.
- Turn on `Warn before you run low` to make a vital yellow under two thirds and red under one third. The Group pane shows the health of your group in the same way.
- Turn on `Show each hit` to keep the part that a hit takes pale for a moment before it drains away. A heal first shows the part it gains as pale, and then the fill follows. `Ladders` also keeps the segment you were at before the hit lit for a moment.
- `Show each hit` works in every style with a fill, and for your opponent too. `Traces` already draws each hit in its line. The status line and `Text` don't show it. When Reduce motion is on in your system settings, the pale part goes away after it holds.
- `Reset to default` puts every vital back on in the usual order, with your opponent on top. It sets `Current and max` and `Line`, and turns the warning and `Show each hit` off. The button stays dim until you change something. It doesn't change your style or the place your vitals show.
- With `Text`, your text decides all of this. So `Customize vitals` holds your text and a preview of it. `Reset to default` puts back the text it started from. That is your 0.7 text if you had one on, and the text of Vosh if not.
- `Edit…` beside your text opens `Your vitals text` over the terminal, beside the panel. It works like `Customize prompt`. To change a part of your text, click it in the footer. Or use `Insert value…` and `Edit as text`. Command Z takes a change back. Every change saves when you make it.
- `Presets` holds the text of Vosh and the text you opened the card with. It also holds the text before that, and your 0.7 text if you had one.

To change your vitals without Settings, right click them, at the foot of the panel or on the status line. `Style` and `Values` each open a list with a check beside your current choice. A choice takes effect at once. `Style` lists the styles in the order of the gallery, with a line between each family. `Customize vitals…` opens Settings at `Customize vitals`.

With `Text`, `Edit your text…` opens `Your vitals text` over the terminal. `Values` stays dim, because your text writes its own values. The keyboard can't open this menu. But each choice in it is also in Settings, under Vitals, then Style and `Customize vitals`.

Each default draws the panel you already know, so nothing changes until you choose something. One line drops the Health, Mana, and Moves labels only when they don't fit beside the values. This happens under about 360 pt with health of four digits. It keeps the values and meters. `Current` and `Percent` keep the labels, even on a narrow panel. A panel too narrow for even the values puts them in rows.

When you turn off `Show the panel` under Layout, your vitals move to the status line. There they follow `Values` and `Warn before you run low`, but they never draw a meter. In a fight your opponent comes after them, with its health in yellow. When the target you set is the mob you fight, the two share one item. A target on another mob keeps its own item after it.

When the line is too short, things give way in this order. First the name of your opponent, then the labels. Then each value falls back to the current number. Then the moons, a round trip under 300 ms, and the game time. The tick always stays, and so does a slower round trip.

When the game hides your vitals, as it does under lamented tears, every value reads `?` in dim text over an empty meter. This is the same in the panel and on the status line. Nothing turns yellow or red while they are hidden. Your numbers come back with the next update that the game sends. In a fight, the opponent row reads `?` in the same way when the game hides its health or sends none. Its health on the status line does the same.

### 4.5 Watch your group and affects

<!-- id: shape.group-affects -->

The Group pane shows the health of each member of your group. The Affects pane shows what affects you and the hours each affect has left. To add either one, use `Add a pane` in the title band.

- Read the Group rows. Each member gets a row with the name, a `lead` tag on the leader, a thin health meter, and the percent. The meter and the percent stay quiet at 67% and up. They turn yellow down to 34%, and red below that. The header counts the members.
- To choose the affects you track, go to Settings under Characters, then Tracked affects. Choose `Add affect…`, and choose one of the affects on you now or type its name. Matching ignores case and extra spaces.
- Under Advanced, you can give a tracked affect a short label to show in its place, such as `sanc`. You can also set the order of your slots.
- To choose how the affects pane draws, go to Settings under Layout, then Affects. Or use `Style` and `Marker` in the menu of the pane. Each character keeps its own choice.
- `Timers first` is the default. `Countdown` lists every affect by the hours it has left. `Grouped chips` puts what to recast first. `Draining chips` does the same and colors only the hours a chip has left.
- In `Timers first`, the affects pane has two columns. Each entry shows the hours left, then the name exactly as the game sends it. `+` means permanent, and `-` means you don't have it. The game uses the same marks in its own affects bar. A pane narrower than about 360 pt shows one column.
- Your tracked affects fill the top rows in your order. They keep their places as the hours change. The dot beside each one agrees with its hours. Green is up, yellow is running out, and red is almost gone.
- Unless you change them, yellow is two hours or fewer, and red is one hour or none. A hollow red ring and a red name mean the affect is missing.
- All the other affects are under a thin line. Harmful affects such as `faerie fire` come first, with a red diamond and a red name. The rest follow by hours left, down the left column and then down the right column.
- In `Countdown`, the tracked affects you are missing come first. Then every affect follows by the hours left, down the left column and then down the right column. A thin meter under each affect drains as its hours go down.
- In `Grouped chips`, `Recast` holds the tracked affects that are missing or running out. `Tracked` holds the other affects you track. `Other` holds all the other affects.
- A missing affect is a dotted red chip. A tracked chip is filled, and the fill drains from the left as its hours go down. A chip that is running out turns yellow, then red when it is almost gone. Other affects are in outlined chips, and harmful ones are red.
- `Draining chips` groups and orders the chips in the same way. A chip that is running out has no tint of its own. A thin outline shows its full width. Yellow or red fills only the part that matches the hours it has left, and the whole chip when it is full.
- The meters and the fills measure each affect against the most hours Vosh saw for it since you last cast it. Vosh remembers this for each character between logins. An affect that Vosh first sees part of the way through starts full. A permanent affect stays full.
- `Marker` sets the mark beside each affect you track in `Timers first` and `Countdown`. Choose a dot, a square, plus and minus, or none. Plus and minus shows a plus while you have the affect and a minus while it is missing. In every shape, the color shows the state. With none, the hours and the red names still show it.
- Turn on `Tint what to recast` to tint the affects to recast. A missing affect gets a red wash. An affect that is about to drop gets a yellow or red wash. This works in `Timers first` and `Countdown`. Both chip styles always mark what to recast.
- The hours follow the game unless you change them. One hour or none shows in bold red, and two hours shows in yellow. To change this, set `Running out at` and `Almost gone at` in Settings under Layout, then Affects. Or choose `Change when affects warn…` in the menu of the pane.
- Each setting takes whole hours. Almost gone is never more than running out. So the same number for both skips the yellow. Each character keeps its own settings. The header counts the tracked affects that are missing and the ones running out.
- The pane shows only whole rows. When some rows don't fit, the last entry tells how many more there are, such as `5 more`. Click it to scroll to them. Point away to scroll back.

Group data comes from `Group.Info`. Affects come from `Char.Affects`. The game sends it when you log in, when an affect changes, and every tick. With no group, the pane says that your group shows when you join one.

When the game hides your affects or your group, as it does under lamented tears, the pane tells you so in place of its rows. The affects pane marks no tracked affect as missing. The group pane shows no member health from before. Each pane fills in again with the next update that the game sends.

### 4.6 Watch the staff queues

<!-- id: shape.imm-board -->

The Staff queues pane lists the staff queues that need you, with the worst first. The game sends them only to immortals.

- Log in on an immortal. The game sends `Imm.Queues` at login. `Show staff queues` then shows in the View menu, the command palette, and `Add a pane`.
- To add the pane, use `Add a pane` in the title band, or choose `Show staff queues`. Until the queues come in, the pane says that they show when you log in as an immortal.
- Read from the top down. Only queues with work show. Items past their deadline come first. Then come items in the last quarter before the deadline. Then come the rest, with the bigger backlog first.
- The queues are Description checks, Applications, Journals, Votes, Notes, Bugs, Penalties, Ideas, and Typos.
- Read the asides. A row tells how many items are overdue or near their deadline. Applications adds how many are unread. Journals adds how many have no award. Point at a row to read what it counts and its deadline.

The header adds up the overdue items. If there are none, it adds up the items near their deadline. When nothing waits, the pane says that no staff queue needs you now.

### 4.7 Choose where your prompt shows

<!-- id: shape.prompt-show -->

When Vosh reads your prompt, you choose where it shows. Open Settings, choose Prompt, and choose a place under `Where your prompt shows`.

- `In the text` shows each prompt where the game sends it. The terminal reads as it always did.
- `Lifted` keeps every prompt in the text, on a raised band in the selected row color of your theme. This includes the scrollback. A prompt that ends on a character gets one space after its band, so your echo never touches it.
- `Pinned` takes your prompts out of the text. It shows your newest prompt on a band above the command line. The band is only as tall as your prompt.
- When a fight adds a row to a pinned prompt, the text above gives up its top line to make room. It gets the line back when the fight ends. So one blank line always stays between your newest line and the band, as the game leaves one before each prompt.
- Every prompt still goes to the session log and your Prompts triggers. While your prompt is pinned, the panel hides its vitals and gives their space to the panes. Only the row of your opponent in a fight stays. To keep the vitals, turn off `Hide vitals while your prompt is pinned` under Vitals, then Style.

At the foot of Customize prompt, the button beside `Draw your prompt` names where your prompt shows now. Click it and choose another place. Customize prompt moves with your prompt.

In the command line, `#prompt show lifted` chooses the same place. Use `text` or `pinned` in its place to choose the others. `#prompt` alone also tells you where your prompt shows.

To choose a place, Vosh must read your prompt. Until it does, the row stays off and tells you what to do first. `#prompt show` tells you to type `#prompt game` with your prompt setting in braces. While prompts are off in the game, the pinned band tells you so and shows nothing else.

With the xterm renderer, the newest 1000 prompts keep their bands, and older ones show plain. The split history pane also shows them plain. The native renderer keeps a band on every prompt in the scrollback.

The choice saves in the `[prompt]` table of your profile as `show`. An older version of Vosh ignores it and shows your prompt in the text. When that version saves your profile, the choice is lost. So choose it again here.

### 4.8 Watch a player with snoop

<!-- id: shape.snoop -->

When you snoop a player in the game, a split opens at the top of the terminal column. It shows what the screen of that player shows, in the colors of the game. Your own terminal stays under it, next to your command line, and your caret stays where it was. Vosh asks the game for snoop on every connection, so you have nothing to turn on. A character who never snoops sees no change.

- There is one tab for each player you snoop. A green dot marks a snoop that runs, and a ring marks a snoop that ended. A tab behind the front tab gets brighter and gets a dot when new lines come in. Point at a tab to see how long that player has been quiet.
- To stop the snoop of the player in front, click `Stop`. It sends `snoop stop Tolliver`. The tab goes away when the game says that the snoop ended. `Stop every snoop` in the more menu sends `snoop stop`, which ends all of them.
- A snoop can end in other ways, such as when you type the command, when Tolliver quits, or when your connection drops. Then the tab stays with the last thing it showed and tells when it ended. To remove it, click `Close`. When you snoop Tolliver again, the same tab continues.
- Each tab keeps 5,000 lines in the font and size of your terminal, wrapped at words like your own terminal. `Find` in the more menu searches the tab in front. So does `Cmd+F` while you're in the snoop. `Cmd+C` copies what you select there.
- Your triggers, highlights, gags, and sounds never act on snoop text, because you wrote them for your own screen. Your Lua still gets `Snoop.Start`, `Snoop.Stop`, and `Snoop.Output` like any other GMCP.
- To change the size of the split, drag the line under it. It starts at 40 percent of the column, and your profile keeps the size you choose. It keeps four rows and always leaves you six.
- To fold the split to its strip, drag the line to the top. Or choose `Fold` in the more menu, or double click the line. Double click again to open it.
- To move into the snoop, press `Cmd+J` on macOS or `Ctrl+J` on Windows and Linux. Press it again to go to the next tab. To go back to the command line, press `Escape` or start to type. So what you type always goes to your own character.
- To move the tabs to a window of their own, choose `Open in a window` in the more menu. This is useful on a second screen. `Cmd+J` brings that window to the front. Your profile remembers the place and size of the window. When you close it, the tabs go back to the split.

Each session keeps its own snoops. Its row in the sessions sidebar shows an eye and how many snoops run. A disconnect ends every snoop on that session.

The session log keeps each line of a snoop. The line starts with the name of the player, such as `Tolliver|`. To read it again after the tab is gone, search your logs for `^Tolliver\|`.

While a snoop is open, type `snoop` in the command palette. You then get `Go to snoop`, `Next snoop`, `Stop snooping Tolliver`, `Stop every snoop`, `Open snoop in a window`, and `Close ended snoops`. Snoop has no row in Settings.

## Tick and target

### 5.1 Configure the tick timer

<!-- id: tick.tick-timer -->

The tick timer shows the tick of the game in the status line under the command line. The game time and the moons are beside it. The tick of the game decides when the timer fires. Vosh knows the game ticked when the game hour moves, because Aabahran moves the hour once each tick. A line that matches your `Reset on` pattern also tells Vosh that the game ticked.

When the tick comes, the count starts again, the sound plays, and your `Send each tick` command goes out, one time for each tick. To set up the tick, go to Settings under Automation, then Timers. `Tick` is at the top of the list. Click `Save` to apply your changes.

- Turn on `Enabled`. Every connection starts the tick. When you change characters, it keeps running until you turn it off. While another session on the profile is connected, a new connection uses the switch setting of that session. `Play a sound` under `Advanced` plays a sound when the tick comes.
- Set `Every` in seconds, from 1 to 3600. It is how long you expect a tick to take. Aabahran chooses each tick between 25 and 35 seconds. So after the game ticks, the timer waits for the game and doesn't fire at `Every`.
- To send a command on every tick, put it in `Send each tick`. Leave it blank for no command.
- Give `Reset on` a regex. A line that matches is the tick. A signal within 2 seconds of a tick counts as that tick. So a matching line and a move of the game hour at the same time fire one time.
- Before the first tick of the game in a session, the timer fires on its own every `Every` seconds. It does the same on a game that never tells Vosh when it ticks. When the game is quiet for two times `Every`, the timer fires one time on its own. It then keeps its own time until the next tick comes in.
- In its last 5 seconds, the tick in the status line turns the warn color on a soft background. It stays that way while the game is late.
- To also print a warning line in the terminal, turn on `Warn before it fires`. Set `Warn at` to the number of seconds of warning you want. The status line then uses the same number. The terminal prints the warning one time for each tick.
- To change the style of the warning line, fill in `Warning text` and `Warning color`. The color takes an ANSI name, `#rrggbb` hex, or a 256 palette index. Blank keeps the defaults.
- While the game is late, the tick pulses gently in the warn color until the tick comes. When Reduce motion is on in your system settings, it stays still in the warn color.
- To choose which way the tick counts, use the Tick counts row in Settings under Layout, then Status line. `Up` shows the seconds since the last tick. It keeps counting past `Every` while the game is late, such as `31s`.
- `Down` shows the seconds left until the tick. It goes from `Every` right after a tick down to `1s` in its last second. It waits at `0s` when the game is late. `Down past 0` counts down in the same way, but continues below zero until the tick comes. An early tick starts either count again at once.
- The Tick and time row at the top of the card sets how the status line shows the tick, the time, and the moons. `Value` shows each value alone, such as `14s` and `8:42`. `Caption` puts Tick, Time, and Moons before them. `Icon` puts a ring before the tick.
- When the ring counts up, it fills clockwise as the seconds go by. It closes when the tick is due, and stays closed while the game is late. When it counts down, it shows the time left and empties clockwise toward the top. While the game is late, only the faint ring shows.
- With `Icon`, the time has the sun on its path over the horizon before it. The sun rises on the left, is highest at midday, and sets on the right. After dark, it goes under the horizon as an open dot.
- To choose the clock for the game time, use the Game time row under Tick and time. `24 hour` reads like `18:00`. `12 hour` reads like `6:00 PM`, with `12:00 AM` at midnight and `12:00 PM` at noon. Each character keeps its own choice.

The tick settings belong to the profile. So what you set here reaches every session on the profile, and so does each `#tick` command that changes a setting. Each session keeps its own count. `#tick reset` starts the count again only in the session where you type it. `Send each tick` goes out in every session on its own count. The sound plays only for the session in front.

The game time takes a tint from your theme for the part of the day. Each moon in the sky shows as a small icon of its phase in its own color. Each moon takes the color that the game gives its name, from your theme. So Lysenties is in the bright white of the theme, Nercuros in its bright cyan, and Dyphrities in its red.

On a light theme, the icons are ink on paper, like a printed calendar, with the dark part filled in. So a new moon is a solid disc and a full moon is an open ring. Point at a moon to read its name and phase, such as `Nercuros, nearly full and still growing`. During an eclipse, the triad, or a near alignment, one word in the warn color comes after the moons. A dormant moon stays hidden. The moons leave the line while you aren't connected.

### 5.2 Track a target with quick keys

<!-- id: tick.track-target -->

Set a target with `tar`, and Vosh keeps it in the status line. A quick key pairs a short name with a verb. When you type the name, the verb acts on your target.

- Type `tar` to list the people in the room. Type `tar 2` or `tar drag` to choose one by number or by part of the name. `tarn` and `tarp` go to the next or previous person. `tarclear` clears the target.
- Read the status line under the command line. When a target is set, it shows `Target` and the name. While you fight that target with the panel hidden, the line names it one time, with its health in yellow.
- Look at the room. When the `Room, time and weather colors` preset is on, the line of your target turns bright red while the room lists them.
- Read the vitals at the foot of the panel. In a fight, your opponent gets a row on top with its health.
- To set a quick key, type `#qkey <name> <verb>`, such as `#qkey gg backstab`. Then type `gg` as the first word of a command. Vosh sends `backstab` and your target. Vosh doesn't echo it, because the backend echoes the expansion instead.
- Type `#qkeys` to list the quick keys. Type `#qkey clear <name>` to clear one.

When you set a target with `tar`, Vosh also fills `$target`. So `cast dispel $target` aims at your current target. Each session keeps its own target and its own quick keys. A new session starts from the stock `gg`, `xx`, `zz`, and `tt` slots. Every session goes back to them when you restart. So set your verbs again with `#qkey` after each launch.

## Make it yours

### 6.1 Switch themes

<!-- id: make-it-yours.switch-themes -->

A theme sets the colors of the whole window, the terminal included. Themes are in Settings under Appearance, in the Theme section.

- Open Settings and choose Appearance.
- Click a theme in the gallery. Each theme draws in its own colors, with its name under it. Your own themes come after the built in themes. The theme applies at once and saves. You can also choose a theme with the arrow keys.
- Read the line under the gallery. It describes the theme on screen. For a built in theme, it also names the source of its colors, who made them, and their license.
- The `Vision` switch above the gallery shows every theme as a player with deuteranopia, protanopia, or tritanopia sees it. It starts on your `Color vision` from Accessibility. It is only a preview, so it changes no theme.
- To choose how the window changes theme, use `Switch themes`. `Off` keeps the theme you click. `With the system` changes between the `Light theme` and the `Dark theme` you choose under it, when your system changes.
- `With the game` shows your `Day theme` from dawn in the game and your `Night theme` from dusk. So the window changes about every 6 minutes.
- High Contrast and High Contrast Light keep every text color at 7:1 or better. When `Switch themes` is on `With the system`, Increase contrast in macOS settings shows them. The light one shows while your system is light, and the dark one while it is dark.
- While Increase contrast is on, a theme you click waits in its slot. It shows after you turn Increase contrast off.
- Choose the `Day theme` and the `Night theme` from any theme, light or dark. Both start on the theme that shows, so nothing changes until you choose. A click in the gallery fills the one that shows now.
- Choose a pair with a similar tone, such as Obsidian Ember by night and Gruvbox by day. Two dark themes feel like evening coming on. A dark theme and a light theme flash at every change.
- When you go offline, the window keeps the theme it showed last, also through a relaunch. It keeps it until the game gives the time again.
- To read a Ghostty, iTerm2, Kitty, or Alacritty theme file, click `Import…`. Vosh adds it to your own themes and changes to it.
- Or choose `Choose theme` in the View menu or the command palette. It lists every theme.

A theme sets both layers of the window. The window layer covers the backgrounds, the text, the separators, and the accent. It also covers the warn, danger, and success colors. The terminal layer covers the background, the text, the cursor, the selection, and all sixteen ANSI colors. MUD text takes the sixteen colors of the theme while `Use the theme's colors for MUD text` is on under Terminal text. It is on for every theme until you turn it off.

A new install starts on Triad, with Rubric as its light theme. The built in themes don't change. So to change one, start a custom theme from it. If the theme you use is ever gone, Vosh uses Obsidian Ember.

One Dark, Vellum, and Everforest Light are no longer in Vosh. If you chose one of them, Vosh shows the theme that replaced it until you choose another. One Half Dark replaces One Dark, Rubric replaces Vellum, and Melange Light replaces Everforest Light.

### 6.2 Create a custom theme

<!-- id: make-it-yours.build-your-own-theme -->

A custom theme starts as a copy of the theme you see. You can change any of its colors. The editor is in Settings under Appearance, then Advanced.

- Open Settings, choose Appearance, and change to the theme you want to start from.
- Open `Advanced` at the bottom of the page and click `New custom theme`. Vosh copies the theme you see. It names the copy after that theme, with `copy` at the end, and changes to it.
- Choose the theme you are changing in `Theme to edit`. Set its `Name` and `Description`.
- Change its colors in four groups. Accent and status holds Accent, Danger, Warning, and Success. Terminal holds the background, the text, the cursor, the text under the cursor, the selection, and the selected text. Normal colors and Bright colors hold the sixteen ANSI colors.
- To choose a color, click a swatch, or type a color in the field beside it.

Every change applies at once and saves. Your custom themes come after the built in themes in the gallery. To remove one, choose it in `Theme to edit`, click `Delete…`, and confirm.

### 6.3 Control terminal colors

<!-- id: make-it-yours.control-terminal-colors -->

The colors of MUD text are in Settings under Appearance. The rows that help you see them are under Accessibility.

- Open Settings and choose Appearance.
- Under Terminal text, turn on `Use the theme's colors for MUD text` to draw what the game sends in the sixteen colors of the theme. Turn it off to keep the exact colors that your MUD sends. It is on for every theme until you turn it off.
- Open `Advanced` and change `Base palette`. These are the sixteen colors that MUD text uses while the colors of the theme are off. To change a color, use its swatch or type a hex color. The first change keeps all sixteen as your own list.
- To go back to the stock chart, click `Reset` beside Base palette. It stays off until you change a color.
- If you confuse red and green, or blue and green, choose Accessibility. Then choose your `Color vision` under Color and contrast. Vosh changes those colors to colors you can tell apart, in the game text and the status colors of the window. If not, keep `Typical`.
- Leave `Keep highlight colors readable` on under Color and contrast. Vosh then makes a color that your triggers set darker or lighter when the theme would make it faint. It is on until you turn it off.

`Fit game colors` is under Accessibility, then Color and contrast. It is on until you turn it off. While you play, it makes the game colors that fade on the theme easier to see. Examples are a faint room name, or a bold white that is the same as the text around it. Vosh moves each color lighter or darker with the same hue, so the red of the theme stays red.

These all draw the fitted colors. They are the terminal, your pinned prompt, the chat pane and its channel colors, and the room name under the map. They are also the game time and the moons in the status line. On a light theme the terminal also makes darker the fixed colors that the game sends past the sixteen colors of the theme. It does this until they show, such as the yellow desert and the white snow of the minimap of the game.

The window, Settings, and log exports keep the theme as it was published. So it still matches the same theme in your other terminals. Only a Color vision other than Typical changes the window. It changes the status colors and, where necessary, the accent. Solarized Dark also stays as it was published during play, because its soft text is part of the scheme. This is true until you choose a Color vision other than Typical.

The base palette under Advanced stays as you set it.

`Color vision` changes the colors that your eyes confuse to colors that they can tell apart. Color blind modes in games do the same. Typical keeps every theme as it ships. Deuteranopia and Protanopia change tells in green to blue. So tells are easy to tell apart from hits on you in red and from says in bold yellow. Reds move toward orange and blues toward violet, where the theme allows it.

Tritanopia changes blues to purple and magentas to pink. So blue is easy to tell apart from yells in cyan and from tells in green. Each color keeps the lightness of its theme where it can. A soft theme such as Kanso Zen gets enough color for the change to show.

Two channels that you could tell apart never become the same for your vision, newbie chat and immortal talk included. No color goes below the contrast that the theme keeps, or becomes too near to body text and white. Hits on you stay as far from HP at 40 percent as they were. When a color has no room left to change, it keeps its own hue.

On a theme where its own text, yells, and cabal already use all the blues, such as Tokyo Night, tells stay green. Newbie chat in bold green changes to blue only where the blues leave room. The game text follows Color vision whenever the colors of the theme draw it. This is true with Fit game colors on or off, Solarized Dark included. Every window also follows it, Settings included.

Under Deuteranopia and Protanopia, the success color of the window changes to blue and danger moves toward orange. This shows in health bars, the affect chips, group health, and alert marks. Under Tritanopia, the window keeps its status colors where you can tell them apart. It makes them lighter or darker where danger is near warn or success, such as on Harbor Dark.

An accent that the theme sets stays as the theme drew it. An accent that Vosh chooses is as far from all three status colors as the hues of the theme allow. The theme editor still shows the published colors of each theme. The line under the Color vision row tells what your vision changes. A built in theme changes at once. A theme of your own plays its usual colors for a few seconds while Vosh calculates the change.

Your color vision follows your theme. So it stays the same on every character while `Theme` is on under General, in Keep the same for every character.

Keep highlight colors readable covers the exact colors that a trigger or a preset uses for text, such as `{#8fa7d9}` or `{fg:244}` in `Replace with`. Vosh measures each one against the terminal background. When one is too faint, Vosh keeps its hue. It makes the color darker on a light theme or lighter on a dark theme until it is readable. A color that is already readable stays as you chose it. Your trigger keeps the color you saved.

This switch never changes the colors of the game or the sixteen colors of the theme. Fit game colors covers those. A change of theme reaches the lines that come in after it. Earlier lines keep the color they were drawn in.

Other colors are with the rows they belong to. `Command color` under Input, then Sent commands, changes the color of the local echo of every command you send. `Mark color` above it changes the color of the mark before each command. The mark stays the grey of the bright black of your theme until you choose a color.

`Text color` and `Caret color` under Input, then Command line, change the color of what you type and of the caret. They keep the text and accent colors of your theme until you choose a color. While `Color commands as you type` is on, four more rows are under it. `Aliases` start in the cyan of your theme, `Vosh commands` in its magenta, `Chat` in its yellow, and `A # command Vosh doesn't know` in its danger color.

`Divider color` under Layout, then Split terminal, changes the color of the line between the history and the live tail. To choose a color, click the swatch or type a hex color such as `#fffc41`. Each one applies live. When you empty the field, it goes back to the theme default.

### 6.4 Set the fonts

<!-- id: make-it-yours.pick-your-fonts -->

The terminal font is in Settings under Appearance, then Terminal text. The panel font and its size are right after it, under Panel text.

- Open Settings and choose Appearance.
- Under Terminal text, choose a font in `Font`. JetBrains Mono ships inside Vosh, so it works on every computer. The rest of the list holds the monospace fonts installed on your computer.
- Choose a size in `Size`, from 11 to 18 pt. The default is 14.
- Choose `Compact`, `Default`, or `Loose` in `Line height`.
- Under Panel text, choose a font in `Font`. It sets the font of every pane in the panel and of the status line.
- `As designed` is the default. It keeps the fonts the panes were designed in. The headers, labels, counts, and rows use the font of the menus and Settings. The game text in your affects, the chips, and chat uses your terminal font.
- `Same as terminal` draws all of it in your terminal font. `System font` draws all of it in the font of the menus and Settings. The rest of the list holds the fonts that the terminal `Font` offers.
- Choose a size in `Size` under Panel text. The headers, the labels, the rows, chat, the map labels, and the status line all get larger or smaller with it. So the panel reads as one size.
- The size starts at 12 pt, the size the panes were designed at. `Same as terminal` follows your terminal size. The menus, the title band, Settings, and Help keep their sizes.
- To set a whole list of fonts, open `Advanced` and type it in `Font stack`, such as `"Fira Code", "JetBrainsMono Bundled", monospace`. Vosh uses the first font in the list that you have.
- To draw bright colors in the bold weight of your font, turn on `Bright text in bold` under Advanced. It works on macOS.
- To stop text that your MUD or your prompt sets to blink, turn off `Blinking text` under Accessibility, then Motion. It starts off when your system reduces motion.

Each change applies at once and saves. Under General, `Font and size` in Keep the same for every character decides one thing. It decides whether every character shares one terminal font and size, and one panel font and size.

### 6.5 Hear the game with a screen reader

<!-- id: make-it-yours.screen-reader -->

Vosh can give the game to your screen reader, such as VoiceOver on macOS or NVDA on Windows. The switches are in Settings under Accessibility, in the Screen reader section. Each switch is off until you turn it on.

- Open Settings and choose Accessibility.
- Under Screen reader, turn on `Read new game lines`. Your screen reader then reads each line that the game shows, after your gags and routes. So a line that your gags hide, or that your routes take out of the terminal, stays quiet.
- The lines that come within one pulse of the game join into one announcement. So a room look reads as one piece.
- Choose a number in `Long bursts`, 4, 8, 16, or 32. When more lines than that come at once, you hear how many came, and then the last line. An example is `12 lines.` and the line. It starts at 8.
- Turn on `Read your prompt` to hear your prompt after the lines of each pulse. Your prompt comes every pulse, so this starts off.
- With it off, press `Cmd+Shift+P` on macOS or `Ctrl+Shift+P` on Windows and Linux to hear your newest prompt. `Read your prompt` in the command palette does the same.
- To hear the game while you work in another app, turn on `Read in the background`. With it off, Vosh stays quiet while another app is in front.

Vosh also keeps the last 500 lines of the session in front in a list named Game lines, right after the terminal. To read back what you missed, go through it line by line with your screen reader. The list doesn't read anything aloud by itself. A session behind keeps its lines quietly. Its list shows them when you bring the session to the front. All of this works with both terminal renderers.

The prompt key works only while `Read new game lines` is on. With it off, a macro that you bound to `Ctrl+Shift+P` keeps working.

Each change applies at once and saves with your profile.

When a screen reader runs as Vosh starts and `Read new game lines` is off, the terminal tells you one time where to find it. On macOS, Vosh asks the system if VoiceOver is on. On Windows, it looks for the sign that Narrator, NVDA, JAWS, and other screen readers give the system. Linux gets no line, because no sign of a running screen reader works on all its desktops.

## Characters and data

### 7.1 Manage profiles

<!-- id: characters-and-data.profiles -->

A profile has its own aliases, triggers, macros, and variables, its tracked affects, and its panes. Vosh chooses the correct profile when you connect, and again when you log in. Manage profiles in Settings under Characters.

- Open Settings and choose Characters. Your profiles are listed on the left. Each profile that a session plays has a dot, and each profile shows its world beside it. A profile on a port that isn't the own port of the world also shows the port, as The Forsaken Lands 1825.
- Click `New profile` under the list, type a name, and press `Enter`. Names take letters, numbers, spaces, hyphens, and underscores.
- To edit a profile, select it. When you select a profile, the session you play doesn't change.
- Choose its `World`, then turn on `Use this profile when you log in`. The row names your character after Vosh has seen you log in. When you turn it on, it takes that character from any other profile on the same world. Vosh tells you so under the list.
- On a port that isn't the own port of the world, such as 1825, something different happens. A profile that claims the character on the whole world keeps it on the own port of the world. The line under the list names each claim that moved.
- Open the more menu of a profile to choose `Switch to this profile`, `Rename…`, `Duplicate…`, `Export to Downloads`, or `Delete…`. `Switch to this profile` moves the session in front to that profile.
- With two or more sessions open, the line under the list names the sessions on each profile, such as `Default plays in Tolliver's session, Build in Orla's.`

`Duplicate…` copies all of the setup of a profile, but not its world and login. You can't delete a profile that a session plays. So first change that session to another profile, or close it.

Settings edits the profile of the session in front, and follows you to the profile of another session. Unsaved changes to a list under Automation keep Settings on their profile until you save or discard them.

`Export to Downloads` saves the profile as a file in your Downloads folder. When a profile has characters, Vosh first asks which ones the file names. Each character starts off. So a profile you share names your characters only when you turn them on.

The file has the presets you have on and your changes to them. So a friend who imports it sees your colors. In loadout mode it has the presets of the catalog, which every character shares. `New profile` copies both from the profile you play. `#profile reset` turns every preset off and clears your changes. `#profile load` reads both back from the file.

An older version, such as 0.8.1, runs the presets as they ship. It keeps the groups you gave their triggers. It drops your other changes the first time it opens the profile. So back in this version, the presets run as they ship, with your groups.

To bring in a profile, click `Import…` beside `New profile` and choose a Vosh profile export. Vosh shows what the file holds before anything changes. Under `Add as`, `New profile` adds it with the name you type. `Replace a profile` puts it over the profile you choose, which keeps its own world and characters.

Click `Import` or `Replace`. Vosh selects the profile and tells you under the list what happened. The profile takes all of the presets that the file has on, and all of the changes to them.

In loadout mode, the triggers, aliases, and macros in the file go into the shared catalog, never into the profile file. They go into a group with the name of the file, such as `Healer profile`. When the catalog already has an item with the same name, or a macro of yours on the same key, yours stays. The line under the list tells you so.

The presets of the file stay out. That is the triggers and macros they added, the list of presets that are on, and the changes to them. This is because the presets of the catalog serve every character. Under `In this file`, `Presets` says `Stay as the catalog has them`.

Plugins that the file turns on come in off, so you turn each one on under Scripts. When a trigger or an alias in the file runs Lua, Vosh names each one under a warning. This is because Lua can send commands to the game and read everything the game sends. Import profiles only from people you trust.

A new profile takes the world that the file names, with a switch for each character that the file names. A character that no other profile has starts on and goes into the new profile. A character that another profile has starts off and stays there. Turn it on to move it. When that leaves the other profile with no character, its login turns off. A new profile with no character starts with its login off.

Some settings can stay the same for every character. Under General, Keep the same for every character holds `Theme`, `Font and size`, `Keep last command`, and `Check for updates`. When a switch is on, every character shares one value. When you turn it off, each character keeps its own value.

In the command line, `#profile save` and `#profile load` write and load again the file of the profile your session plays, when you ask.

Each profile reads your prompt on its own. Vosh moves the capture trigger that `#prompt` made into each profile that draws your own prompt. It turns the trigger off, and tells you one time at launch. Profiles that draw nothing then show the prompt of the game.

On The Forsaken Lands, the moved pattern changes to your prompt codes the first time the game shows them. That is when you log in, or when you type `prompt`. From then on, Vosh follows each prompt you set in the game. It keeps your design and the draw switch as they are.

Sometimes a color code conflicts with a code in that prompt. Or the pattern fills a value under a name that no prompt code fills, such as `health`. Then the pattern stays, and `#prompt` tells you why. A pattern that you set with `#prompt {regex}` never changes.

An older version of Vosh shows the prompt of the game in every profile until you turn `prompt-capture` on again under Automation. Back in this version, Vosh moves the capture into your profiles again and turns the trigger off.

### 7.2 Set up loadouts

<!-- id: characters-and-data.loadouts -->

Loadouts turn whole groups of aliases, triggers, and macros on and off from one shared catalog. Loadout mode starts with a migration from the files of each profile. The migration runs one time.

- Open Settings, choose Automation, and click `Import…`. Find the `Shared catalog` section and click `Preview…`.
- Look at the plan. The preview opens as a dialog in three sections. First it asks you to choose the version to keep of each item that your profiles hold in different versions. When only one version was on, that version is already chosen.
- `Merged as they are` counts the aliases, triggers, and macros that merge with no question. `A loadout for each character` names the groups that each loadout turns on. It also names the presets that each character gets or loses. The preview writes nothing.
- Click `Apply`. Vosh copies each profile file to `profiles/legacy/` and writes the catalog and the loadouts. It removes the aliases, triggers, and macros from each profile file. Every other setting stays with its profile, but not the presets.
- Loadout mode keeps one list of presets that are on, and every character shares it. The list starts with every preset that any profile file had on. The preview names each character that gets or loses a preset.
- Every character also shares one set of changes to the presets. Changes that your profiles agree on stay. Where two profiles changed a preset in different ways, the wizard asks which version to keep, as it does for an alias.
- Loadout mode waits for the next launch. So click `Quit Vosh` in the dialog and open the app again. Every loadout starts off. So each profile keeps on the items it had on, at launch and when you change profiles.
- Open Settings again and choose Automation, then Loadouts. Loadouts now shows after Presets. Turn on the loadouts you want to use and click `Save`. Vosh turns on all the groups of every active loadout.

The catalog keeps your folder names where it can. Each alias, trigger, and macro goes into a group that is on for exactly the profiles that had it on. So a folder that two characters filled in different ways can become more than one group.

`combat` holds what most characters kept in their combat folder. `combat (Healer)` holds the combat items that only the Healer had. `(Healer)` holds the items the Healer had outside all folders. Each profile file remembers which groups its folders became. So `#group combat on` and `#group combat off` still turn on and off exactly what that profile had in its combat folder.

When two characters had a trigger in different versions, the catalog keeps each version. The second one gets a name that adds its characters, such as `greet (Healer)`. An alias or a macro keeps the one version you choose in the wizard, because you type or press its name. Triggers keep the order each character had them in. This is because every trigger that matches a line fires in that order. When two characters had the same triggers in different orders, one of them gets its own copy of a trigger, named in the same way.

To make the catalog dormant, click `Turn all off`, then `Save`. Dormant turns off every alias, trigger, and macro that is in a group. It stays through restarts and profile changes. Items with no group always stay on.

The set of loadouts that are on belongs to the profile. While your sessions play one profile, a change in Loadouts reaches every profile that hasn't made its own choice. When sessions play two or more profiles, a change there applies only to the profile of the session in front. The other profiles keep the loadouts they have on.

When no active loadout lists any enabled groups, the loadouts set nothing. Each group then stays on or off as you left it, unless you keep the catalog dormant. While the loadouts set groups, and while the catalog is dormant, the switch on each catalog group in Automation waits. A note names the loadouts that decide the group, or says every loadout is off. Timers stay with each profile, so no loadout turns a timer group on or off.

The only edit that Loadouts makes is to turn loadouts on and off. To make or change loadouts, edit `loadouts.toml` in the app data folder while Vosh is closed. The migration wizard runs one time. It doesn't make a new catalog in these cases.

- While `catalog.toml` or `loadouts.toml` is in the app data folder.
- While `profiles/legacy/` holds the copies from an earlier run.
- While an earlier run waits to finish at the next launch.

After the move, `catalog.toml` holds your aliases, triggers, and macros. It also holds every one you add or change later. Each file in `profiles/legacy/` is a backup of its profile as it was before the move. Never copy a backup back while `catalog.toml` is in the app data folder.

If you do, Vosh puts the old aliases, triggers, and macros of the backup over the catalog. It turns on, for that character, items that only other characters had. At the next save, it puts the old versions in the catalog for every character. To keep your items, leave `catalog.toml` where it is, and change them in Automation settings.

To make a new catalog from the backups, do these steps.

- Quit Vosh first, because Vosh saves `catalog.toml` again when it quits.
- Move `catalog.toml` and `loadouts.toml` out of the app data folder.
- Copy the files in `profiles/legacy/` back over the files in `profiles/`.
- Move `profiles/legacy/` out of the app data folder too.
- Open Vosh again and run the wizard.

Each profile then comes back as it was before the move, with every setting. It loses every change that you made to its settings since then. Your items as they are now stay in the `catalog.toml` that you moved out.

### 7.3 Import a TinTin++ file

<!-- id: characters-and-data.tintin-import -->

The `#import-tintin` command reads aliases and variables from a TinTin++ `.tin` file. It loads them into the live profile. You run it from the command line.

- Type `#import-tintin <path>` with the path of the `.tin` file. `~` expands in the path.
- Read the echo. It prints `imported <path>` and a count line such as `12 aliases, 4 vars`.
- Check the `skipped (unsupported)` line. It counts by name the directives that Vosh doesn't support, so you can move them by hand.
- Check the `unparsed` count. It shows the alias or variable lines that the parser couldn't read.

The importer reads `#alias {name} {expansion}` and `#variable {name} {value}`. It also takes `#var` as a short form. Nested braces and escaped braces in the values parse correctly. The importer skips `#nop` lines and comments that start with `;`, and doesn't tell you. Imported aliases overwrite existing aliases with the same name. Variables go into profile scope, so they stay with the profile.

Example. `#import-tintin ~/aabahran.tin` imports the file from your home folder. A skip line of `event=2 ticker=1` tells you that it left out two `event` directives and one `ticker` directive.

Files from other clients go through Settings. Choose Automation and click `Import…`. Choose a MUSHclient, Mudlet, GMUD, or `CMUD or zMUD` export with `Choose file…`, or paste it into `Contents`. Leave `Format` on `Detect automatically` and click `Import`. The summary lists counts, and anything that was refused, not supported, or not readable.

A trigger with the name of a preset trigger stays out, so the preset keeps its own trigger. The summary lists it under `Left out, a preset uses the name`. A Vosh profile export goes in under Characters, with `Import…` beside `New profile`.

### 7.4 Search session logs

<!-- id: characters-and-data.search-logs -->

Vosh logs every session automatically. You can search the logs with regular expressions. A log is the record of one connection, so a session that connects three times saves three logs. The search is in Settings under Logs.

- Open Settings, choose Logs, and click `Search logs…` in the Session logs section. The row counts your saved logs and lines.
- Type a pattern in the search field. Patterns are regular expressions, and the view searches as you type.
- For case sensitive matching, click `Aa`.
- In the menu at the right, choose what to search. The view opens on `Last 7 days`. `This session` reads what the selected session saved since Vosh opened. `Last 30 days` and `All time` go further back.
- Each choice reads the world that the selected session dials. To search only one connection, choose it under `One log`.

The view shows the newest 500 matches under day headings, oldest first, so it reads like the terminal. The count beside the pattern reads like `Newest 500 of 2,423 lines`. Earlier matches load as you scroll up.

Each line keeps its original colors. Your matches are marked as the find bar marks them. With no pattern, the view shows the newest lines. To go back, click `Logs` in the breadcrumb.

Example. The pattern `dragon|wyvern` finds lines that hold either word.

To keep a copy outside Vosh, click the save button to the left of the menu. Choose `Plain text (.txt)`, `With colors (.log)`, or `Web page (.html)`. If you first turn on `Include times` in the same menu, each line starts with the time it came in, as the log view shows it. When the file covers more than one day, the date of each day is above its lines.

Vosh saves every line that the menu chooses, oldest first, to your Downloads folder. The file has a name such as `Vosh log, last 7 days.txt`. The count beside the pattern names the file. A `.log` keeps the colors of the game, so `less -R` or `cat` in a terminal shows them. A web page opens in any browser in the colors of the theme you see when you save. Its heading names your character, the world, and the time it covers.

A file you save shows `> (hidden)` for every line that `#logs forget-passwords` would clean. So the file never holds a password, even one that the log still keeps. With one log chosen, the copy button beside the menu copies that whole log to your clipboard as plain text. Those lines are hidden in the same way.

The count on General doesn't include connections to `127.0.0.1` and `localhost`. The store is `logs.sqlite` in the app data folder. It fills on every connection, so logging needs no setup. To leave a character out, turn off `Log sessions` in the Session logs section. Its sessions then save nothing from the next connection on. A connection to `127.0.0.1` or `localhost`, such as a test server you run beside Vosh, saves nothing until you turn `Log sessions` on.

`Keep logs for` in the same section keeps your logs `Forever` until you choose `1 year`, `90 days`, or `30 days`. Then, once a day, Vosh deletes each whole log that ended longer ago than that. It gives the disk space back a little at a time. A heavy week of play takes about 85 MB.

The first time Vosh deletes a log from a file that an older version wrote, it rebuilds the file one time. On a large log this takes a few seconds. Your game continues while it does. The log writes the lines it held back when the rebuild ends. A search waits until it finishes, and so does a connection, even the reconnect after a drop. Every character shares this setting, because they share one log file.

The log keeps what the game sent and each line you sent, marked `> `. Each line of a snoop starts with the name of the player you snooped, such as `Tolliver|`. So the pattern `^Tolliver\|` finds what the screen of Tolliver showed. Lines that you type at a password prompt aren't saved. Each one shows as `> (hidden)` in its place.

Older versions of Vosh saved those lines in full. So a log saved before you updated can still show your password after a `> `. The game also shows two kinds of password as you type them. These are the password you set for a new character, and any password you give a command such as `password <old> <new>`. The log saves those in full in every version.

To share a part of your play, save it as a scene. With one log chosen, click `Save a scene…` beside the copy button.

To start from the newest log of the session in front, on its last 15 minutes, click `Save a scene…` beside `Search logs…`. Or right click the terminal and choose `Save a scene…`. Choose the log. Then type the time the scene starts in `From` and the time it ends in `To`. Or click a time in the preview to start on that line, and Shift click a time to end there.

`Prompts` and `Your commands` start off. `Channels left out` starts with tell, newbie, pray, immortal, and imp. A tell counts both ways, the tells you send and the tells you get. To keep a channel, click its close button. To leave out another channel, click `Add`.

The lines before you play and after you go back to your account menu never go in. So a scene never names your other characters.

The preview shows every line of the range. What stays out is faint, with the reason beside it. Choose `Text`, `ANSI`, or `HTML` and click `Save scene`. Vosh saves the file to your Downloads folder. Its name has the first room in the range and the day, such as `Thickening Woods, October 3.html`. The main window tells you so, with a button that shows the file.

An HTML scene opens in any browser with the colors of the theme you see when you save. It has a heading with the place, your character, and the time. A line at the end tells what was left out. Lines saved before this version of Vosh have no tag. So Vosh finds their prompts and channels by their text, and a note above the preview tells you so. While `Log sessions` is off for the profile, `Save a scene…` does nothing.

Type `#logs forget-passwords` to count the lines that hold a password. Vosh tells you how many it found and in how many logs. It never shows the lines. Type `#logs forget-passwords now` to blank them. Each one then reads `> (hidden)`. Vosh rewrites `logs.sqlite`, so the old text is also gone from the disk.

On a large log this takes a few seconds, and new game text waits until it finishes. The rewrite needs free disk space of about the size of `logs.sqlite`. When Vosh can't finish it, the lines stay blank and Vosh tells you so. The next `#logs forget-passwords now` finishes the rewrite. A backup of your disk, such as Time Machine, keeps its own copy of the old file. If you copied or shared one of those logs, change your password in the game.

### 7.5 Check for updates

<!-- id: characters-and-data.stay-updated -->

Vosh checks for new builds and installs them in place. The controls are in Settings under General, in the Updates section.

- Open Settings. General opens by default. The Updates heading reads `You have Vosh <version>.` beside a `Check now` button.
- Click `Check now`. The line reads `Checking for updates…`, then `Vosh is up to date.` when there is no newer build.
- When a build is ready, the line reads `Vosh <version> is ready.` and the button changes to `Install and restart`. Click it. Vosh installs the build and starts again on it.
- To check at every start, turn on `Check for updates when Vosh opens`. It is off by default. With it on, a banner shows in the main window when an update waits.

Updates download from the GitHub releases of the project. Vosh checks the signature of every build before it installs it.

`Check for updates` is one of the four switches under Keep the same for every character, also in General. It is on by default, so one setting covers every character. Turn it off when you want one character to check at launch and the others to stay quiet.

## Fix it

### 8.1 Switch terminal renderers

<!-- id: fix-it.terminal-renderer -->

Vosh ships two terminal renderers. On macOS the native GPU surface draws the terminal by default. The `#nativesurface` command changes between it and the xterm renderer from the command line. Windows and Linux always use the xterm renderer, whatever the switch says.

- To draw with the xterm renderer, type `#nativesurface off`.
- To draw with the native surface, type `#nativesurface on`.
- To go back to the macOS default, the native surface, type `#nativesurface default`.
- Restart Vosh. The switch applies only when you restart. The echo reminds you with `restart Vosh to apply`.

The command runs fully in the frontend. It stores your choice locally under the key `vosh.nativesurface`. A bad argument echoes `usage #nativesurface on | off | default (takes effect on restart)`.

On Windows and Linux, Settings under General, then Advanced, holds `GPU rendering`. It draws the xterm renderer with your graphics card. When the terminal draws incorrectly, turn it off, then restart Vosh.

If the text shows in the wrong typeface, open Settings and choose Appearance, then Terminal text. The default font is JetBrains Mono. It ships inside Vosh and works on every computer.

A font list that an older Vosh saved can name a font that Vosh no longer ships. Then the terminal draws in JetBrains Mono. Install the font you want, or choose it in `Font`. The default size is 14.

When the panes or the status line show the wrong typeface or size, check `Font` and `Size` under Panel text, right after Terminal text.

Under General, `Font and size` in Keep the same for every character decides one thing. It decides whether every character shares one terminal font and size, and one panel font and size. Turn it off to let each character keep its own.

### 8.2 Recover a bad connection

<!-- id: fix-it.reconnect -->

The session button in the title band holds the connection controls. Its dot shows idle, connecting, connected, or an error. After a connection fails, the button reads `Not connected`. Point at it to see why.

- Click the session button and choose `Disconnect`. Wait for the dot to go idle.
- To check the address of this session, choose `Edit connection…`. The form holds `Host`, `Port`, and `Use TLS`. The defaults are `play.theforsakenlands.com` on port `1848` with TLS off. Click `Save`.
- Choose the `Connect to` row, or press `Cmd+R` on macOS or `Ctrl+R` on Windows and Linux.

`Use TLS` puts the connection in TLS. Set it to match what the server offers on that port. The default port `1848` needs it off. Settings under General, then Connection, edits the same address for the session in front. It has the `World`, `Host and port`, and `Use TLS` rows. Its `Reconnect when the link drops` row turns the reconnect after a drop on or off for the profile of that session.

A disconnect has other effects. Session variables clear when the next connection opens, so nothing you set with `#var` lasts longer than its connection. Profile variables stay.

The chat pane empties when you choose `Disconnect` or connect to another world. It keeps its lines through a drop and the reconnect after it. When you reconnect, Vosh compares the host and port with your profiles and changes to the best match automatically. After login, it changes to the profile set to log in as your character.

Two other places have the same controls. On macOS the Session menu in the menu bar has the `Connect to` row, `Edit connection…`, `New session…`, and `Disconnect`. The command palette (`Cmd+K`) runs the `Connect to` row or `Disconnect`.

The status line shows how long the game takes to answer you, just before the tick, such as `38ms`. Your computer measures it on the connection, so nothing more goes to the game. It stays dim while you can't feel the delay. From 300 ms, it turns the warn color, because then your commands start to come a pulse late. From one second, it shows seconds in red, such as `1.4s`.

When a command you sent is stuck on its way to the game, the reading counts up from the moment you sent it. So you see a stall while it happens. Anything the game sends ends the wait, a line or a GMCP packet. What you type while a skill lags you never counts, because the game holds it until the lag ends. If the game sends you nothing for more than half a minute after a command, that also counts as a stall. The lines you write into a note never count, because the game doesn't answer them.

The reading shows the session in front. It comes with the first answer of the game, and goes away with the connection. Point at it to read `Round trip to the game`. In a narrow window, a reading under 300 ms goes away before the game time does. A slower reading always stays.

To find out whether a delay came from you or from the game, type `#lag`. It prints the round trip now and how it usually runs over the last 10 minutes. It also prints each stall since you connected, with when it started, its worst reading, and how long it lasted. A stall that waited on a command started when you sent the command, so its time and length are correct. A stall is any period at 300 ms or more. Vosh keeps the last 20 stalls of each session.

### 8.3 Find your data on disk

<!-- id: fix-it.data-on-disk -->

Vosh keeps all of its data in one app data folder named `com.aabahran.vosh`.

- On macOS, open `~/Library/Application Support/com.aabahran.vosh`.
- On Linux, open `~/.local/share/com.aabahran.vosh`.
- On Windows, open `%APPDATA%\com.aabahran.vosh`.

These are the contents of that folder.

- `profiles.toml` lists your profiles and names the active one. After you open a second session or name one, it also lists your sessions in order. Each session has its name, its world, and the profile it plays, so they come back at your next launch.
- `profiles/<name>.toml` holds the snapshot of each profile. It has the connection defaults, aliases, variables, triggers, timers, tick config, and macros. In loadout mode the aliases, triggers, and macros are in `catalog.toml` instead, and the profile file keeps the rest.
- `profiles/legacy/` holds a copy of each profile file as it was when the loadouts migration ran.
- `global.toml` holds the UI preferences that all profiles share.
- `catalog.toml` and `loadouts.toml` show when loadout mode is active.
- `logs.sqlite` stores the session logs, with the `-wal` and `-shm` files beside it.
- `scrollback.txt` keeps the newest terminal lines of the first session you opened, across restarts. It keeps as many lines as `Scrollback size` under Logs in Settings says. Each later session keeps its own lines in a file with its number, such as `scrollback-2.txt`.
- Vosh writes each scrollback file when a connection ends, every few minutes while it runs, and when you quit. So a crash loses at most a few minutes of it. When you close a session, Vosh deletes its file.
- `maps.sqlite`, if you have one, holds rooms that older builds recorded. Vosh no longer reads or writes it.
- `affect_full.toml` remembers the most hours that Vosh saw for each affect, for each character.
- `scripts/` holds Lua files for `#script load`.
- `plugins/` holds plugin folders, each with a `manifest.toml`.

Every TOML save is safe by design. Vosh writes the new text to a temp file. It copies the old file to `<file>.bak.<timestamp>`, with a timestamp in milliseconds. It then puts the temp file in place in one atomic step, and keeps the ten newest backups. A save that fails leaves the old file in place. To undo a bad profile edit, copy the backup you want over the live file.

A `profile.toml` that is still at the root is the single profile file of the builds before named profiles. Vosh moves it to `profiles/default.toml` the first time a build with named profiles starts.

## Reference

### 9.1 Slash commands

<!-- id: reference.slash-commands -->

These are all the slash commands that Vosh knows today.

- `#help` prints the command summary. `#help <words>` opens Help on those words.
- `#alias <name> <expansion>` makes an alias. `#unalias <name>` removes it. `#aliases` lists the aliases.
- `#var <name> [value]` sets or shows a variable of this session. `#unvar <name>` removes it from this session and from the profile. `#vars` lists the variables.
- `#trigger <name> {pattern} <action> [args]` makes a trigger. `#untrigger <name>` removes it. `#triggers` lists the triggers by priority.
- `#prompt game {setting}` and `#prompt fight {setting}` read your prompt in this profile from the codes of your PROMPT and fight prompt. `#prompt {regex}` reads it with a pattern. `#prompt` tells you how Vosh reads it. `#unprompt` stops reading it.
- `#prompt draw on|off` draws your design in place of your prompt in this profile, or shows the prompt of the game.
- `#prompt show text|lifted|pinned` shows your prompt in this profile in the text, lifted on a band in the text, or pinned above the command line.
- `#prompt default` puts the default design of Vosh in place of the design in this profile. It keeps your design as an earlier design.
- `#group <name> on|off` turns a group of triggers, aliases, macros, and timers on or off for every session on the profile. `#group <name>` shows its state. `#groups` lists the groups.
- `#tick`, `#tick interval <secs>`, `#tick reset`, `#tick on {pattern}`, `#tick off`, `#tick fire <command>`, `#tick nofire`, `#tick sound on|off`, `#tick disable`, and `#tick enable` control the tick timer. `#tick reset` starts the count of this session again. Each command that changes a setting changes it for every session on the profile.
- `#tick warn`, `#tick warn at <secs>`, `#tick warn message <text>`, `#tick warn color <name>`, and `#tick warn off` set the tick warning.
- `#lag` prints the round trip to the game now, and how it usually runs over the last 10 minutes. Then it prints each stall since you connected, with its time, its worst reading, and how long it lasted.
- `#script load <name>` loads a Lua file. `#script reload` reads every loaded script again and runs it. `#scripts` lists the scripts.
- `#lua <code>` runs Lua inline.
- `#profile save`, `#profile load`, and `#profile reset` manage the profile snapshot. A load or a reset reaches every session on the profile. In loadout mode all three only show notices.
- `#import-tintin <path>` imports TinTin++ aliases and variables.
- `#logs forget-passwords` counts the lines in your session log where you sent a password. `#logs forget-passwords now` blanks them.
- `#record <name>` starts to record what you type in this session. `#record` shows the status. `#record cancel` discards the recording. `#endrec` saves it as an alias.
- `#qkey <name> <verb>` sets a quick key. `#qkey clear <name>` clears it. `#qkeys` lists the quick keys.
- `#target <args>` works like `tar`. `#target clear|next|prev`, `#tarn`, `#tarp`, and `#tarclear` are its slash forms.
- `#walk <steps>` walks a string of directions such as `3n2e`, one room at a time. `#walk` tells how many steps are left. `#walk stop` stops the walk.
- `#nativesurface on|off|default` sets the renderer on macOS. The change applies when you restart.

Targeting also works with no `#`. Type `tar` to list, `tar <N>` or `tar <substr>` to choose, `tarn` and `tarp` to go through the list, and `tarclear` to clear.

An unknown command points you to `#help`. Errors echo in square brackets.

### 9.2 Keyboard shortcuts

<!-- id: reference.keyboard-shortcuts -->

These are all the built in keys of Vosh, grouped by where they work. On macOS the window shortcuts use `Cmd`, because `Ctrl` belongs to your macros there. The one exception is `Ctrl+Cmd+S`, the key that macOS gives a sidebar. Windows and Linux use `Ctrl`.

Anywhere in the main window.

- `Cmd+K` opens and closes the command palette.
- `Cmd+F` opens the find bar. When you press it again, it puts the caret back in the find bar.
- `Cmd+R` connects the session in front while it isn't connected.
- `Cmd+,` opens Settings.
- `Cmd+/` opens Help.
- `Cmd+Shift+L` shows or hides the panel.
- `Cmd+\` opens or closes the scrollback split.
- `Cmd+J` moves into the snoop while one is open. When you press it again, it goes to the next tab.
- `Cmd+Shift+P` reads your newest prompt aloud while `Read new game lines` is on under Accessibility.
- `Cmd+Option+1` opens Settings on Timers, `Cmd+Option+2` on Aliases, `Cmd+Option+3` on Triggers, and `Cmd+Option+4` on Macros. They also work in Settings. On Windows and Linux the keys are `Ctrl+Shift+1` to `Ctrl+Shift+4`.

For your sessions, in the main window.

- `Cmd+T` opens a new session.
- `Cmd+1` to `Cmd+9` bring the session at that place in the sidebar to the front. Hold `Cmd` for a moment and each row shows its key.
- `Cmd+Shift+]` goes to the next session and `Cmd+Shift+[` to the session before. After the last session, they go round to the first. They use the bracket keys, whatever your keyboard layout types on them.
- `Cmd+W` closes the session in front. It asks first while the session is connected. With one session it closes the window.
- `Cmd+Shift+W` closes the window. It asks first while a session is connected.
- `Ctrl+Cmd+S` hides or shows the sessions sidebar while two or more sessions are open. In a window too narrow for it, the sidebar slides in over the terminal. On Windows and Linux the key is `Ctrl+Shift+S`.

A macro on one of these keys, or on one of the four Settings keys, keeps the key in every session on its profile. Settings tells you so at the top of the macro. The other keys above win over a macro.

On macOS, `Cmd+W` in Settings or Help closes that window. `Cmd+Q` quits Vosh. `Cmd+Q` asks first while two or more sessions are connected.

In the command line.

- `Enter` sends. `Shift+Enter` adds a new line for text with more than one line. In password mode it sends instead.
- `Tab` and `Shift+Tab` go through tab completion. The candidates are your history words, the characters in the room, and names you saw recently. On an empty line, `Tab` moves to the panel and `Shift+Tab` moves back to the terminal.
- `ArrowUp` and `ArrowDown` recall history, filtered by the prefix you already typed.
- `PageUp` and `PageDown` page the scrollback. On macOS press `Fn+Up` and `Fn+Down`.
- `Escape` cancels a paste burst that is in progress, stops a walk, closes the scrollback split, and takes the terminal back to the live tail.
- `Home` and `End` move the caret to the start and end. On macOS `Cmd+Left` and `Cmd+Right` or `Fn+Left` and `Fn+Right` do the same. Add `Shift` to extend the selection.
- `Cmd+A` on an empty command line selects the whole terminal, scrollback included.
- `Cmd+C` with nothing selected in the command line copies the terminal selection.

In the find bar. `Enter` finds the next match, and `Shift+Enter` finds the previous match. `Escape` closes the bar and clears the highlights.

In a snoop. `Cmd+F` opens Find on the tab in front. `Cmd+C` copies what you select. `Escape`, or any key that types, puts you back on the command line.

In the command palette. `ArrowUp` and `ArrowDown` move the selection, and `Enter` runs the entry. `ArrowRight` opens a list such as Choose theme. `ArrowLeft` or `Backspace` goes back out of it. `Escape` goes back or closes the command palette.

In the right click menu. `ArrowUp` and `ArrowDown` move through the items, and `Enter` chooses one. `ArrowRight` opens the Settings list, and `ArrowLeft` goes back out of it. `Escape` closes the list, then the menu.

In Settings and Help. `Cmd+F` puts the caret in the search. `ArrowUp` and `ArrowDown` move through the results. `Escape` clears the search. In Help, `Enter` goes to the next match in the topic you read, and `Shift+Enter` goes to the previous match.

In an Automation list in Settings. `ArrowUp` and `ArrowDown` move through the group headings and items. `Home` and `End` go to the first and the last item. On a heading, `ArrowLeft` folds its group and `ArrowRight` opens it. `Tab` from a heading goes to its group switch, and `Space` turns the switch on or off.

Mouse on the terminal. Wheel up opens the scrollback split. Middle click closes the split and goes back to the live tail. Right click opens the right click menu.

To bind your own keys as macros, go to Settings under Automation, then Macros. Canonical names look like `F1`, `Ctrl+N`, `Shift+F5`, and `Ctrl+Alt+Numpad7`. While the `Numpad movement` preset is on, `Numpad8`, `Numpad6`, `Numpad2`, and `Numpad4` walk north, east, south, and west. `Numpad9` and `Numpad3` go up and down.

### 9.3 Prompt design codes

<!-- id: reference.prompt-codes -->

Your own prompt is a design of text and codes. Customize prompt writes the codes for you when you click the parts of your prompt and choose values. To read the codes or type your own, choose `Edit as text` there.

Until you change it, your design follows the game. Vosh writes it from your PROMPT and fight prompt, so it draws as the game does. It writes it again each time you change them in the game. Your first change makes the design yours. `Same as the game` among the starts follows the game again.

| Code                                           | What it does                                                                        |
| ---------------------------------------------- | ----------------------------------------------------------------------------------- |
| `%hp` `%mana` `%move`                          | Your current Health, Mana or Moves.                                                 |
| `%maxhp` `%maxmana` `%maxmove`                 | The most you can have.                                                              |
| `%pct_hp`                                      | Health as a percent with no sign. Add %% for the sign.                              |
| `%{hp:pct:game}`                               | Health as a percent with no sign, rounded down as the game does, so 37.5 reads 37.  |
| `%hp_bar:10:auto`                              | A bar ten cells wide, colored by how full it is.                                    |
| `%{gold:grouped}`                              | Any value from the picker, in any of its forms.                                     |
| `%{gold:thousands}`                            | Gold in thousands with one decimal, as 12.3K.                                       |
| `%{hour:ampm}`                                 | The game hour as 3PM, with 12AM for midnight and 12PM for noon.                     |
| `%{tick:since}`                                | The seconds since the last tick, as 16s.                                            |
| `%c_green` `%c_hp`                             | A theme color, or Health's color by how full it is.                                 |
| `%{c:hp:steps}`                                | Colors by how full Health is in eleven steps from red to green, one for each tenth. |
| `%{c:#80c8ff}` `%{c:128,200,255}`              | Any color you choose, as hex or as red, green and blue.                             |
| `%bg_blue` `%{bg:#3b4252}`                     | The ground behind the text, in any form a text color takes.                         |
| `%c_default`                                   | Back to the terminal text color. Bold and italic stay on.                           |
| `%c_reset`                                     | Back to plain text with every color and style off.                                  |
| `%s_italic` `%s_bold` `%s_underline` `%s_off`  | Turns a style on, or every style off.                                               |
| `%s_strike` `%s_dim` `%s_inverse`              | Strikes the text through, dims it, or swaps its color and ground.                   |
| `%s_blink`                                     | Makes the text blink.                                                               |
| `%s_double` `%s_curly` `%s_dotted` `%s_dashed` | Underlines with two lines, a wave, dots or dashes.                                  |
| `%{ul:#bf616a}` `%{ul:default}`                | Colors the underline, or gives it the text color again.                             |
| `%nl`                                          | Starts a new line.                                                                  |
| `%{right}`                                     | Pushes the rest of its line to the right edge of the terminal.                      |
| `%{if:fight}` `%{ifnot:fight}` `%{end}`        | Shows what sits between them only in a fight, or only out of one.                   |
| `%{raw}`                                       | Your prompt exactly as the game sent it.                                            |
| `%%`                                           | A percent sign.                                                                     |

Every value in `Insert value…` has codes of its own. The picker shows them beside each form. Your tick, the time, and the date keep counting while your prompt is idle. Vosh draws the prompt again each second they change. It waits while you select text or read back.

When the prompt is pinned, the band keeps counting through both. A line with `%{right}` ends on the last column of your terminal. Vosh draws it again when the width of the window changes.
