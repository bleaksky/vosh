# Fixtures

Captured byte streams used by parser tests.

## Layout

```
fixtures/
  telnet/    Raw telnet negotiation captures (IAC sequences).
  ansi/      ANSI escape sequence captures, including 256 color and truecolor.
  gmcp/      GMCP message captures.
    aabahran/  Hand written Aabahran packets, one payload per file, for the
               new server build and the two older builds. Its README lists
               what each one stands for.
  mccp/      MCCP compressed stream captures.
  pane-layout/  Pane tree cases shared by the Rust and TypeScript sanitize tests.
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
  themes/    One theme file per format the Appearance import reads (Ghostty,
             iTerm2, Kitty, Alacritty TOML, legacy Alacritty YAML).
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
