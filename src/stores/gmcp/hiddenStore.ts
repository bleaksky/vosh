import { hiddenGet, onHidden, type HiddenPayload } from '../../ipc/prompt';
import { createSessionStore } from '../sessionStore';

// Which values the game hides right now, from session://hidden. The
// prompt engine in the backend works it out on every server build from
// the latest Char.Vitals, Char.Affects, Group.Info and Char.Combat and
// from your prompt, and never stores it. It reports each change once, so
// the store also asks hidden_get for a session's last report the first
// time it shows that session, or a window that opens or reloads during
// the song, Settings among them, would show the true values an older
// build sends. Each session keeps its own flags.
//
// The new build marks each hidden packet with `"hidden": true`, and
// the stores read that flag themselves, so this store adds nothing
// there. An older build sends the true values under lamented tears and
// only Char.Affects names the song, so this store is how the panes
// learn to hide them. vitalsStore, affectsStore, combatStore and
// groupStore each OR their field from here into their own hidden flag.

export type HiddenState = HiddenPayload;

/** Nothing hidden, the state before the first report and after a
 *  disconnect. */
export const NOTHING_HIDDEN: HiddenState = {
  vitals: false,
  tank: false,
  opponent: false,
  affects: false,
  group: false,
};

/** Read a session://hidden payload. A field that is not `true` reads
 *  as shown. */
export function parseHidden(payload: unknown): HiddenState {
  const p = payload && typeof payload === 'object' ? (payload as Record<string, unknown>) : {};
  return {
    vitals: p.vitals === true,
    tank: p.tank === true,
    opponent: p.opponent === true,
    affects: p.affects === true,
    group: p.group === true,
  };
}

function same(a: HiddenState, b: HiddenState): boolean {
  return (
    a.vitals === b.vitals &&
    a.tank === b.tank &&
    a.opponent === b.opponent &&
    a.affects === b.affects &&
    a.group === b.group
  );
}

/** The state after a report, the same state when nothing in it moved. */
function report(state: HiddenState, payload: unknown): HiddenState {
  const next = parseHidden(payload);
  return same(state, next) ? state : next;
}

const store = createSessionStore<HiddenState>({
  state: NOTHING_HIDDEN,
  events: [
    (apply) => onHidden((payload, session) => apply(session, (state) => report(state, payload))),
  ],
  snapshot: {
    ask: hiddenGet,
    take: (state, payload) => (payload == null ? state : report(state, payload)),
  },
});

export const startHiddenStore = store.start;
/** What the selected session hides. */
export const getHidden = store.get;
export const subscribeHidden = store.subscribe;
/** What the session `session` names hides. */
export const getHiddenOf = store.stateOf;
/** Hear each session's report change, with that session. */
export const subscribeHiddenOf = store.subscribeStates;
