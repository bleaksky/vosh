import { useCallback, useSyncExternalStore } from 'react';
import { dockRows } from '../../prompt/pinnedDock';
import { type PromptSpan } from '../../ipc/promptDesign';
import { onOutput, type SessionOutput } from '../../ipc/terminal';
import { createSessionStore } from '../sessionStore';

// The prompt the session pinned above the command line, as the text the
// band draws: the output's pin field, decoded, with where each piece of
// your design landed on it. It listens from launch, so the band shows
// the latest prompt even when it mounts after the output that carried
// it, as it does when you choose Pinned. An empty pin and a disconnect
// clear it. Each session pins its own prompt, so the store keeps the band
// of each session and the dock shows the selected one's.

/** What the band shows: its text, and where each piece of your design
 *  landed on it, rows counted from the band's first. No pieces when it
 *  shows no design, such as the game's prompt with drawing off. */
export interface PinnedBand {
  text: string;
  spans: PromptSpan[];
}

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

const decoder = new TextDecoder('utf-8', { fatal: false });

const store = createSessionStore<PinnedBand | null>({
  state: null,
  events: [
    // Only a write that changes the band decodes, in any session.
    (apply) =>
      onOutput(
        (_, payload) => typeof payload.pin === 'string',
        (out, session) => apply(session, (band) => bandAfterOutput(band, out, decoder)),
      ),
  ],
});

export const startPinnedPromptStore = store.start;

export function getPinnedPrompt(): string | null {
  return store.get()?.text ?? null;
}

/** The band with the pieces of your design on it, or null. */
export const getPinnedBand = store.get;

export const subscribePinnedPrompt = store.subscribe;

/** The latest pinned band with the pieces of your design on it, or
 *  null before one comes and after you disconnect. */
export function usePinnedBand(): PinnedBand | null {
  return useSyncExternalStore(subscribePinnedPrompt, getPinnedBand);
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
