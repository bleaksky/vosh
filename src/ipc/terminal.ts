// The output the session writes to the terminal, the text the page writes
// itself, the native grid's cursor and screen, and the saved scrollback.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { OUTPUT } from './events';
import type { PromptSpan } from './promptDesign';
import { sessionOf } from './session';

export interface OutputPayload {
  /** The session whose terminal takes the output. */
  session?: number;
  /** Output bytes as standard base64. Decoded once with `atob` in
   *  `onOutput`. Replaces the old `number[]` array, which made the
   *  backend serialize one JSON number per byte. */
  b64: string;
  /** Replace a region an earlier payload marked, applied before `b64`
   *  (see src/terminal/terminalRegion.ts). */
  replace?: {
    gen: number;
    b64: string;
    fresh: boolean;
    /** The lines the region's prompt shows right above it, as plain
     *  text, and what goes in their place when they are there. */
    above?: { plain: string; b64: string };
    /** The end of the region the bytes leave out, as base64, which a
     *  terminal that writes them on a new row, or finds the region open
     *  with nothing held back, holds back in their place. */
    tail?: string;
  };
  /** The live render for the region this payload leaves open, as
   *  base64, written back before anything else lands. */
  restore?: string;
  /** What the band above the command line shows from now on, as base64,
   *  while your prompt shows pinned. An empty string clears it. */
  pin?: string;
  /** Where each piece of your design landed on the band `pin` shows,
   *  rows counted from the band's first. Absent when it shows no design. */
  pin_spans?: PromptSpan[];
  /** Line ends at the end of this payload that each renderer keeps back
   *  until the next write lands, as base64. */
  hold?: string;
  /** While your prompt shows pinned, whether the pinned prompt's row is
   *  still where the next thing lands after this payload, so the line
   *  end that would end that row writes nothing. */
  pin_row?: boolean;
  /** The bytes start a row of their own, as a line Vosh prints about
   *  itself does, so a terminal whose cursor sits past the start of a
   *  row writes a line end first. Absent when false. */
  fresh?: boolean;
  /** Which output of the prompt stage this is. Absent on output from
   *  elsewhere, such as a slash command's echo. */
  id?: number;
}

/** One session write, decoded: the replace goes first, then `bytes`,
 *  and `hold` waits for the next write. */
export interface SessionOutput {
  bytes: Uint8Array;
  replace?: {
    gen: number;
    bytes: Uint8Array;
    fresh: boolean;
    above?: { plain: string; bytes: Uint8Array };
    tail?: Uint8Array;
  };
  restore?: Uint8Array;
  pin?: Uint8Array;
  /** Where each piece of your design landed on the band `pin` shows. */
  pinSpans?: PromptSpan[];
  hold?: Uint8Array;
  pinRow?: boolean;
  /** The bytes start a row of their own. */
  fresh?: boolean;
  /** Which output of the prompt stage this is. A terminal keeps the
   *  newest it took, so text it writes itself can tell the session
   *  which output it follows. */
  id?: number;
}

/** Standard base64 to bytes. `atob` yields a binary string, one char per
 *  byte, and each char code goes back to a byte. Far cheaper than parsing
 *  a JSON number[] and copying it. */
function base64Bytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return bytes;
}

/** Decode a `session://output` payload. */
export function decodeOutputPayload(payload: OutputPayload): SessionOutput {
  const out: SessionOutput = { bytes: base64Bytes(payload.b64) };
  const replace = payload.replace;
  if (replace) {
    out.replace = { gen: replace.gen, bytes: base64Bytes(replace.b64), fresh: replace.fresh };
    if (replace.above) {
      out.replace.above = { plain: replace.above.plain, bytes: base64Bytes(replace.above.b64) };
    }
    if (typeof replace.tail === 'string') out.replace.tail = base64Bytes(replace.tail);
  }
  if (typeof payload.restore === 'string') out.restore = base64Bytes(payload.restore);
  if (typeof payload.pin === 'string') out.pin = base64Bytes(payload.pin);
  if (Array.isArray(payload.pin_spans)) out.pinSpans = payload.pin_spans;
  if (typeof payload.hold === 'string') out.hold = base64Bytes(payload.hold);
  if (typeof payload.pin_row === 'boolean') out.pinRow = payload.pin_row;
  if (payload.fresh === true) out.fresh = true;
  if (typeof payload.id === 'number') out.id = payload.id;
  return out;
}

/** Hear each write to a session's terminal that `wants` takes, decoded,
 *  with that session. `wants` reads the session and the payload as it
 *  came, so a write no listener needs, such as one to a terminal of
 *  another session, decodes nothing. */
export async function onOutput(
  wants: (session: number, payload: OutputPayload) => boolean,
  cb: (out: SessionOutput, session: number) => void,
): Promise<UnlistenFn> {
  return listen<OutputPayload>(OUTPUT, (event) => {
    const session = sessionOf(event.payload);
    if (wants(session, event.payload)) cb(decodeOutputPayload(event.payload), session);
  });
}

/** Hear each session whose terminal took a line from the game. A write
 *  counts when the prompt stage made it, so it carries an id, and its
 *  bytes end a line. A prompt alone ends none, and Vosh's own echoes
 *  come from elsewhere with no id. `wants` says whether a session still
 *  needs to hear it, so the writes of one marked already decode nothing. */
export async function onGameLine(
  wants: (session: number) => boolean,
  cb: (session: number) => void,
): Promise<UnlistenFn> {
  return listen<OutputPayload>(OUTPUT, (event) => {
    const { payload } = event;
    if (typeof payload.id !== 'number') return;
    const session = sessionOf(payload);
    if (wants(session) && atob(payload.b64).includes('\n')) cb(session);
  });
}

/** Write text the webview drew itself, such as your typed echo or an
 *  error notice, into the native grid too, and tell the session, which
 *  closes the open row, since the text now follows it. `after` is the
 *  newest output of the prompt stage xterm took before the text while
 *  xterm shows, and null while the native grid shows, which names its
 *  own. The session can hear of your echo after the reply to your line,
 *  and the prompt that came after the echo stays open. `session` names
 *  the session whose grid takes it, the selected one when it names
 *  none. */
export async function terminalLocalWrite(
  text: string,
  after: number | null,
  session?: number,
): Promise<void> {
  await invoke('terminal_local_write', { text, after, session });
}

/** You started or stopped selecting text or reading back in the xterm
 *  of `session`. While you do, a clock piece in your design leaves that
 *  session's prompt in the text as it is. */
export async function terminalReaderBusy(busy: boolean, session: number): Promise<void> {
  await invoke('terminal_reader_busy', { busy, session });
}

/** Where the native grid's cursor sits and where its open region
 *  starts. Lines count from the top of the live screen, negative in
 *  history, so while `at_bottom` holds a line is the screen row the grid
 *  draws it on. `region` is null once anything lands after the region. */
export interface TerminalCursor {
  line: number;
  col: number;
  at_bottom: boolean;
  cols: number;
  region: { gen: number; line: number; col: number } | null;
}

/** The cursor and open region of a session's native grid, the selected
 *  session's when it names none, for mapping a pointer to a piece of your
 *  prompt while the native renderer draws the terminal. Null before the
 *  grid exists and on a build without it. xterm reads its own buffer
 *  instead. */
export async function terminalCursor(session?: number): Promise<TerminalCursor | null> {
  return invoke('terminal_cursor', { session });
}

/** The native grid's live screen as text: each row with trailing blanks
 *  gone, its width, and whether it shows the live tail. */
export interface TerminalScreenRows {
  rows: string[];
  cols: number;
  at_bottom: boolean;
}

/** The live screen of a session's native grid as text, the selected
 *  session's when it names none, for the prompt card's marks while the
 *  profile reads no prompt. Null before the grid exists and on a build
 *  without it. xterm reads its own buffer instead. */
export async function terminalScreenRows(session?: number): Promise<TerminalScreenRows | null> {
  return invoke('terminal_screen_rows', { session });
}

export interface ScrollbackLoad {
  bytes: Uint8Array;
  /** True when this load also seeded the native grid with the bytes. Only
   *  the first seed request per process does, so a reloaded page sees
   *  false while the grid still holds the history. */
  seededNative: boolean;
}

/** The lines a session keeps for its terminal, the selected session's
 *  when it names none. */
export async function loadScrollback(
  feedNative = false,
  session?: number,
): Promise<ScrollbackLoad> {
  const res = await invoke<{ bytes: number[]; seeded_native: boolean }>('scrollback_load', {
    feedNative,
    session,
  });
  return { bytes: new Uint8Array(res.bytes), seededNative: res.seeded_native };
}

/** The terminal background the session lifts trigger colors against
 *  while Keep highlight colors readable is on, and the one it fits game
 *  colors against while Fit game colors is on, each null while its
 *  setting is off. terminal/highlightGround.ts says when each goes. */
export function highlightGroundSet(ground: {
  background: string | null;
  game: string | null;
}): Promise<void> {
  return invoke('highlight_ground_set', { ...ground });
}

/** Forget the lines a session keeps for the next launch, the selected
 *  session's when it names none, and on the native renderer its grid's
 *  history too. xterm clears its own buffer. */
export async function scrollbackClear(session?: number): Promise<void> {
  await invoke('scrollback_clear', { session });
}
