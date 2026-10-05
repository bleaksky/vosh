import { useCallback, useSyncExternalStore } from 'react';
import { dockRows } from '../lib/promptBand';
import { type PromptSpan } from '../ipc/promptDesign';
import { onState } from '../ipc/session';
import { onOutput, type SessionOutput } from '../ipc/terminal';
import { closePinRow } from '../lib/terminalRegion';
import { createStore } from './store';

// The prompt the session pinned above the command line, as the text the
// band draws: the output's pin field, decoded, with where each piece of
// your design landed on it. It listens from launch, so the band shows
// the latest prompt even when it mounts after the output that carried
// it, as it does when you choose Pinned. An empty pin and a disconnect
// clear it.
//
// It also keeps whether the row the pinned prompt held is still where the
// next thing lands, as each renderer keeps it. Enter on an empty line
// echoes nothing only then. A prompt Vosh does not read, such as the
// pager's [Hit Return to continue], stays in the text and closes the row,
// so Enter there ends its row as it always did.

/** What the band shows: its text, and where each piece of your design
 *  landed on it, rows counted from the band's first. No pieces when it
 *  shows no design, such as the game's prompt with drawing off. */
export interface PinnedBand {
  text: string;
  spans: PromptSpan[];
}

const store = createStore<PinnedBand | null>(null);
let started = false;
let rowOpen = false;

/** The band after `out`: what its pin says, or `band` as it was when it
 *  carries none. An empty pin clears it. */
export function bandAfterOutput(
  band: PinnedBand | null,
  out: SessionOutput,
  decoder: TextDecoder,
): PinnedBand | null {
  if (!out.pin) return band;
  if (out.pin.length === 0) return null;
  return { text: decoder.decode(out.pin), spans: out.pinSpans ?? [] };
}

/** Whether the pinned prompt's row is open after `out`: as the payload
 *  says, else closed by anything it writes at the cursor. */
export function pinRowAfterOutput(open: boolean, out: SessionOutput): boolean {
  if (out.pinRow !== undefined) return out.pinRow;
  if (!open) return false;
  if (out.replace?.fresh && out.replace.bytes.length > 0) return false;
  if (out.bytes.length === 0) return true;
  return !closePinRow(new TextDecoder('utf-8', { fatal: false }).decode(out.bytes)).closed;
}

/** Whether the pinned prompt's row is open after the page writes `text`
 *  to the terminal, such as your echo or an error notice. */
export function pinRowAfterWrite(open: boolean, text: string): boolean {
  return open && !closePinRow(text).closed;
}

export function startPinnedPromptStore(): void {
  if (started) return;
  started = true;
  const decoder = new TextDecoder('utf-8', { fatal: false });
  void onOutput((out) => {
    store.set(bandAfterOutput(store.get(), out, decoder));
    rowOpen = pinRowAfterOutput(rowOpen, out);
  });
  void onState((state) => {
    if (state.kind === 'disconnected') {
      store.set(null);
      rowOpen = false;
    }
  });
}

/** The page wrote `text` to the terminal itself. */
export function notePageWrite(text: string): void {
  rowOpen = pinRowAfterWrite(rowOpen, text);
}

/** The row your pinned prompt held is where the next thing lands, so
 *  Enter on an empty line has no row to end. */
export function pinnedRowOpen(): boolean {
  return rowOpen;
}

export function getPinnedPrompt(): string | null {
  return store.get()?.text ?? null;
}

/** The band with the pieces of your design on it, or null. */
export function getPinnedBand(): PinnedBand | null {
  return store.get();
}

export function subscribePinnedPrompt(cb: () => void): () => void {
  startPinnedPromptStore();
  return store.subscribe(cb);
}

/** The latest pinned prompt, or null before one comes and after you
 *  disconnect. */
export function usePinnedPrompt(): string | null {
  return useSyncExternalStore(subscribePinnedPrompt, getPinnedPrompt);
}

// The last count dockRows gave, so a prompt that changes only its values
// is not parsed again on every read of the snapshot.
let rowsMemo: { pin: string | null; zone: number; off: boolean; rows: number } | null = null;

/** The rows the dock shows for the latest pinned prompt (dockRows). */
export function getPinnedDockRows(zone: number, promptsOff: boolean): number {
  const pin = getPinnedPrompt();
  const memo = rowsMemo;
  if (memo && memo.pin === pin && memo.zone === zone && memo.off === promptsOff) return memo.rows;
  const rows = dockRows(pin, zone, promptsOff);
  rowsMemo = { pin, zone, off: promptsOff, rows };
  return rows;
}

/** The rows the dock shows for the latest pinned prompt. It changes only
 *  when a prompt takes more or fewer rows, as when a fight starts or
 *  ends, so a component that reads it renders only then, in the same
 *  commit as the dock. */
export function usePinnedDockRows(zone: number, promptsOff: boolean): number {
  const read = useCallback(() => getPinnedDockRows(zone, promptsOff), [zone, promptsOff]);
  return useSyncExternalStore(subscribePinnedPrompt, read);
}
