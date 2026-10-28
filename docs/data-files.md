# Data files

Every file Vosh writes, its format, which upgrades touch it and what has
to stay readable. A change to any of these files changes a player's disk,
so read this before you touch a save path.

The names of the files in the app data folder live in
`src-tauri/src/disk/paths.rs`. The launch upgrades live in
`src-tauri/src/disk/upgrades.rs` and its folder. The golden files in
`fixtures/config` hold the exact bytes Vosh writes for each TOML file,
and `src-tauri/src/tests/config_golden.rs` checks them.

## Where the files live

Vosh keeps its data in one app data folder named `com.aabahran.vosh`.

| Platform | Folder                                            |
| -------- | ------------------------------------------------- |
| macOS    | `~/Library/Application Support/com.aabahran.vosh` |
| Linux    | `~/.local/share/com.aabahran.vosh`                |
| Windows  | `%APPDATA%\com.aabahran.vosh`                     |

Exports go to your Downloads folder instead. The page keeps a few small
choices in the webview's browser storage.

A debug build, the one `npm run tauri dev` runs, keeps a folder of its
own beside that one, named `com.aabahran.vosh.dev`. Its first launch
copies the installed app's folder into it, without `logs.sqlite`,
`maps.sqlite` and the backups, and from then on the two never touch each
other's files. `src-tauri/src/disk/dev_folder.rs` does this. A dev folder
without its `.copied` mark, which a failed copy leaves, moves aside and
is copied again. Delete the dev folder to start again from a fresh copy.

## How a TOML save works

Profile files, profiles.toml, global.toml, catalog.toml, loadouts.toml,
the wizard journal and the copies in `profiles/legacy` go through
`write_with_backup` in `src-tauri/src/disk/atomic.rs`. It writes
`<file>.tmp`, copies the old file to `<file>.bak.<unix ms>`, renames the
temp file over the old one and keeps the ten newest backups. A file
that already holds the new text is left alone with no backup, so the ten
are the last ten changes. A failed write leaves the old file in place.
Restore from a backup… under Characters lists a profile file's backups
through `src-tauri/src/profile/backups.rs` and lays one over the profile
as an import with Replace does, keeping its world, characters and
plugins, and in loadout mode the catalog. In loadout mode a backup that
holds aliases, triggers, macros or preset edits is from before the move
and neither lists nor restores. A profile's backups move with a rename
and go with a delete, as its layout history does.

A file Vosh could not read goes on the unread list, and every save to it
fails until it reads again. So a save never writes the defaults over
settings Vosh could not parse.

A profile file, its `[ui]` and `[ui.panes]` tables, and global.toml keep
the keys this build does not know in an `extra` map and write them back
as they were. So a build older than the one that wrote a key saves the
file without losing it. A profile file reads past `[connection]`, which
builds before 0.9 wrote, and `[vosh_export]`, which an export adds.

`writing.toml` and the files in a plugin folder go through `swap_in`,
which swaps the text in whole and keeps no backup. A backup inside a
plugin folder would ride along when you export the plugin.
`affect_full.toml` and the scrollback files write a temp file and rename
it the same way.

## The app data folder

| File                                        | Format              | Written by                                                     |
| ------------------------------------------- | ------------------- | -------------------------------------------------------------- |
| `profiles.toml`                             | TOML                | `src-tauri/src/profile/set.rs`                                 |
| `profiles/<name>.toml`                      | TOML                | `src-tauri/src/profile/file.rs`                                |
| `profiles/<name>.toml.before-prompt-editor` | TOML, a plain copy  | `keep_before_prompt_editor` in `src-tauri/src/profile/file.rs` |
| `profiles/legacy/<name>.toml`               | TOML, a plain copy  | `src-tauri/src/loadouts/wizard/apply.rs`                       |
| `global.toml`                               | TOML                | `src-tauri/src/profile/shared.rs`                              |
| `catalog.toml`                              | TOML                | `src-tauri/src/loadouts/catalog.rs`                            |
| `loadouts.toml`                             | TOML                | `src-tauri/src/loadouts/set.rs`                                |
| `catalog.journal.toml`                      | TOML                | `src-tauri/src/loadouts/wizard/journal.rs`                     |
| `affect_full.toml`                          | TOML, `version = 1` | `src-tauri/src/affects/full.rs`                                |
| `writing.toml`                              | TOML, `version = 1` | `src-tauri/src/writing.rs`                                     |
| `layouts/<name>.toml`, `layouts/.seen.toml` | TOML                | `src-tauri/src/profile/layout_history.rs`                      |
| `logs.sqlite`                               | SQLite in WAL mode  | `crates/log/src/sqlite.rs`                                     |
| `scrollback.txt`, `scrollback-<n>.txt`      | Raw bytes, CRLF     | `src-tauri/src/logs.rs`                                        |
| `history.txt`, `history-<n>.txt`            | Text, one per line  | `src-tauri/src/input/history.rs`                               |
| `scripts/`                                  | A folder            | `create_scripts_dir` in `src-tauri/src/app/launch.rs`          |
| `plugins/<name>/manifest.toml` and its Lua  | TOML and Lua        | `src-tauri/src/app/plugins.rs` and its folder                  |
| `maps.sqlite`                               | SQLite              | Nothing now                                                    |

### profiles.toml

The index of your profiles. It names the active one and lists each
profile with its description and the host, port and characters it
matches. It also holds the sharing scope (`[scope]`), which says which
categories global.toml shares, the sessions Vosh restores at launch, the
Get started state, Keep logs for, the launch notices still to show and
`migrations`, the ids of the one time upgrades that already ran.

Upgrades. Every recorded upgrade writes its id here. The launch also
moves the root `profile.toml` of the oldest builds to
`profiles/default.toml` as it reads the set.

Goldens. `profiles.default.toml`, `profiles.full.toml`,
`profiles.sessions.toml` and `first-save/profiles.toml`. Old inputs
`old/profiles-0.8.1.toml` and `old/profiles-character.toml`.

### profiles/<name>.toml

One file per profile. It holds the aliases, triggers, room triggers,
timers, macros, variables, tick settings, group folders, plugins that
are on, alerts, preset edits, `[prompt]` and `[ui]`. In loadout mode the
aliases, triggers and macros live in catalog.toml instead, and the
profile file keeps the rest. Room triggers sit in `[[room_triggers]]`,
apart from `[[triggers]]`, since 0.8.0 fails a whole file on a trigger
target it does not know.

The top level `charset` key says how the profile reads the game's text
and writes yours. It writes `charset = "latin1"` for Latin 1 and
nothing for UTF-8, the default. A name it does not know, such as
`"cp437"`, reads as UTF-8 and the profile still loads. Builds before it
keep the key among the keys they do not know and read UTF-8.

A pane in `[ui.panes]` writes `folded = true` while it is folded down
to its header, and nothing while it is open. Builds before 0.95.6 drop
the key and open the pane.

The first save that adds `[prompt]` to a file without one first copies
the file to `<name>.toml.before-prompt-editor`. The prompt capture
upgrade reads that copy to tell when an older build dropped `[prompt]`
again.

Upgrades.

- `prompt-capture-to-profile` moves a trigger that hands its groups to
  `mud.set_prompt_var` into `[prompt.capture]` and turns the trigger
  off. It runs again for a file that lost the `[prompt]` Vosh wrote.
- `preset-sent-tells-on` and `preset-room-and-time-on` add their preset
  once to an `enabled_presets` list that names presets.
- The custom theme move gathers `[[ui.custom_themes]]` into global.toml
  while the theme scope is global. It records no id, since it runs until
  no file holds a list.
- The shared catalog wizard moves the aliases, triggers and macros into
  catalog.toml, after it copies each file into `profiles/legacy`.

Goldens. `profile.default.toml`, `profile.fresh.toml`,
`profile.full.toml`, `profile.full-regex.toml` and
`first-save/default-profile.toml`. Old inputs
`old/profile-bare-tracked-affects.toml`, `old/profile-connection.toml`,
`old/profile-dock-no-panes.toml`, `old/profile-grouped-preset.toml`,
`old/profile-no-prompt.toml`, `old/profile-numpad-0.8.1.toml` and
`old/profile-one-with-erelei.toml`.

### global.toml

The settings every profile shares while the scope says global, such as
the theme, the font, the day and night themes, `dock_layout` and the
custom themes. At its defaults the file is empty.

Upgrades. The custom theme move writes `[[custom_themes]]` here.

Goldens. `global.default.toml`, `global.full.toml` and
`first-save/global.toml`.

### catalog.toml and loadouts.toml

catalog.toml holds the shared catalog of aliases, triggers, room
triggers and macros, with its presets, alerts and preset edits. While it
exists Vosh runs in loadout mode. loadouts.toml holds the loadouts with
their groups and auto match, the one stack of active loadouts with its
`dormant` switch, and `[profiles]`, the stacks of profiles that keep
their own. `[profiles]` is left out while empty, and an older build
reads `active` and `dormant` as the one stack and passes over it.
Loadouts no longer write empty `tick`, `connection` and `profile_vars`
tables. 0.7.2 reads defaults for them.

Upgrades. The wizard writes both. `prompt-capture-to-profile` turns a
catalog capture trigger off, and the two preset upgrades add their
preset to the catalog's list.

Goldens. `catalog.default.toml`, `catalog.full.toml`,
`loadouts.default.toml` and `loadouts.full.toml`. Old inputs
`old/catalog-no-presets.toml` and `old/catalog-numpad-0.8.1.toml`.

### catalog.journal.toml

The shared catalog wizard writes every file it will change, with the new
text, before its first write, and removes the journal after its last. A
journal left behind by a crash or a failed write makes the next launch
write every file it names before anything loads. Each profile file's
path in it is relative to the app data folder.

### affect_full.toml

The most hours Vosh has seen for each affect since it was cast, so the
Affects pane can drain a gauge. `[characters]` keys each character by
`{host}:{port} {name}`. A store writes only when it changes what the
file holds, a moment after a burst settles.

### layouts/

`layouts/<name>.toml` holds up to 20 pane layouts the profile `name` had
before, newest first, each in an `[[earlier]]` entry with `until`, the
Unix milliseconds it gave way, `why` (`change`, `arrange`, `reset`,
`restore` or `update`), `version` for an update, and the `[ui.panes]`
table it had as `panes`. A rename takes the file along and a delete
drops it. `layouts/.seen.toml` names in `version` the last version whose
first launch kept every profile's layout, so each version does that
once, before any profile opens. Both go through `swap_in` with no
backup, and a file that does not read counts as empty.

### writing.toml

Your drafts and the last 20 posts under Sent for each character on each
world, with the card's switches at the top. A file of its own, so a
keystroke never rewrites a profile. A file Vosh cannot read stays as it
is, and the card works from memory until you quit.

### logs.sqlite

The game log, with `-wal` and `-shm` beside it. The `sessions` table
holds one row per connection and `log_lines` one row per line. Opening
the file adds the columns later builds need, `sessions.character`,
`log_lines.kind` and `log_lines.channel`, each on its own and without
rewriting a row. `#logs forget-passwords` adds the
`forget_passwords_wipe_pending` table while a blanking run waits for the
file to be rebuilt, and drops it once the rebuild ends. Keep logs for deletes old sessions at
launch, once a day and on a change.

### scrollback.txt and scrollback-<n>.txt

The newest terminal lines of each session, as many as Scrollback size
says, one line per CRLF with its SGR codes. The echo of each command you
sent sits among them where it showed, its mark included, and a password
leaves none. The first session keeps
scrollback.txt, which an older build reads too. Each other session
keeps a file with its number. Vosh writes each one every 3 minutes while
it changes, when a connection ends and as you quit. Closing a session
deletes its file.

### history.txt and history-<n>.txt

The commands each session sent from the command line, one per line,
oldest first, which Up brings back after a restart. Each one lands at
the end of the file as it goes out, so a crash loses none. The page asks
for a session's newest 1,000 the first time it shows the session, and
Vosh cuts a longer file back to them with `swap_in`. A command that gives
the game a password never lands, read as the input pipeline reads it,
so `look;password a b` and an alias that sends `password` stay out. On a
game that speaks GMCP, nothing you send before it says you play lands
either, which keeps out a password typed ahead of the login's prompt.
The page keeps what you type in password mode and lines for the game's
editor out before they get here. One lock covers every read, write and
delete of the files. The files take the scrollback's numbers, and closing
a session deletes its file.

### scripts/ and plugins/

Launch creates `scripts/`, where `#script load` reads Lua files. Vosh
never writes into it.

Each plugin is a folder in `plugins/` with a `manifest.toml` (a
`[plugin]` table with `name`, `version`, `description`, `author` and
`entry`) and its Lua files. Launch seeds the `vitals_alert` example
when its folder is missing. New plugin, the editor's saves and an
install from a .zip write here, and Remove deletes the folder.

### maps.sqlite

Older builds recorded every room here. Vosh retired that map store and
no longer reads or writes the file. It stays on disk untouched.

## Outside the app data folder

| File                                          | Format                    | Written by                                 |
| --------------------------------------------- | ------------------------- | ------------------------------------------ |
| `.window-state.json` in the app config folder | JSON                      | `tauri-plugin-window-state`                |
| `<name> profile.toml` in Downloads            | TOML with `[vosh_export]` | `src-tauri/src/ipc/characters.rs`          |
| `<name>.txt`, `.log` or `.html` in Downloads  | Text, SGR or an HTML page | `logs_save` in `src-tauri/src/ipc/logs.rs` |
| `<room>, <day>.txt`, `.log` or `.html`        | Text, SGR or an HTML page | `src-tauri/src/logs/scene.rs`              |
| `<plugin>.zip` in Downloads                   | Zip of the plugin folder  | `src-tauri/src/app/plugins/archive.rs`     |

The window state file keeps the size, position, maximized and full
screen state of each window. On macOS and Windows the app config folder
is the app data folder.

An export never replaces a file. `export_path` in
`src-tauri/src/disk/paths.rs` adds ` (2)`, ` (3)` and on, and an import
reads the name back without the count. A profile export ends with
`[vosh_export]`, which names the world and the characters you ticked.
The golden is `export.full.toml`.

## Browser storage

The webview keeps these keys in `localStorage`, apart from the one in
`sessionStorage`. Each one is a convenience. A missing or unreadable key
falls back to its default.

| Key                              | What it keeps                                                            |
| -------------------------------- | ------------------------------------------------------------------------ |
| `vosh.cache.fontFamily`          | The terminal font, for the first paint before Settings load              |
| `vosh.cache.fontSize`            | The terminal font size, for the first paint                              |
| `vosh.cache.lineHeight`          | The terminal line height, for the first paint                            |
| `vosh.cache.panelFont`           | The panel font, for the first paint                                      |
| `vosh.cache.panelSize`           | The panel font size, for the first paint                                 |
| `vosh.cache.themePaint`          | The colors the last theme painted, which every window reads              |
| `vosh.nativesurface`             | `0` turns the native renderer off on macOS                               |
| `vosh.nativesurface.failed`      | In `sessionStorage`. The surface failed, so xterm draws this run         |
| `vosh.webgl`                     | `0` turns off xterm's WebGL renderer                                     |
| `vosh.connection.target`         | The saved world, where a New session form starts and a new session dials |
| `vosh.layout.sessionsWidth`      | The width of the sessions sidebar                                        |
| `vosh.layout.splitHistoryHeight` | The height of the history half of the split                              |
| `vosh.layout.serverMapTileset`   | The map pane's tile set                                                  |
| `vosh.layout.serverMapZoom`      | The map pane's zoom                                                      |
| `vosh.layout.serverMapStyle`     | The old map style key, read once when `vosh.map.style` is missing        |
| `vosh.map.style`                 | The map pane's style                                                     |
| `vosh.map.view3d`                | The turn and tilt of the stacked map view                                |
| `vosh.automation.folded.<list>`  | The folded groups of an Automation list                                  |
| `vosh.palette.recent`            | The recent rows of the `Cmd+K` palette                                   |
| `vosh.settings.pendingTab`       | The Settings tab another window asked to open                            |
| `vosh.help.pending`              | The Help topic another window asked to open                              |
| `vosh.help.topic`                | The Help topic you read last                                             |

## What must stay readable

- Every file in `fixtures/config/old` loads, and its digest never
  changes. Those are files older builds wrote, back to 0.8.1.
- Every golden reads in 0.8.0, whose triggers know only `line` and
  `prompt` targets, and its macros read in 0.8.1.
- A root `profile.toml` still moves into `profiles/default.toml`.
- A profile with no `[prompt]` reads its switch and design from `[ui]`.
- A profile with `dock_layout` and no `[ui.panes]` gets panes made from
  the dock when it loads. Nothing reaches disk until your first edit.
- A bare string list of `tracked_affects` still loads.
- An older `logs.sqlite` opens and gains its missing columns.
- `scrollback.txt` stays the first session's file, which an older build
  restores.

## The rollback rule

Through 1.0 Vosh keeps a way back to 0.7.2. Every save keeps writing
what 0.7.2 reads.

- The `[ui]` copy of the prompt, `prompt_template_enabled` and
  `prompt_template`, beside `[prompt]`.
- `dock_layout`, in `[ui]` and in global.toml, beside the panes.
- The three old fields nothing reads now, `ui.vitals`, `moons_position`
  and `side_panels_fill_height`. Rust writes back the values it loaded
  without reading them.

They retire together in the first release after 1.0. Until then a
change that stops writing any of them breaks a downgrade, and the
goldens show it.
