import { subscribeProfileSwitched } from '../../ipc/profiles';
import {
  onPromptGagWithoutReader,
  promptGagsWithoutReader,
  subscribePromptConfigChanged,
} from '../../ipc/prompt';
import { onState } from '../../ipc/session';
import { subscribeUiConfigReplaced } from '../../ipc/uiConfig';
import { createGmcpStore } from '../gmcp/gmcpStore';
import { getSelected, subscribeSelected } from './sessionsStore';

// The triggers that hid your prompt in a session while the profile reads
// no prompt, so Vosh drew nothing in its place. The Triggers editor marks
// them. The session names each one once on
// session://prompt-gag-without-reader, and Settings may open later, so
// the store also asks for the list when it starts. A connection that
// opens or closes starts the list over, as the session does. Once the
// profile reads your prompt, or another profile takes over, the session
// forgets them, so the store asks for the list again then and takes it
// whole.
//
// Each session keeps its own list, and the editor marks the selected
// session's. A session behind forgets its list too when its profile
// reads your prompt, so the store asks for the list again on each
// selection.

type Gags = ReadonlySet<string>;
type Apply = (session: number, change: (now: Gags) => Gags) => void;

const NONE: Gags = new Set();

/** For each session, the connects, disconnects and asks heard for it. An
 *  answer applies only when none came after it was asked for. */
const asks = new Map<number, number>();

function bump(session: number): number {
  const count = (asks.get(session) ?? 0) + 1;
  asks.set(session, count);
  return count;
}

function add(now: Gags, names: readonly string[]): Gags {
  const missing = names.filter((name) => !now.has(name));
  return missing.length > 0 ? new Set([...now, ...missing]) : now;
}

/** Ask the selected session for its list and take it whole, unless a
 *  connection opened or closed or another ask began meanwhile. */
function reread(apply: Apply): void {
  const session = getSelected();
  const mine = bump(session);
  promptGagsWithoutReader(session)
    .then((names) => {
      if (asks.get(session) !== mine) return;
      apply(session, (now) => {
        const same = names.length === now.size && names.every((name) => now.has(name));
        if (same) return now;
        return names.length > 0 ? new Set(names) : NONE;
      });
    })
    .catch(() => undefined);
}

const store = createGmcpStore<Gags>({
  state: NONE,
  events: [
    (apply) =>
      onPromptGagWithoutReader(({ trigger }, session) => {
        if (typeof trigger === 'string' && trigger.length > 0) {
          apply(session, (now) => add(now, [trigger]));
        }
      }),
    () =>
      onState((payload) => {
        if (payload.kind !== 'connected') bump(payload.session);
      }),
    (apply) => subscribePromptConfigChanged(() => reread(apply)),
    (apply) => subscribeProfileSwitched(() => reread(apply)),
    (apply) => subscribeUiConfigReplaced(() => reread(apply)),
    (apply) => subscribeSelected(() => reread(apply)),
  ],
  connection: (now, payload) => (payload.kind === 'connected' ? now : NONE),
  // The names the session gave before this window opened, beside any
  // that came meanwhile.
  snapshot: { ask: promptGagsWithoutReader, take: (now, names) => add(now, names as string[]) },
});

export const getPromptGags = store.get;
export const subscribePromptGags = store.subscribe;

/** The triggers that hid your prompt in the selected session with
 *  nothing drawn in its place. */
export const usePromptGags = store.use;
