import { useSyncExternalStore } from 'react';
import { onOutput, onState, type SessionOutput } from '../session';
import { closePinRow } from '../terminalRegion';
import { createStore } from './store';

// The prompt the session pinned above the command line, as the text the
// band draws: the output's pin field, decoded. It listens from launch, so
// the band shows the latest prompt even when it mounts after the output
// that carried it, as it does when you choose Pinned. An empty pin and a
// disconnect clear it.
//
// It also keeps whether the row the pinned prompt held is still where the
// next thing lands, as each renderer keeps it. Enter on an empty line
// echoes nothing only then. A prompt Vosh does not read, such as the
// pager's [Hit Return to continue], stays in the text and closes the row,
// so Enter there ends its row as it always did.

const store = createStore<string | null>(null);
let started = false;
let rowOpen = false;

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
    if (out.pin) store.set(out.pin.length > 0 ? decoder.decode(out.pin) : null);
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
