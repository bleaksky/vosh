# Fixtures

Captured byte streams used by parser tests.

## Layout

```
fixtures/
  telnet/    Raw telnet negotiation captures (IAC sequences).
  ansi/      ANSI escape sequence captures, including 256 color and truecolor.
  gmcp/      GMCP message captures.
  mccp/      MCCP compressed stream captures.
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
