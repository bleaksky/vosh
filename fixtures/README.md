# Fixtures

Captured byte streams used by parser tests.

## Layout

```
fixtures/
  telnet/    Raw telnet negotiation captures (IAC sequences).
  ansi/      ANSI escape sequence captures, including 256 color and truecolor.
  collapse/  splits.b64, the session's payloads with Collapse repeated lines
             on, for pulses, a fight and lines with no prompt from the fake
             Aabahran, your prompt pinned and for the pulses in the text
             too, as one read and as two cut at every place, after the
             login, with the native grid's screen of each. It also holds
             runs whose line ends on a background across pinned pulses,
             your echo landing before the session heard of it, and a pane
             that loads your scrollback during a run. Generated and
             synthetic, from the server's own lines with an invented name,
             stored the way prompt/aabahran/pinned/ is, held to the session
             by its test, and written again with
             VOSH_WRITE_COLLAPSE_SPLITS=1.
  config/    The exact bytes Vosh writes for each config file, through its
             own save functions. A default and a full profile file,
             global.toml, loadouts.toml, catalog.toml and profiles.toml,
             and first-save/ with the three files a fresh install writes
             on its first save. Generated and synthetic. A golden changes
             only in a commit tied to a numbered bug or a lettered
             decision, and VOSH_WRITE_CONFIG=1 writes them again. Each
             one still reads in 0.8.0, which knows Line and Prompt
             triggers only, so Room triggers go under room_triggers (D14).
    old/     Files in the shapes older builds wrote, written by hand. They
             never change, and each one still loads.
  gmcp/      GMCP message captures.
    aabahran/  Hand written Aabahran packets, one payload per file, for the
               new server build and the two older builds. Its README lists
               what each one stands for. The Map.Tiles packets in map/
               come from a port of the server's own map code run over
               the game's area files instead.
  ipc/       names.txt, every name the page and the app share, each command
             with the keys its function reads and each event with who
             sends it and whether the page hears it. The IPC contract
             test in src-tauri holds it to the sources, and
             VOSH_WRITE_IPC_NAMES=1 writes it again. gmcp-events.json,
             the event each GMCP package goes out on, read by a fake MUD
             test in src-tauri and by src/ipc/session.test.ts on the
             page.
             aliases_export.json is the reply to aliases_export plus a
             final newline. A test in src-tauri/src/ipc/automation.rs
             holds it to that command byte for byte, and the palette and
             Automation tests read it as the reply.
  links/     Golden lists of the ids that links name, taken from the code.
             help-topics.json holds every help topic id with its number,
             in rail order, for src/lib/helpTopicIds.test.ts.
             settings-anchors.json holds every Settings link that search,
             the palette, the pane menu and other pages open, where each
             lands, the anchors each page draws and the help topics the
             pages open, for src/components/settings/settingsAnchors.test.tsx.
             Change either only in a commit tied to a numbered bug or a
             lettered decision.
  macros/    kept-keys.json, macros stores with the six macros of the
             Numpad movement preset and the keys your macros keep from
             it, shared by hold_taken_keys in src-tauri and
             keysYourMacrosKeep on the page. The Macros page tests mount
             its first case. Hand written.
  mccp/      MCCP compressed stream captures.
  pane-layout/  Pane tree cases shared by the Rust and TypeScript sanitize tests.
  prompt-bands/ cases.json, the band under a lifted prompt for a few lifts
               and cell sizes, shared by layoutBands on xterm and
               band_rects on the native grid, so both renderers draw the
               same bands. Hand written.
  prompt/
    aabahran/  Aabahran prompt lines as the game sends them, raw and plain,
               and PROMPT settings for the compiler in crates/prompt.
      wire/    Synthetic socket reads the fake Aabahran in the test kit
               plays, one .bin of raw telnet bytes per case with a
               .notes.md that says what it holds and marks it synthetic.
      pinned/  splits.b64, the session's payloads with your prompt pinned
               for every wire case and a few pulses back to back, as one
               read and as two cut at every place, with the native grid's
               screen of each. Generated, synthetic, and stored as base64
               of a gzip so the webview test can import it as text. The
               session test holds it to what the session sends, and
               VOSH_WRITE_PINNED_SPLITS=1 writes it again.
      preview/ splits.b64, the session's payloads with the prompt card's
               Low health preview on, in the text and lifted, for the same
               streams, as one read and as two cut at every place, with
               the native grid's screen of the live session after your
               echo. Generated and stored the way pinned/ is, held to the
               session by its test, and written again with
               VOSH_WRITE_PREVIEW_SPLITS=1.
      pointer/ cases.json, two pulses as the session plays them (a quiet
               prompt the fight leaves in history, then the fight's prompt
               with its tank line) for three designs, in the text, lifted
               and pinned: the payloads, the open row the prompt card
               reads, the band's zone, and the native grid's screen and
               cursor report at 80, 30 and 12 wide. Generated and
               synthetic. The session test holds it to what the session
               sends, VOSH_WRITE_POINTER_CASES=1 writes it again, and the
               webview test maps a pointer with it on xterm, the native
               grid and the dock.
  readable/  grounds.json, the terminal background of every built in theme,
             the fixed text colors the trigger presets paint with the
             #8fa7d9 weather blue, and every Replace template of the
             presets. The Keep highlight colors readable tests in
             crates/automation lift every color on every ground and run
             every template through the trigger engine on every ground, and
             readableGrounds.test.ts on the page holds the lists to the
             themes and presets. Written from the page sources.
  room-colors/ looks.json, room looks as the Aabahran server prints them,
               each with its Room.Chars packet, for the Room trigger tests
               in src-tauri. lines.json, the lines the Room, time and
               weather colors preset colors and the near misses it leaves
               alone, read by presets.test.ts and src-tauri. The trigger
               crate highlights each word of both in place and draws each
               weather line on every ground. preset.json, that preset's
               triggers, which presets.test.ts holds to presets.ts and the
               Rust tests install. Hand written and synthetic, from the
               server's own format strings and area files. Its README says
               where each line comes from.
  session-labels/ cases.json, what a session goes by, its name, its
               character or the world where it dials with or without its
               port, shared by sessionLabel on the page and label_of in
               src-tauri/src/sessions.rs, so a banner names a session as
               its row does. Hand written.
  terminal-rows/ cases.json, the rows the terminal keeps and the rows the
               game is told while your pinned prompt band borrows rows,
               shared by keptRows and gameSize on xterm and
               grid_and_game_rows on the native grid. Hand written.
  themes/    One theme file per format the Appearance import reads (Ghostty,
             iTerm2, Kitty, Alacritty TOML, legacy Alacritty YAML).
  ui-config/ defaults.json, the UI config Rust sends for a profile that
             sets nothing, which normalizeUiConfig on the page fills in
             for a field that arrives missing. Hand written.
             fields.json, one value that is not the default for each field
             ui_set_fields takes, read by the setter tests in
             src-tauri/src/ipc/ui_config.rs and src/ipc/uiConfig.test.ts.
             Hand written.
             vitals-text.txt, Vosh's vitals text, which a profile that sets
             no vitals_text draws, held to DEFAULT_VITALS_TEXT in
             crates/prompt and to the copy the Settings gallery draws by
             their tests. Hand written.
  wrap/      Word wrap cases shared by the Rust wrap in crates/prompt and the
             TypeScript WordWrapper, so both renderers break lines alike.
```

## Capturing From Aabahran

Run a session through `socat` or `nc` with hex logging to record raw bytes. Strip credentials before committing.

Sample.

```
socat -x -v TCP:theforsakenlands.com:9009 - 2> capture.hex
```

Trim the hex log to the interesting region, then drop it under the matching subdirectory with a short descriptive name. Add a sibling `.notes.md` if the capture needs context (server version, what command produced it, expected parser output).

## Rules

- No credentials, no character names, no chat content, no PII.
- Each fixture must have a parser test that consumes it.
- Prefer many small fixtures over a few big ones.
- A fixture written by hand rather than captured says so. A capture file gets a `.notes.md` that marks it synthetic, and a JSON fixture says it in its `notes` field. It stays marked until an approved socat capture takes its place.
