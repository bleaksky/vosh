import { useEffect, useSyncExternalStore } from 'react';
import { profileHoldEdits } from '../ipc/profiles';
import { createStore } from '../stores/store';
import { getSelected, getSessions, subscribeSessions } from '../stores/session/sessionsStore';

// The profile Settings shows and edits, by Q14 and board 7 of the
// Sessions review. Settings shows the profile the selected session
// plays and names it in every call, so a save that lands after the
// selection moved still reaches the profile it was made on.
//
// A page with unsaved edits holds its profile. While it holds, a
// selection that brings another profile to the front leaves Settings
// where it is, and the header says so. Save or Discard lets go, and
// Settings moves to the selected session's profile, which every page
// that read the old one hears through subscribeShownMoves. A selection
// on the same profile only changes the session the header names. Rust
// keeps a held profile open after its last session leaves it, so Save
// still finds it, and closes it once the hold lets go.

export interface Shown {
  /** The profile Settings shows, null until the session list names one. */
  profile: string | null;
  /** The session the header names, the selected one while it plays
   *  `profile`, else the one Settings showed last. Null until the list
   *  names one. */
  session: number | null;
  /** A page holds `profile` with unsaved edits while the selected
   *  session plays another. */
  held: boolean;
}

const store = createStore<Shown>({ profile: null, session: null, held: false });
/** One token for each page that holds unsaved edits. */
const holds = new Set<symbol>();
const moves = new Set<(profile: string) => void>();
let started = false;

/** Follow the selected session and the profile it plays, unless a page
 *  holds another profile. */
function settle(): void {
  const selected = getSelected();
  const row = getSessions().find((r) => r.id === selected);
  if (!row?.profile) return;
  const now = store.get();
  if (holds.size > 0 && now.profile !== null && row.profile !== now.profile) {
    if (!now.held) store.set({ ...now, held: true });
    return;
  }
  if (now.profile === row.profile && now.session === selected && !now.held) return;
  const moved = now.profile !== null && now.profile !== row.profile;
  store.set({ profile: row.profile, session: selected, held: false });
  if (moved) for (const cb of [...moves]) cb(row.profile);
}

function start(): void {
  if (started) return;
  started = true;
  subscribeSessions(settle);
  // A hold taken before the session list named a profile reaches Rust
  // once it does.
  store.subscribe(tellRust);
  settle();
}

// Settings starts following the session list as its header first
// draws. A read before then finds no profile, and a call that names none
// reaches the selected session's, the one Settings is about to show.

/** What Settings shows now. */
export function getShown(): Shown {
  return store.get();
}

/** The profile every Settings call names, or undefined before the
 *  session list names one, when a call names none and reaches the
 *  selected session's. */
export function getShownProfile(): string | undefined {
  return store.get().profile ?? undefined;
}

/** Whether a page holds the profile Settings shows while the selected
 *  session plays another. The events the app sends then speak of the
 *  profile in front, not the one Settings shows. */
export function isShownHeld(): boolean {
  return store.get().held;
}

function subscribe(cb: () => void): () => void {
  start();
  return store.subscribe(cb);
}

/** What Settings shows, for a view that draws it. */
export function useShown(): Shown {
  return useSyncExternalStore(subscribe, getShown, getShown);
}

/** Hear Settings move to another profile, when the selection brings
 *  one to the front or a page lets go of the one it held. Returns the
 *  unsubscribe. */
export function subscribeShownMoves(cb: (profile: string) => void): () => void {
  start();
  moves.add(cb);
  return () => {
    moves.delete(cb);
  };
}

/** The profile Rust last heard Settings holds. */
let told: string | null = null;

/** Tell Rust which profile Settings holds, or none, when that changed. */
function tellRust(): void {
  const held = holds.size > 0 ? store.get().profile : null;
  if (held === told) return;
  told = held;
  profileHoldEdits(held).catch(() => {});
}

/** Hold the profile Settings shows while `dirty`, so a selection that
 *  brings another profile to the front waits for Save or Discard. */
export function useProfileHold(dirty: boolean): void {
  useEffect(() => {
    if (!dirty) return;
    start();
    const token = Symbol('hold');
    holds.add(token);
    tellRust();
    return () => {
      holds.delete(token);
      tellRust();
      settle();
    };
  }, [dirty]);
}
