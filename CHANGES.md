# Changes

What changed in each version of Vosh, with the newest version first.

## 0.96.4, November 2, 2026

Drag a selection above the scrollback, and the selection continues.

### Fixed

- On Windows and Linux, a selection in the scrollback continues when you drag it above the top. Before, Vosh showed an error and removed the selection.

## 0.96.3, October 29, 2026

Bold text in the terminal shows in one font on Windows and Linux.

### Changed

- On Windows and Linux, the terminal uses a new version of the code that draws the text. This version draws letters correctly in more cases.

### Fixed

- On Windows and Linux, all bold text uses the same font after you start Vosh. Before, some bold letters used a different font until you changed the theme.

## 0.96.2, October 29, 2026

Copy text from the scrollback with the right click menu.

### Fixed

- On Windows and Linux, Copy in the right click menu copies the text that you select in the scrollback. Before, it copied nothing there, and only Ctrl+C worked.

## 0.96.1, October 28, 2026

See your commands in the scrollback, between the lines of the game.

### Changed

- The scrollback that Vosh restores when you open it again also keeps your commands, on all systems. Vosh never keeps a command that sends a password.

### Fixed

- On Windows and Linux, each command that you send stays in the scrollback where you sent it. Before, your commands disappeared from the scrollback.

## 0.96.0, October 27, 2026

Change many automation items at once, and keep your command history after a restart.

### New

- Select many triggers, aliases, macros or timers, and move, turn on, turn off, duplicate or delete them together.
- Rename a group from the menu on its heading, and the new name shows in all four lists and in your loadouts.
- When you give a group the name of another group, Vosh asks before it merges the two groups.
- Vosh keeps your command history after you quit or after a crash, with the newest 1,000 lines of each session.
- Your command history never keeps a password or the text that you type at the login prompts.
- Vosh stops a line that your triggers, timers or scripts send 40 times in a row, and tells you which line it was.
- The tick can send its command a set number of seconds before or after the tick.
- Use Or when you send to start the tick count again from a command that you type.
- `#list` keeps items in order under one name, such as `$foes[1]`, `$foes[-1]` for the last item and `$foes[#]` for the count.
- `#list foes keep` saves a list with the profile, and Lua scripts can read and change lists too.
- Lua scripts and plugins get `json` to read and write JSON, and `regex` to match text as triggers do.
- The menu of a profile on the Characters page of Settings can show its file in Finder or Explorer.
- Restore from a backup returns a profile to an earlier version.
- Set Text encoding to Latin 1 for an older world that shows your accented letters wrong.

### Changed

- Vosh sends the lines from your triggers, timers and scripts at a steady speed, and what you type never waits.
- A burst sends 10 lines at once, and then 4 lines each second.
- The command line shows how many lines wait, and a click on the count stops them.
- Change the speed, or turn the pace off, on the Input page of Settings under Advanced.
- Backups follow a profile when you rename it, and a save that changes nothing does not use a backup.
- Text encoding is on the General page of Settings, and the default UTF-8 still shows accented letters from older worlds.

### Fixed

- On macOS, a line from the game shows in full when it holds a byte that is not UTF-8. Before, the rest of the line disappeared.

## 0.95.6, October 15, 2026

Arrange your panes in arrange mode, fold a pane to its header, and use an earlier layout again.

### New

- In arrange mode, each pane is a tile that you drag, and the other panes move to make space.
- Drop a tile on the top or bottom of a pane to stack the two panes.
- Drop a tile near the left or right edge of a pane to put the two panes side by side.
- Done saves the layout for that character, and Cancel keeps the layout as it was.
- Click the layout button over the panel to open arrange mode.
- Arrange panes in a pane menu, the View menu and the command palette also opens arrange mode.
- In arrange mode, a shelf over the game text holds the panes that you removed and the panes that you can add.
- The shelf includes another Chat pane and your plugin panes.
- Click a pane header to fold the pane to its name, and click again to open it.
- When a pane is folded, the other panes use its space, and its header still shows its counts.
- Vosh keeps the last 20 layouts of each character.
- Click Earlier layouts, in arrange mode or under Panel layout on the Characters page of Settings, to restore one of them.
- The first time that a new version opens, it saves the layout of each character before it changes anything.
- After Close pane, you can click Undo.

### Changed

- Vosh saves the panes that you change shortly before an update, so they are still there after the restart.
- Split right, Split down, Move to sidebar and Add a pane are not in the pane menu now, and headers do not drag.
- Use arrange mode for those changes.

## 0.95.5, October 14, 2026

Put panes in the sidebar, read your stats in a new pane, and copy a trigger, alias or macro.

### New

- The sidebar across from the panel holds panes too, under your sessions or alone when you have one session.
- Drag a pane by its header to move it, or choose Move to sidebar in its menu.
- Add a pane has Add to the sidebar at the bottom.
- Panel layout, on the Characters page of Settings, shows the sidebar next to the panel.
- Drag panes there to arrange any character, also a character that you do not play now.
- A new Stats pane shows your stats, luck, hitroll, damroll, armor and saves as the game sends them.
- Pick Score sheet, Rows or Ledger from its menu, or under Stats on the Layout page of Settings.
- Duplicate alias, Duplicate trigger and Duplicate macro make a copy that you can change and save.
- The name of the copy ends in copy, so no two names are the same.
- A copied macro has no key until you give it one.

### Fixed

- The status bar styles fit in a narrow window, and Vosh removes whole parts before it cuts a name.
- The Dashboard style aligns its captions.
- A profile that never picked a status bar style shows Compact again, the quiet line from before 0.95.3.
- Find says No matches when the text is not in the scrollback. Before, it said Found.
- Stop on a snoop tab closes the tab when the game says that you snoop nobody with that name, also when your prompt is off.
- A snoop tab closes when its player leaves the game. Before, the tab stayed open.

## 0.95.4, October 12, 2026

The letters stay in place when you change sessions.

### Fixed

- On macOS, the game text stays in place when you change sessions. Before, the text stretched to the right for a moment, and sometimes it stayed stretched.

## 0.95.3, October 10, 2026

Choose how your status bar looks, put the panel on the left, and keep spell check on.

### New

- The status bar has four styles, and you pick one under Status line on the Layout page of Settings.
- Meters is the new default and fills the bar with your vitals, your opponent and the tick.
- Compact is the old quiet line, Strip adds small gauges, and Dashboard puts a caption over each value.
- Put the panel on the left under Panel on the Layout page of Settings.
- When the panel is on the left, your sessions and the title band buttons move to the right.
- Copy design copies your whole prompt design.

### Fixed

- Spell check on chat lines stays on. Before, it turned off every second time that you opened Vosh.
- Select all and Copy keep your selection in the text of your prompt design.
- A snoop window closes when you stop a snoop after a hot reboot of the game.

## 0.95.2, October 9, 2026

Pick text sizes in half steps, and give each character its own aliases and triggers with the same names.

### New

- Terminal text, panel text and the command line take half sizes, such as 13.5.
- A font that has fixed sizes only keeps to those sizes and tells you.
- Two groups can each hold an alias with the same name, and while both groups are on, the first group in the list wins.
- Settings shows which alias fires.
- Use a loadout for each character, so that each alias fires only for its character.
- Two groups can each hold a trigger with the same name, and both triggers fire while both groups are on.

### Changed

- When Settings cannot save your aliases, triggers, macros or timers, it tells you why next to Save and marks the rows to fix.
- Vosh removes spaces at the start and end of alias and trigger names.
- Vosh refuses an alias name with a space in it, and tells you why.
- Vosh tells you about a new group that no loadout lists, before the group turns off at your next launch.
- An error names the alias that calls itself.
- `#unalias` and `#untrigger` ask which group you mean when two groups hold the name.

## 0.95.1, October 9, 2026

Write your description in the writing card, save a scene to share, and change the look of the command line.

### New

- The writing card edits your description, history, personality, purpose, notes and the other kinds of board.
- Open the writing card from the Write menu, the command palette, or the notice when the game editor opens.
- Drag the writing card to any place, or pin it into a Writing pane in the panel.
- Drag the corner of its text box to change the size.
- Vosh keeps the position of the writing card for each profile.
- Before it posts, the card compares your note with the copy in the game and tells you which line is different.
- The card also finds a post again after you lose the connection.
- Send for approval and Send for review are in the menu of the card.
- Vosh remembers a check that waits, and reminds you when you send newer text.
- Save a scene saves a part of your log as text, ANSI or a web page in your theme.
- Open Save a scene from Session logs or the right click menu.
- Choose how long Vosh keeps logs, and turn off the log of one profile.
- Vosh keeps your scrollback after you quit or after a crash.
- Set the size of the scrollback on the General page of Settings.
- Pick the mark before the commands that you send, such as ›, >, your own text or no mark.
- Give the mark a color, and dim your commands if you want.
- Pick the background, text size, caret blink and caret color of the command line, and the color of your text.
- Color commands as you type shows aliases, Vosh commands and chat in their own colors before you send them.
- A pill at the start of the command line shows when you are in the game editor, with the line that you are on.
- The pill also shows at a password prompt, in the pager and on a walk.
- Snoop a player, and a split shows their screen above yours, with a tab for each player.
- The status line shows how long the game takes to answer, and `#lag` lists each stall.
- Your prompt can show the change in your health, mana and moves since your last prompt and over the last tick.
- A gain shows in green, and a loss shows in red.
- Find these values under Insert value as Health change and Health this tick, and pick how a zero shows.
- Show each hit keeps the part of a vital that a hit takes pale for a moment before it drains.
- Tab follows the screen in every window, and a screen reader hears the names, keys and regions.
- Turn on the Screen reader section on the Accessibility page of Settings to hear new lines.
- Press Cmd+Shift+P to hear your prompt.

### Changed

- This version is the test build before 1.0.
- Save as file can add times and save a web page.
- Copy as text does not copy password lines, the same as a saved file.
- Log search stays fast on long logs and opens on the last 7 days.
- Settings has eleven pages, and Logs, Accessibility, Vitals and Prompt are now pages of their own.
- Old links to Settings still go to the right row.
- High Contrast has a new design, and High Contrast Light is new.
- On macOS, the Increase contrast setting of your system picks those two themes.
- Menus, buttons and keys look the same in every window.
- A sessions button is next to the window buttons, or press Ctrl+Cmd+S on macOS or Ctrl+Shift+S on Windows and Linux.
- On macOS, Cmd+Option and a number open the Settings pages, because Cmd+Shift+3 and Cmd+Shift+4 take screenshots.
- Help has new text in short, plain sentences.
- Berkeley Mono is not in Vosh now, and a setting that named it uses the bundled JetBrains Mono.
- If an error occurs, its notice stays in the corner, counts repeats and closes with Close or Escape, so you can continue to play.
- On Windows, if SmartScreen warns you about the unsigned build, choose More info and then Run anyway.

### Fixed

- Things and people in a room take their colors after a walk or a goto, not only after a look.
- Your target takes its color on the first look.
- A right aligned part of a pinned prompt stays whole.
- A line that Vosh prints after a prompt starts on its own row.
- Submenus stay open while the pointer moves into them.
- A reconnect from the account menu knows which character you play.
- Vosh does not stop with an error when a fast scroll to the bottom closes the scrollback split.

## 0.9.0, October 6, 2026

Play several characters in one window, write plugins that draw panes, pick a vitals style, and get alerts.

### New

- You can open more than one session in a window, and each session keeps its own terminal, command line, history and scrollback.
- Vosh opens all your sessions again the next time that you start it.
- A sessions sidebar lists your sessions while two or more are open.
- Each row shows the room or fight of that character, their health, and a count of the tells and mentions that you missed.
- Put the pointer on a session row to see its character, world, room, vitals and time online.
- Double click the name of a session, or press Return or F2, to rename it.
- When you fold the sidebar, the session button in the title band shows the same rows.
- The session button also shows the total of what waits in your other sessions.
- Move between sessions from the keyboard, and pick a session to go directly to its command line.
- Vosh asks before you close a session or quit while you are connected.
- Vosh can connect again when you lose the connection, and a notice shows the seconds to each try.
- The notice has Cancel and Reconnect now.
- Turn reconnect on or off under Connection on the General page of Settings.
- Add an Alert row to a trigger to post a system banner, play a tone, or do both.
- Five alert presets cover tells, your name, an attack on you, low health and your connection.
- The alert presets are under Presets on the Automation page of Settings.
- Choose a vitals style for your health, mana and moves, in the panel or the status line.
- The vitals styles are Rows, One line, Ledger, Gauges, Pips and Text.
- Customize vitals sets the colors, the order and the low marks, and a right click on the vitals gives quick changes.
- Write your own vitals text, or keep the template that you used before.
- The Scripts page of Settings installs, exports, turns on and off, and reloads your Lua plugins, and shows their output.
- A plugin can draw its own pane with `mud.pane`, with rows, meters, lines and rules.
- Add a plugin pane from Add a pane, as you add any other pane.
- A plugin can send an alert with `mud.alert`.
- Each session runs its own Lua, and a plugin that runs too long or uses too much memory stops before it can freeze Vosh.
- Open up to four Chat panes, and a second pane starts on tells while your first pane shows Everything else.
- Pick one or more channels for each Chat pane.
- One Chat pane can show Everything else, which is the channels that no other pane picks.
- Import a profile that another player exported from Vosh, as a new profile or over one that you have.
- Each pattern of a trigger can match as Text, Starts with or Regex.
- Turn a whole group of triggers, aliases or timers on or off with the switch on its heading.
- Timers can be in groups.
- The Numpad movement preset moves you with your number pad.
- Type `#walk` and a string of directions, and Vosh walks them one step at a time.
- Press Esc to stop a walk.
- Color vision on the Appearance page of Settings changes the colors that your eyes confuse, for Deuteranopia, Protanopia and Tritanopia.
- Fit game colors keeps the colors of the game readable on any theme.
- New themes are in the gallery, such as Triad, Rubric, Harbor Dark, Iceberg Dark, Srcery, Nightfly, Melange Dark, Melange Light and Modus Vivendi.
- The panes can use their own font and size, under Panel text on the Appearance page of Settings.
- The status line can show the game time on a 12 hour clock.

### Changed

- A new install starts on Triad, with Rubric as its light theme.
- One Dark, Vellum and Everforest Light are gone, and Vosh moves your choice to the theme that replaces each one.
- Collapse repeated lines can also fold fight lines and attack lines.

### Fixed

- The pinned prompt does not leave empty rows under the login menu before your first prompt.
- A blank Enter with your prompt pinned does not leave an extra empty row.
- When you save your aliases, an alias that a plugin stopped stays off.
- The switches on trigger and alias groups work again.

## 0.8.1, October 3, 2026

Use five new themes, a 3D map, room colors, text that blinks and new prompt forms.

### New

- Five new themes make 20 in the gallery, with Solarized Dark and Light, Everforest Dark and Light, and Green Screen.
- Green Screen shows green text on black, as old terminals did.
- Text that the game sets to blink now blinks with both renderers.
- Turn off Blinking text on the Appearance page of Settings, under Advanced, to keep the text still.
- Vosh turns off Blinking text when your system reduces motion.
- Your prompt design can blink too, and More styles in Customize prompt lists blink and every kind of underline.
- Customize prompt has new forms for numbers, so gold can show in groups of thousands.
- A percent in your prompt can cut its decimals, as the game does.
- Customize prompt has new time forms, with a short 12 hour form for the game hour.
- The tick in your prompt can count the seconds from the last tick.
- A value in your prompt can change through ten colors as it fills.
- A prompt row can move the rest of its text to the right edge.
- The new Room, time and weather colors preset colors the room around you, the clock and the weather.
- With that preset, exits are green, things and people are yellow, day and night are blue, and your target is bright red.
- Triggers can match what a room lists, or the line of your target.
- Keep highlight colors readable, on the Appearance page of Settings and on by default, makes a faint trigger color lighter or darker.
- Turn on Collapse repeated lines on the Appearance page of Settings.
- A line that the game sends again and again then takes one row with its count, also in the scrollback.
- The map has a 3D style that stacks the floors around you.
- Drag to turn and tilt the 3D map, and scroll or pinch to zoom.
- Set Running out at and Almost gone at for each character to choose when affects warn you.
- Affects have a new style, Draining chips.
- The right click menu of the terminal opens any Settings page.
- A gear at the right of the title band opens Settings on all systems.

### Changed

- Each command that you send now shows after a grey ›.
- Turn off Mark your commands, under Command line on the Input page of Settings, to show them with no mark.
- The map pane shows the room name in its terminal color, with its terrain and region.
- A tracked affect that you do not have shows a soft dotted edge, so you can see it quickly.

## 0.8.0, October 1, 2026

Use a new main window with a panel of panes, a new Settings window and a prompt that you design.

### New

- The new main window has the terminal on the left and a panel of panes on the right.
- A flat command line and a status line are under the terminal.
- A slim title band holds the session button, which connects to your saved world, edits it, makes a new one or disconnects.
- The window title names your character and world, so your taskbar shows which character is which.
- The panel holds the Map, Affects, Group, Chat and Staff queues panes.
- Split the panes right or down from the menu of a pane, and drag the lines between them.
- Each profile keeps its own panel arrangement, and Reset panel layout in the View menu restores the default.
- Health, Mana and Moves are at the foot of the panel, each with a thin meter.
- In a fight, your opponent gets a row above them.
- Under Vitals on the Layout page of Settings, choose rows or one line, how the numbers read, and the meter.
- Turn on Warn before you run low to see yellow and then red when a vital is low.
- While lamented tears hides your values, your vitals show ? in dim text.
- The status line shows your target, the tick, the game time and each moon as a phase icon in its own color.
- Vosh plays a soft tone on each tick, and you can turn it off with `#tick sound off` or under Timers in Settings.
- Vosh reads your prompt, and on The Forsaken Lands it learns your prompt codes from the game at login or when you type `prompt`.
- The Prompt section of the Input page of Settings shows where your prompt codes came from, and warns you when something is wrong.
- On other games, select your prompt line in Customize prompt, or type `#prompt {regex}`.
- Customize prompt opens a card over your prompt line, from the right click menu, the palette or the Prompt section of Settings.
- In Customize prompt, start from a ready design and click any part to change its content, color, background and style.
- Curly and dotted underlines are among the styles.
- Customize prompt works fully with the keyboard and with a screen reader.
- Choose where your prompt shows with `#prompt show` or in the Prompt section of Settings.
- In the text keeps the prompt where the game sends it, and Lifted moves every prompt to a band.
- Pinned shows only your latest prompt, on a band above the command line, and the log still keeps it.
- The default prompt design shows your vitals, colored by how full they are, with your exits and gold.
- In a fight, the default prompt design also shows the health of the tank.
- Turn on Draw your own prompt, or type `#prompt draw on`, to draw your design.
- A new Settings window has six pages in a sidebar, General, Appearance, Layout, Input, Automation and Characters, and Cmd+comma opens it.
- Search in Settings goes directly to a setting.
- Outside Automation, each change in Settings saves when you make it.
- Automation is one editor for triggers, aliases, macros, timers, presets and loadouts, with folders, a filter and a Save bar.
- On Characters, you can edit any profile without a switch, link a profile to a character, and track affects as chips.
- The Affects pane has three styles, Timers, Countdown and Grouped chips, and you pick one for each profile on the Layout page of Settings.
- Your tracked affects are first in the Affects pane, harmful affects show in red, and you can tint the affects that you must cast again.
- The Chat pane shows one line for each message, in the game color of each channel.
- Give any channel a new color from the menu of the Chat pane.
- The Map pane lists your room, exits and people under the map, and keeps the last map after you disconnect.
- Help opens in its own window with a sidebar, a ranked search and every topic written again for the new layout.
- Open Help with Cmd+/ or type `#help` and a few words.
- Two new themes are in the gallery, Rosé Pine and Vellum, a warm light theme like paper.
- On the Appearance page of Settings, you can import Ghostty, iTerm2, Kitty and Alacritty themes.
- You can also follow the light and dark appearance of your system, and pick a line height.
- The new command palette opens on your last three commands and finds each command, setting and alias as you type.
- New shortcuts connect, open Settings and Help, and show the panel and the scrollback split.
- On macOS, Vosh has its own menu bar, the native title bar and a frame in the colors of your theme.
- Vosh has a new icon, Moonpath, on all systems.
- `#logs forget-passwords` counts the passwords that older versions saved in your log, and `#logs forget-passwords now` removes them.
- Lua from `#lua`, `#script load`, timers and plugins can start timers, send input and set prompt values.

### Changed

- Your old dock layout moves to the new panel.
- Your custom prompt from 0.7 moves to the new design.
- On macOS, the shortcuts use Cmd only, so Ctrl stays free for your macros.
- Your session log does not keep passwords, and a line that you type at a password prompt shows as > (hidden).
- The top bar, the bottom status bar, the panel strips, the room strip and the affects bar are gone.
- Of the panes that split the terminal, only the scrollback split stays.
- Turn loadouts on and off under Loadouts on the Automation page of Settings.
- The tick follows the game hour on The Forsaken Lands, or a line that matches your Reset on pattern.
- The prompt trigger that `#prompt` made in Vosh 0.7 moves into your profiles automatically.
- Only triggers that match Prompts see your prompt now, and Vosh names each trigger that should change to Prompts one time.
- Each theme paints the whole window from its terminal colors, and Use the theme's colors for MUD text is on unless you turned it off.
- Vosh ships JetBrains Mono and not Berkeley Mono, but if you have Berkeley Mono on your computer, Vosh still uses it.
- Both renderers always use the same font face.
- The fast native renderer uses the colors of your theme for selection and find, and draws every underline style.
- Log search is under Session logs on the General page of Settings now.
- Your command goes to the game before Vosh logs it.

### Fixed

- A reply from the game that arrives in several parts shows all at once.
- Each Vosh window opens in your theme.
- A save in Automation does not delete what you made with `#trigger` or `#alias` in the meantime.
- The Appearance page of Settings opens without a freeze. Before, the app froze for two to three seconds.
- Cmd+K and then Enter in the command palette does not disconnect your session.
- Aliases that run Lua work. Before, they sent nothing.
- `#script reload` does not make a script answer game data two times.
- On Windows and Linux, `#profile save`, `#profile load` and `#script load` work.
- TinTin++ and CMUD imports keep accented letters.
- Vosh never writes to a profile that it could not read.
- In loadout mode, a change of profile keeps the automation of that profile.
- A macro group that you turn off stops immediately.
- On macOS, menus and dialogs open over the fast native renderer.
- When you type after the game disconnects, Vosh shows [not connected].

## 0.7.2, September 14, 2026

Pick a caret shape, keep the vitals layout that you choose, and see trigger washes in your theme colors.

### New

- Pick one of seven caret shapes on the General page of Settings, such as block, outline or pipe.

### Changed

- Each caret shape uses the same space, so the input row does not move when you change the shape.
- A trigger wash uses your theme colors for its background, accent bar and text, so the line looks marked and not covered.

### Fixed

- Your vitals layout stays as you set it when you open Settings, move the bar to a different panel or restart. Before, it changed back to ledger.
- Pick your vitals layout again after this update, because the old fault saved ledger in your profile.

## 0.7.1, September 11, 2026

Repeat a command on a timer, and show notes on your screen.

### New

- A timer sends a command at an interval while you are connected, and you add it on the Timers tab of Automation in Settings.
- Each timer has a name, an interval in seconds, a command and a box that turns it on or off.
- Timers save with your profile, next to your aliases and triggers.
- A timer command can be a Lua script of more than one line, in the same code editor that triggers use.
- An alias or a `#lua` command in a timer runs as it does when you type it.
- When Vosh stops for a moment, a timer skips the times that it missed and does not send them all together.
- `#echo` shows text on your screen but does not send it to the game. It replaces each `$variable` with its value, so a timer or a trigger can show you the current state.
- `#showme` does the same as `#echo`.

## 0.7.0, August 28, 2026

Use the new Obsidian Ember design, open the command palette, and find each setting in the new Settings window.

### New

- Obsidian Ember is the new default theme, a warm palette that is almost black, with one ember accent.
- The new design has its own fonts and a mark of three moons.
- A session chip in the top bar shows your character, in place of the old connect row.
- The side panels are cards on carved channels, and you drag a channel to change the width of a column.
- The vitals readout has four layouts on the Vitals page of Settings. Ledger puts the numbers in columns, gauges fills a bar, pips shows ten cells, and strip puts every vital in one row.
- At the top or bottom of the window, the vitals take a width in pixels and a place at the left, center or right.
- A vital below 20 percent turns red and pulses. It stays red until it is above 25 percent, so a small gain does not make it flash.
- A search box at the top of Settings takes you directly to any control.
- The command palette opens with Cmd+K, or Ctrl+K on Windows and Linux, and runs commands, shows or hides panels, opens Settings tabs and runs aliases.
- You can show the session, a log and a chat feed in panes of the terminal area, and the status bar lists them.
- Right click the terminal to open a menu of its actions.
- A trigger can wash a line with a color and add an accent bar at its left, so an important line is easy to see.
- The chat pane gives each channel its own pastel color, so tells, gossip, clan and the other channels look different as they scroll.
- On the Themes page of Settings, you can change the terminal base palette and reset it to the standard chart. This palette holds the sixteen ANSI colors that Vosh uses when theme tint is off.
- The new help in the app has short steps for each task, and its reference lists every slash command.

### Changed

- The new Settings window puts every tab in a rail at the left, in groups with their own icons. The groups are Appearance, HUD, Automation, Characters, Session and Tools.
- Every control in Settings has the same style, with underlined fields and boxed lists, and Typography and Tick & chips are tabs of their own.
- Steps that gave no feedback now ask you to confirm or show a small note in the corner. Vosh asks before it deletes a theme.
- Terminal tint follows each theme by default, and Obsidian Ember has its pastel ANSI colors on.
- The prompt on the command line is one chevron.
- The command line gets the focus again after you copy terminal text, or after a middle click moves the scrollback to the bottom.
- PageUp and PageDown scroll the scrollback, and the terminal keeps its size.

### Fixed

- The input row stays in place and keeps its font as you type. Before, it moved by one pixel or changed its font.
- The password prompt and the command prompt have the same size and spacing.

## 0.6.5, August 10, 2026

Immortals can see the work in their staff queues in a new panel.

### New

- Turn on imm (staff queues) on the Panels page of Settings to see the work queues of the game. The queues include description checks, applications, journals, notes, bugs and votes.
- Only the queues that need work show, and the most urgent queues are at the top.
- A count turns red when work is past its deadline, and amber when the deadline is near.
- The game sends no staff data to mortals, so the panel stays empty for them.

## 0.6.4, August 6, 2026

Triggers and aliases save themselves, and each macro key shows its command when you press it.

### New

- A macro key shows its command in the terminal when you press it, so you can see that the key works before the game answers.
- Turn off Echo macro commands on the General page of Settings if your macros show too much text.

### Changed

- Triggers, aliases and the on or off state of each group save themselves a few seconds after you change them, and again when you quit.
- In loadout mode, `#profile save`, `#profile load` and `#profile reset` tell you that saves are automatic. They do not write files that can restore items that you deleted.

### Fixed

- New triggers and aliases stay after a restart. Before, they disappeared.
- When you turn off a group, every item in it stops, and this includes the items that are on.
- A save from the editor keeps the groups that you turned off. Before, it turned them all on again.
- When you turn off all loadouts, your catalog stays dormant through restarts and profile changes.
- When you fight blinded characters, the group pane shows one row for each name. Before, it filled with copies of "someone".
- The map draws one time for each move, not five times, so an open map pane does not make your commands slow.
- A middle click that opens or closes the scrollback split keeps the focus on your command line, so Enter and your macro keys still work.

## 0.6.3, July 5, 2026

Use a scrollbar, drag the scrollback split smoothly, and see a custom prompt that does not flash.

### New

- A thin scrollbar at the right edge of the terminal shows your position in the scrollback. Drag its thumb to move through the text, or click the track to go to a point.
- A middle click opens the scrollback split, and a second middle click returns you to the live text.
- When you select text, a short note "copied N chars" shows in the bottom corner, so you know that the text is on your clipboard.

### Changed

- The divider of the scrollback split moves with your mouse one pixel at a time, not one row at a time.
- You can grab the divider anywhere that it shows the resize cursor, and that cursor stays on the divider. Before, the cursor showed over the whole terminal.
- Your typed command is on the same row as the custom prompt, as in a MUD.

### Fixed

- On the native renderer, long lines wrap between words. Before, they broke in the middle of a word at the right edge.
- The text does not jump when the scrollback split opens for the first time.
- The split divider color setting works on the native renderer, and a change shows at once.
- A click in the terminal puts the focus on the command line again, as a click in any other part of the window does.
- The custom prompt draws again in place and does not flash. Before, it showed a blank row, moved the text higher for a frame or left a blank line above it.
- On a server that ends its lines in an unusual way, your typed commands do not cover the prompt.

## 0.6.2, July 4, 2026

Color fields wait until you type a full color, and the macro and trigger lists are easier to use.

### New

- The tick warning color takes hex such as `#ff8800`, short hex such as `#f80` and 256 color numbers such as `196`, and also color names.
- Preset triggers are in their own section, which you can collapse.
- Type a group name in the group field of a preset trigger to move it to one of your groups.

### Changed

- The row for a new macro is at the top of the macros panel, so you do not need to scroll to add a key.
- Macro and trigger groups stay collapsed as you left them, after you change tabs or restart.

### Fixed

- The group that you set on a preset trigger stays after a restart. Before, the next launch removed it.
- Settings does not make the window blank while you type a color. Before, half a color made parts of the window transparent.
- The theme editor and the color fields for the split divider and sent commands use only a full color. If you leave a typo, the field returns to the last good color.

## 0.6.1, July 2, 2026

Try the native renderer on Windows and Linux.

### New

- On Windows and Linux, type `#nativesurface on` to use the native GPU renderer for the terminal. Then restart Vosh.
- `#nativesurface off` uses the previous renderer on any system, and `#nativesurface default` uses the default of your system.
- The native renderer stays the default on macOS only, because nobody has tested it yet on real Windows and Linux computers.
- If the terminal looks wrong, `#nativesurface off` and a restart return it to the previous renderer. A report of what you saw is a big help.
- `#help` and the help in the app list the new command.

## 0.6.0, July 2, 2026

On macOS, see a smoother terminal, click links in the game text, and color more of your custom prompt.

### New

- On macOS, the terminal draws on a native GPU surface by default, so it scrolls more smoothly in heavy fights.
- Move the pointer over a URL in the terminal, and it turns blue with an underline. Cmd+click the URL to open it in your browser.
- The custom prompt can color any part with color names, 256 color numbers, RGB values or hex codes, for the text and the background.
- A number in your prompt can take its color from its percent, so your health changes from green to yellow to red.
- Bold, italic, underline, inverse and strikethrough work inside the prompt, and new tokens show the time, the date and percents. The hint in Settings shows how to write them.
- Turn on Bright text as bold on the General page of Settings to show bright text in the bold font. With it off, bright text has the normal weight.

### Changed

- On macOS, the scrollback split, your saved scrollback, selection and find all work on the new surface.
- On macOS, a drag selects text and copies it when you release the mouse, Cmd+C copies, and find takes a regex and marks each match.
- On macOS, the new surface keeps your theme colors, the tint switch, your exact font size and weight, and the echo of your commands.
- If the terminal looks wrong on macOS, set `vosh.nativesurface` to `0` in the local storage of DevTools. Then reload to use the previous renderer.
- Windows and Linux keep the previous renderer for now, because the native renderer needs tests on real computers first.

### Fixed

- Menus and dialogs over the terminal show in full. Before, part of them was behind the terminal.
- The text under a menu or a dialog does not move while it is open.
- When you select text in the live part of an open scrollback split, you get the correct characters.

## 0.5.3, June 25, 2026

Open the scrollback split, and your history shows at once.

### Fixed

- The history pane of the scrollback split shows your scrollback when it loads, so it is never blank and never slow.
- The pause that an earlier fix added before the text showed is gone, and the GPU renderer stays the default.

## 0.5.2, June 23, 2026

See your history the first time that you open the scrollback split.

### Fixed

- The history pane shows text on your first scroll again. Before, from 0.5.0, a split that you opened with the mouse wheel or PageUp could stay blank until a second scroll.

## 0.5.1, June 23, 2026

See your history when you open the scrollback split.

### Changed

- The history pane draws with the standard renderer, so it shows every time, and the live pane keeps the GPU renderer.

### Fixed

- The history pane of the scrollback split is not blank when the split opens. Before, it showed nothing while the scroll depth counter increased.

## 0.5.0, June 23, 2026

Send many lines from the command line at once, use the GPU renderer, and find your way in a simpler Settings.

### New

- The command line takes more than one line, with a number for each line at the left. Press Shift+Enter to start a new line.
- When you press Enter, each line goes to the game as a separate command, so you can send a full sequence at one time. One line works as before.
- The Vitals tab has a live preview, and you drag it to change the fill and see your glyph, color, layout and percent choices.

### Changed

- The vitals readout and the tick timer each have a tab of their own in Settings.
- The GPU renderer is the default, so the terminal scrolls more smoothly in a burst of output. If the GPU stops, the terminal changes to the software renderer and does not become blank.
- To use the old renderer, turn off GPU rendering on the General page of Settings. Then reload.
- The Settings window has clear section headers over its tabs, and many labels and hints are shorter and plainer.
- New profiles start with the Kanso Zen theme.
- Vosh is faster under load, because output gets to the screen with less work. Logs save in batches, and triggers repeat less work on each line.

### Fixed

- A paste of more than one line does not clear the text that you typed. The paste sends each line, and your typed text stays, selected or not.
- The last text that the game sends when you quit goes to the terminal, the scrollback and the session log. Before, those lines disappeared, so your next launch did not show them.
- The window stays smooth in heavy output and does not lose frames on each round of a fight.
- The match counter in the find toolbar stays steady while output arrives. Before, it changed as each new line arrived.

## 0.4.3, June 7, 2026

Drag the divider of the scrollback split, and the top pane moves over the live pane as in CMUD.

### Changed

- As you move the divider, the top pane becomes larger or smaller over the live pane.
- The text stays in place while you drag. Both panes use one scrollback, so the rows that the top pane shows match the top of the live pane.

### Fixed

- Each snap draws in one step, so the shake at each snap and the refresh after you release the divider are gone.
- When you close the split with a scroll to the bottom, Esc or a middle click, the text does not visibly reflow.

## 0.4.2, June 7, 2026

Drag the divider of the scrollback split smoothly, as in CMUD.

### Changed

- The live pane stays in place in the background, and the history pane is on top of it, with a height that the divider sets.
- As you drag, the divider moves over the live output, and both panes stay in place.
- The divider snaps to full rows, so it stops between lines and never cuts a line in half at the bottom of the history pane.

### Fixed

- When you release the divider, nothing reflows. Before, the rows shook one at a time and jumped when you released it.

## 0.4.1, June 6, 2026

Pick one of five new themes, use a target variable, and turn a group on or off with one command.

### New

- Five new themes are in the Theme list on the General page of Settings.
- Dracula at Night puts the purple, green and pink of Dracula on darker window colors, and Monokai has the Sublime colors with their magenta accent.
- One Dark has the cool slate look of the Atom editor, and One Half Dark gives it brighter text and cooler surfaces.
- Tango Dark is the GNOME Terminal theme, with strong primary colors on a warm dark background.
- A new `${target}` variable holds the name that you last set with `tar`. Use it in an alias such as `kill ${target}`, or in the Send of a trigger such as `bash ${target}`.
- In Lua, read the target with `mud.var("target")`.
- `${target}` clears when you disconnect or type `tarclear`, and trigger patterns and highlights do not expand it.
- Type `#group <name> on` or `#group <name> off` to turn the triggers, aliases and macros of a group on or off together. For example, turn on a highlight group for one fight, and then turn it off.
- `#group <name>` alone shows if the group is on for triggers, aliases and macros, and `#groups` lists every group in your profile.
- In Lua, `mud.set_group_enabled(name, enabled)` does the same as `#group`.

### Fixed

- The percent chip in the template vitals layout is level with the text around it. Before, that text looked taller and looser than in the plain inline layout.
- When your saved scrollback is short, game output starts at the bottom of the terminal, as it does when the scrollback is empty. Before, it collected in the middle with a large gap when you connected.

## 0.4.0, June 5, 2026

Write Lua in aliases and triggers, get vitals from your prompt, and see doors and new vitals styles.

### New

- Aliases and triggers can run Lua in a sandbox, with the capture groups of the match in a `captures` table. You write the code in a new code editor in Settings, in lua mode for an alias or a Script effect for a trigger.
- The new `#prompt {regex}` command, as in TinTin, takes your vitals from any prompt that the server sends. Each named capture, such as `(?<hp>\d+)`, sets the value of the same name and has priority over GMCP.
- Values from your prompt use the chip style that you picked in Settings. Vosh gags the line that matches, and `#unprompt` removes the rule.
- `#prompt` works with no GMCP, so your bar can show vitals from the prompt of a MUD that sends none over GMCP.
- The trigger editor has a new target list. Prompt matches the open prompt that the server sends at GA or EOR, and line, the default, matches full lines.
- The map shows hidden doors in pink dashes, closed doors in amber and locked doors in red. These states come from the GMCP map data, and a hidden door with no room after it shows as a short line.
- Your own commands go into session log files with a `>` in front, so `#log` search finds both sides of the session.
- On macOS, spell check works in the input line, but it checks only chat lines that start with `say`, `tell`, `gossip`, `emote` and similar commands. Game commands do not get a red line.
- On macOS, spell check is off at first, and the spell check chat lines switch on the General page of Settings controls it.
- Two new vitals styles, drain and badge, show each vital as a chip, and drain fills and glows in the color of the vital.
- Pick drain or badge in the new inline style list in Settings, then Panels, then Panes, then Vitals. Plain stays the default.
- The new % mode list colors the percent in drain and badge, from the color of the vital to red as the value decreases. Its other choices keep the color of the vital or make the percent pink.
- The new % chip list puts the percent of the inline and template layouts in a pill, soft tint, glow ring or drain fill. It also works with the `%pct_hp`, `%pct_mn` and `%pct_mv` tokens, and it is plain until you pick a chip.

### Changed

- Commands that you type show in the history pane of the scrollback split, next to the game output, with no delay. The scrollback that Vosh restores at your next launch holds only game output.
- Commands joined with `;` or a line break go to the game as separate lines, and the escapes `\\`, `\;` and `\\\n` still work.

### Fixed

- The terminal scroll is calmer with a trackpad, and small, unsteady movements do not start a long scroll.
- A trigger such as a disarm chain does not send `get 1.;wield 1.` to the game as one broken line.
- A trigger that gags a line and echoes text from Lua puts the echo where the gagged line was. Before, you saw a blank row and then the echo on a new line.
- A new session with no scrollback starts output at the bottom of the terminal, also after you resize the window. Before, a large gap showed between your last prompt and the room strip and vitals chip.
- `#profile load` and `#profile save` use your active profile, not the old single settings file. Setups from before Vosh had many profiles still use that file.
- Triggers, aliases and macros in your profile are on top of the shared catalog, and your version has priority when names are the same. Before, the shared catalog replaced them at launch.
- A trigger that you make in your profile or with `#prompt` is still there after the next launch.

## 0.3.15, June 4, 2026

See the trend of your vitals, put the combat chip in its own panel, and scroll calmly with a trackpad.

### New

- A new history layout draws a braille trend grid under your vitals bar, so you can see each drop in health, mana and moves.
- Pick the history layout in Settings, then Panels, then Panes, then Vitals.
- Give the combat chip its own panel in any zone from Settings, then Panels, then Combat.
- You can drag the inner edge of the chat pane to set its height, and Vosh keeps that height between launches.

### Changed

- Bar style and layout are now two separate choices, so solid, track and ramped bars all work with the trend grid.
- The live preview in Settings shows your bar style and your layout.
- The default zone, hidden, keeps the combat chip next to your vitals.
- In the bottom zone, the combat chip joins your vitals bar as one block, and in other zones it gets a full pane, centered vertically.
- The bar of the combat target uses your vitals bar style and bar font, in one red that becomes darker as your opponent dies.
- The combat chip fits on one line with swords, the target name, the bar and the percent, and no condition word.
- The low health vignette has a name that tells you what it does, and it keeps your choice.
- A selection in one pane of the scrollback split clears the selection in the other, so a copy always takes your latest selection.

### Fixed

- On macOS, the terminal and the history pane of the scrollback split scroll as far as your fingers move, so a swipe stays calm.
- When you change to another chat channel, such as all, chat or tell, the pane shows the latest message. Before, it kept the scroll position of the last channel.
- A selection clears when new output moves it off the top. Before, a gray ghost highlight stayed on the screen.

## 0.3.14, June 3, 2026

See your vitals settings in two columns, add a red glow for low health, and pick from more bar fonts.

### New

- The low health vignette makes a soft red glow pulse at the edges of the window while your health is below 30 percent.
- The vignette is over the window, so your bar, template or inline layout shows as usual under it.
- The bar font picker adds MonoLisa, Menlo, Consolas and Courier New.
- A line under the font list tells you which name of MonoLisa your system has, or warns you when it finds none.

### Changed

- The vitals settings are a two column console, with the controls on the left and a live preview of your real bar on the right.
- The footer of the preview shows your layout, style, width and font, so you can check them without a scroll.
- When the Settings window is narrower than 820 pixels, the console changes to one column.

## 0.3.12, June 3, 2026

Pick a vitals style from one list, and give your bar its own font.

### New

- A new bar font picker changes only the bar glyphs, so labels, numbers and panel frames keep your app font.
- Pick the bundled Berkeley Mono or JetBrains Mono for clean block and braille glyphs.
- You can also name any monospace font on your computer, and Vosh loads it itself so that macOS shows it.
- A font family field under the list takes any list of fonts that you type.

### Changed

- One style list and a small width field replace the crowded row of style, filled, empty and width at the top of the vitals settings.
- Each style sets the bar style and its glyph pair, such as solid with block glyphs, ramped with full braille, or track with no glyphs.
- If you edited your glyphs by hand, the list shows a custom entry with the filled and empty fields.

## 0.3.11, June 3, 2026

Pick from five new glyph sets for your vitals bars, and use the smooth ramped style again.

### New

- Five new glyph sets in the style of btop are dark and light shade (▓ ░), block and medium shade (█ ▒), braille full (⣿ ⣀), braille mid (⠿ ⠤) and braille thin (⠶ ␣).
- The ramped bar style is back, next to solid and track in the style list.
- Ramped draws the end of the bar in eighths of a character (▏▎▍▌▋▊▉), so the bar moves smoothly and not a whole cell at a time.
- At a bar width of 20, a solid bar moves in steps of 5 percent and a ramped bar in steps of 0.625 percent.
- Ramped shows most clearly in Berkeley Mono or JetBrains Mono, the fonts that come with Vosh.

## 0.3.10, June 2, 2026

Drag a vitals bar in the preview to test your colors at each fill level.

### New

- Drag the health, mana or moves bar in the vitals preview to move it from 0 to 100 percent.
- The colors change through the ramp as you drag.
- The header above the preview shows the percent of each bar, so you can check a custom color at each fill level.

## 0.3.7, June 2, 2026

Give each vital its own color, and keep your vitals readable in a narrow panel.

### New

- Pick a color for each vital from a swatch in Settings, then Panels, then Panes, then Vitals.
- Leave a color blank to keep the default green, blue and orange ramps.
- With drain through red as the bar empties on, the bar becomes red only in its bottom half.
- With that setting on, a dark color stays true, so a dark green does not become a muddy olive.
- With that setting off, the bar keeps your color at each fill level.
- The preview in Settings shows the colors that you pick, so the live bar looks the same.

### Changed

- Bar width in Settings now sets the largest width of the bar, so in a narrow panel the bar becomes narrower first.
- A narrow bar shows its true fill with the track and solid styles, so a bar at 50 percent always shows half full.
- A custom vitals template with bar tokens breaks only at the line breaks that you typed.
- Each line of such a template is its own row, and the bar fills the space that the text leaves.
- A template without bar tokens shows as before, so the spaces that you typed stay the same.

### Fixed

- In a narrow panel, the vitals bar keeps your current and maximum values, the percent and the change per tick in view. Before, it hid them.

## 0.3.6, June 2, 2026

Search your scrollback, keep the settings of each profile, and get steady color from more MUDs.

### New

- Press Cmd+F on macOS or Ctrl+F on Windows and Linux to open a search bar.
- The search bar opens at the top of the terminal.
- Press Enter to find each match in the whole live session, not only the text on the screen.
- A count such as 3 / 12 shows next to the search box.
- Shift+Enter or the Up arrow moves to the previous match, and Esc closes the bar.
- The case, word and regex switches work as their names say.
- When a match is above the live view, the scrollback split opens and highlights it in the history pane, and the live output continues below.
- Servers that check environment variables now get an answer too, which says that Vosh supports 256 colors and true color.

### Changed

- The moons always show on the right side of the status bar, and the moons position list is gone from Settings, then Panels, then Chips.
- A profile that put the moons beside the tick and MUD time chip now shows them at the right edge.
- Servers that use MTTS still get the full answer, with the full list of what Vosh supports.
- Each loadout saves its own profile file.
- When you change loadouts, the shared catalog goes on top again, so your aliases and triggers stay the same.
- The disconnect message reads [connection reset by server] or [server closed connection], and not a raw system error.

### Fixed

- Profile settings now save when you use the global catalog. Before, Vosh kept them in memory only, and you lost them when you quit or changed profiles.
- The fix covers tracked affects, the panel layout, the vitals shape, custom themes, paste pacing, the chip style and the presets that are on.
- It also covers the switch that tints server output with your theme, and the position of the moon glyph.
- Vosh now names itself a 256 color terminal in its first answer to the server, so Forsaken Lands and similar servers keep 256 colors on.
- `quit` no longer ends in a raw error on MUDs that close the connection hard, such as Forsaken Lands, so the goodbye line shows.

## 0.3.5, June 2, 2026

Read Help inside Vosh, see the tick and the time in one chip, and find panel settings on three tabs.

### New

- Click [help] in the top bar to open a help window that explains each feature in plain words.
- The help window has a search bar that filters the topics and highlights each match as you type.
- Press Esc or click outside the help window to close it.
- The same help text is in the Vosh repository as a file that you can read offline.
- Click outside a button or a selection in the main window to put the cursor in the command input.
- When you bring Vosh to the front from another app, the cursor goes to the command input too.
- Choose value only, caption plus value, or icon plus value for the tick chip in Settings, then Panels, then Chips.
- The Chips tab also holds the tick interval, the command that the tick sends, the regex reset, sound and warnings.

### Changed

- The tick countdown and the MUD time clock share one chip on the top border of the input row.
- The Panels page of Settings now has three tabs, Layout, Panes and Chips.
- Layout places the zones, Panes holds what each panel shows, and Chips holds the tick, MUD time and moons chips.
- The chat pane follows new messages while you are at the bottom, and it stops when you scroll up.
- The chat pane follows again when you scroll to within 24 pixels of the bottom.
- When you open the chat pane, it shows the latest message and not the oldest.
- In a left or right panel, the room strip wraps to several lines and stays inside the edge.
- You can resize a left or right panel.
- The top bar no longer has a chat button, so you show the chat pane with its zone in Settings, then Panels.
- The group pane no longer has an unpin button, and it shows when its zone is not hidden.

### Fixed

- You can now delete a profile, alias or trigger with [delete] and then [confirm delete] on the same row. Before, a confirm dialog that never showed stopped the delete.

## 0.3.1, May 31, 2026

Let the profile of your character load when you enter the game, and keep your settings safe from a bad save.

### New

- List several character names with commas in Settings, then Profiles, then Auto match, and the profile loads for each.
- You can use one profile for all your warriors, or for all the alts on one host.
- Each save of your settings files now completes in full or not at all.
- Vosh keeps a maximum of ten dated backups, so a failed save or old settings cannot remove your custom themes or tracked affects.
- This protection starts with this version, after a player lost custom themes and tracked affects in 0.3.0.

### Changed

- When you enter the game, the MUD sends your character name over GMCP, and Vosh changes to the profile that matches it.
- You no longer type your character name in the Connect form, so a name that you forget cannot load the wrong profile.

## 0.3.0, May 31, 2026

Write your vitals as a template, put your automation in folders, and use a much faster client.

### New

- Your vitals row is a template that you write, with tokens such as `%hp`, `%mana` and `%sp`, or any GMCP field such as `%{Char.Worth.gold}`.
- Vitals can show a percent gradient, inline or stacked.
- The center of the status bar always shows a tick countdown and the MUD time from the MUD clock.
- The time in the status bar changes color with the time of day.
- A trigger can hold several pattern rows, as in Mudlet, and each row has its own switch.
- Turn a mob name or an event on or off without an edit to one long regex.
- Triggers that you made with one pattern still load.
- Aliases have `%N-` for N from 0 to 9, which means word N and the rest of the input with its spaces.
- `%1-` is the same as `%0`, and `%N-` gives nothing when the input has fewer than N words.
- Put your aliases, triggers and macros in folders with an optional group tag.
- One checkbox in Settings turns a whole group on or off, and each item keeps its own switch.
- In a group that is off, aliases send what you typed, triggers do not fire, and macros go to an alias or the MUD.
- Each tracked affect chip has up and down arrows to change the order.
- Give any tracked affect its own label, so Field of Discord can show as Shroud.
- Each Settings form shows a saved mark that disappears after 1.5 seconds when your changes save.
- A form that you save by hand, such as triggers, aliases and the JSON tabs, shows a dot for unsaved changes.
- When you close a window with unsaved edits, Vosh asks you first.
- Place each panel by zone and alignment in Settings, then Panels, with a live preview at the top.
- The moon phase has three more places, at the right edge, before the time and after the time.

### Changed

- Each bar can use the new track style, which replaces the old ramped glyph bars.
- The combat target chip is to the right of mana and shows the name and condition of your target, and it wraps when necessary.
- The map label moves into the header row that the map already has, so the map has one more row.
- The accent of the Kanso Zen theme is a cool blue that matches the Aabahran site, and not pink.
- Log writes are about 18 times faster in tests, and about 3 times faster with a full log database.
- Map updates are about 21 times faster.
- Each panel listens only for the GMCP data that it uses, so a panel in the background does not draw again for each message.
- The tick countdown no longer draws the vitals template again four times a second.
- A save in Settings sends nothing to your other windows when nothing changed, and one update for each field that you change.
- Vosh shows output lines with no ANSI codes about 7 times faster.

### Fixed

- The vitals bar no longer goes into the right sidebar when the window is wide.
- The Duplicate button on a profile row works again and opens a text box on the row, like Rename. Before, a click did nothing.
- After you drag the scrollback split divider, the live pane returns to the newest output. Before, it stayed above the newest output.
- On Windows, the scrollbars in the terminal, the panels and the Settings forms are thin and in your theme. Before, they were wide and white.
- On Windows, window corners are square and show no white at the edges, and macOS keeps its round corners.
- The first `who` and the message of the day no longer wrap at 80 columns, because Vosh keeps your window size between connections.

## 0.2.11, May 30, 2026

Open Settings with no wait.

### Fixed

- The General page of Settings opens at once, because Vosh reads your fonts only when you need the font list.
- Vosh keeps the font list until you quit.

## 0.2.10, May 30, 2026

See the map as a grid of text, and see each block of server output at once.

### Changed

- In glyph mode, the map draws as text in the app font, with each cell a true terminal cell, like the map in TinTin++.
- The map keeps the same sector glyphs, colors, dim cells and @ marker for you.

### Fixed

- Each block of output from the MUD now shows at once, so a reply of 50 lines appears in one step. Before, output appeared one line at a time, most of all on Windows.

## 0.2.9, May 30, 2026

Paste several lines safely, and choose what your vitals panel shows.

### New

- Paste pacing puts a delay between the lines that you paste, so the MUD does not disconnect you because the lines come too fast.
- Set the delay in Settings, then General, then Paste pacing, from 0 to 10000 ms, with 500 as the default.
- A chip such as [paste 7/50 esc cancels] shows the progress next to the prompt, and Esc stops the rest of the lines.
- Settings, then Panels, then Vitals has separate switches for the bar, the percent, the numbers and the change per tick, with a live preview.
- Pick the filled and empty glyphs from a row of quick picks, such as parallelogram, block, circle or square.
- You can also type your own Unicode characters for the glyphs.
- Set the bar width from 4 to 60 cells.
- Four chips change the whole layout to bars, compact, numeric or percent with one click.

### Changed

- When you paste several lines into the input, Vosh sends each line as a command of its own. Before, it joined them into one.
- One line still goes in at the cursor, and a paste at a password prompt works as before.

## 0.2.7, May 30, 2026

Change the order of your panels, put the tick in another bar, and leave quick keys empty with no error.

### New

- Use the arrows in the Panels preview of Settings to change the order of panels in a zone.
- Vosh keeps the order of your panels.
- The tick countdown can go at the right edge of the vitals bar, room strip, affects bar or status bar, with four new zone choices.
- The preview shows such a tick as + tick on the panel that holds it.

### Fixed

- A quick key that you did not set now goes to your aliases or the MUD with no message. Before, it showed the error no verb is set.

## 0.2.6, May 30, 2026

Return to the live output with one click, and close Settings with the main window.

### New

- Click the scroll wheel on the terminal to close the scrollback split and go to the live output.

### Changed

- When you close the main window, the Settings window closes too.

## 0.2.5, May 30, 2026

Change the size of the scrollback split, and use a smaller chat panel that stays in place.

### New

- Drag the divider of the scrollback split down to make the history pane larger, or up to make it smaller.
- Vosh keeps the size of the history pane between sessions.

### Changed

- The chat panel is 160 pixels high by default, so it does not fill the window at the bottom.
- Chat opens on the most recent message, not the oldest.

### Fixed

- A scroll over chat stays in the chat panel and does not move the terminal.

## 0.2.4, May 29, 2026

Complete words with Tab, keep words whole, and see a status bar that wraps.

### New

- Press Tab to complete the word that you type.
- The first Tab gives the most recent match from your history, each next Tab gives the next match, and Shift+Tab gives the previous match.
- Tab also completes the names of the characters in your room and of your combat targets, after the matches from your history.

### Changed

- Vosh now wraps long lines such as tells and channels itself, so no word breaks in the middle.
- The server still wraps most lines through NAWS, and Vosh wraps the rest.
- The chat panel is 240 pixels high by default and at most half the window, so its messages scroll correctly.

### Fixed

- The status bar wraps to a second row when the target name and quick keys do not fit. Before, it cut them off.
- Chat goes to the newest message again when new ones arrive.
- The terminal fits itself again when the panel layout changes, so a hidden chat leaves no empty strip at the bottom.

## 0.2.3, May 29, 2026

Scroll into history with the mouse, move the tick as its own panel, and check for updates.

### New

- The mouse wheel and trackpad open the scrollback split and move through the history pane, as PageUp or Fn+Up does.
- The live pane stays at the bottom, wherever your pointer is.
- Settings, then General has a [check now] button for updates, which shows the status or the error next to it.
- The tick countdown is a panel of its own that you can move, so it stays when you hide your vitals.

### Changed

- Esc also returns the live pane to the bottom, and it closes the split.
- The scrollback divider is thicker and easier to see.

## 0.2.2, May 29, 2026

Get game text that wraps to the size of your window.

### New

- Vosh tells the MUD the columns and rows of your terminal through telnet NAWS, so the MUD wraps its output between words with no delay.
- When you resize the window, Vosh sends the new size at once, so the output wraps again as you drag.

## 0.2.1, May 29, 2026

Copy and select text with the keyboard, and type commands while you are disconnected.

### New

- On Windows and Linux, Ctrl+C and Ctrl+X copy the terminal text that you select.
- On macOS, Cmd+C and Cmd+X do the same.
- Shift+Home and Shift+End extend the selection to the start or the end of the input line.

### Changed

- The input bar stays open for edits while you are disconnected, so you can write commands before you connect again.

### Fixed

- Home and End on a long input now move the view to the cursor.

## 0.2.0, May 28, 2026

Put your panels anywhere with a layout map in Settings.

### New

- A new Panels tab in Settings shows a map of your layout.
- Move each panel to the top, bottom, left or right, or hide it.
- In a left or right zone, align a panel to the top or the bottom.
- You can move six panels, the map, group, vitals, room strip, chat and affects.
- An option lets the left and right zones use the full height of the window, with the input and status bar under the terminal only.
- Resize handles are on the side of a panel that is near the terminal.

### Changed

- The map fills its column, and other panels stack above or below it at their natural size with no overlap.
- Chat is a panel of its own, apart from group, with a close button in its header.

### Fixed

- A change of tab in Settings now clears error banners. Before, they stayed on the screen.

## 0.1.0, May 28, 2026

Read older output in the scrollback split, get full color from more MUDs, and import from CMUD.

### New

- Press PageUp to open the scrollback split, with older output above and the live combat below.
- Press PageDown or Esc to close the scrollback split.
- The history pane shows how far back you scrolled.
- Choose your own color for the divider of the scrollback split.
- Import CMUD and zMUD XML files with their wildcards translated.
- Bind one printable key, such as backslash, as a macro.

### Changed

- Vosh tells MUDs through MTTS that it supports 256 colors and true color, so MUDs that held back full color now send it.

## 0.0.9, May 28, 2026

See the version of the app, copy while the input has focus, and see macro changes at once.

### New

- Put the pointer on the [vosh] label in the top bar to see the version.
- The Settings footer shows the version too.
- Each release now has signed updates for the auto updater.

### Changed

- On macOS, Cmd+C copies the terminal text that you select, also while the input box has focus.
- Macro changes in Settings reach the main window without a relaunch.

### Fixed

- Macros that use Shift with one other key, such as Shift+F1, now fire from the input.

## 0.0.8, May 28, 2026

Keep your theme changes, and see a smaller label in the top bar.

### Changed

- Theme changes save automatically, so the live preview becomes your saved theme.
- The [vosh] label in the top bar is smaller and sits inside the bar.

### Fixed

- You can now delete a custom theme.
- A theme that you create from the active theme now starts from the theme that you clicked.
- Custom themes stay the same in the Settings window and the main window.

## 0.0.7, May 28, 2026

Make your own themes, and use keys to scroll, move the cursor and copy.

### New

- The custom theme editor in Settings lets you copy any theme and change each color in it.
- Fn+Up and Fn+Down scroll the terminal history by one page.
- Fn+Left and Fn+Right, or Home and End, move the input cursor to the start or the end of the line.
- On macOS, Cmd+C copies the terminal text that you select.

## 0.0.6, May 28, 2026

See the game output in the standard colors.

### Changed

- Server output uses the standard 256 color palette by default, and a tint with your theme is a choice that you turn on.

## 0.0.5, May 28, 2026

Use full screen on macOS, and open builds that Apple notarized.

### New

- On macOS, the maximize button opens native full screen.
- On macOS, the builds have a signature and Apple notarization.

## 0.0.4, May 28, 2026

Give each character its own profile, and choose which settings each profile keeps.

### New

- A catalog of named profiles gives each character or MUD its own aliases, triggers, macros and variables.
- When you connect, Vosh changes to the profile that matches.
- Each category of settings has a scope switch, so you choose if it is global or for one profile.
- With Keep last command on, press Enter to send your last line again.

## 0.0.3, May 27, 2026

Change themes while you play, zoom the map, and get updates automatically if you want them.

### New

- Vosh has a catalog of themes, and you can change themes while it runs.
- The map has zoom controls.
- An auto updater installs each new version after you turn it on.

## 0.0.2, May 6, 2026

Connect to a MUD and play with aliases, triggers, a map, logs and Lua in the first version of Vosh.

### New

- Vosh reads telnet and draws ANSI color, with tests against recorded game output.
- Connect to a MUD over plain TCP or encrypted TLS.
- The game shows in a full terminal view, with a command input that keeps your history.
- Make aliases, regex triggers and macros on keys.
- Variables stay with your profile or last only for the session.
- Lua scripts run inside Vosh, with a plugin manager.
- An automapper draws the rooms as you walk and keeps you in the center of the view.
- Session logs save to a local database, and a regex search finds lines in your scrollback.
- Import from MUSHclient, Mudlet, GMUD and TinTin++.
- A status bar in the TinTin++ style shows your hp, sp and mv vitals, a tick countdown and your tracked affects.
- A chat pane collects the channel messages that the game sends over GMCP.
- A Settings drawer holds triggers, aliases, profiles, plugins, themes and fonts.
- The window has no frame, in the style of tmux, with Kanso Zen as the default theme.
- Vosh runs on macOS, Linux and Windows.
