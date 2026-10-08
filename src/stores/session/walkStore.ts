import { onWalk, type WalkProgress } from '../../ipc/session';
import type { GridSpot } from '../../panel/map/mapWalk';
import { createSessionStore } from '../sessionStore';

// Where the selected session's walk stands, for the map's Walking chip
// and Stopped toast, with the route a click on the map last sent, so the
// map knows which path to light and which steps are left (Scripts and
// Panels review, board 9). The walker in Rust sends session://walk on
// each change, and each session keeps its own.
//
// The route holds the cells of the grid the click planned on, in the
// order the steps reach them, and the room clicked. It stays while the
// walk goes on and after it stops, since the Stopped toast dashes the
// steps left, and goes when the walk arrives, when a walk you typed
// takes its place and at a disconnect. A typed walk still under way as
// you click keeps the new route, since the walker finishes its step in
// flight before the route takes over.

/** The route a click on the map sent. */
export interface WalkRoute {
  cells: GridSpot[];
  target: GridSpot;
}

export interface WalkState {
  progress: WalkProgress;
  route: WalkRoute | null;
}

const IDLE: WalkState = { progress: { kind: 'idle' }, route: null };

/** The state a progress event leaves. */
function heard(now: WalkState, progress: WalkProgress): WalkState {
  if (progress.kind === 'idle') return IDLE;
  const typedGoesOn =
    now.progress.kind === 'walking' && !now.progress.route && progress.kind === 'walking';
  const typed = progress.kind === 'walking' && !progress.route && !typedGoesOn;
  return { progress, route: typed ? null : now.route };
}

const store = createSessionStore<WalkState>({
  state: IDLE,
  events: [(apply) => onWalk((progress, session) => apply(session, (now) => heard(now, progress)))],
});

export const startWalkStore = store.start;
export const getWalk = store.get;
export const subscribeWalk = store.subscribe;
export const useWalk = store.use;

/** Keep the route the page just sent to a session's walker. */
export function noteWalkRoute(session: number, route: WalkRoute): void {
  store.apply(session, (now) => ({ ...now, route }));
}
