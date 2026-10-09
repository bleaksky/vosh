import { useSyncExternalStore } from 'react';
import {
  getStartedGet,
  getStartedSet,
  subscribeGetStartedOpen,
  type GetStartedState,
} from '../../ipc/getStarted';
import { onGmcpPackage, type ConnectionTarget } from '../../ipc/session';
import { onGameLine } from '../../ipc/terminal';
import { loadTarget, subscribeConnectionTarget } from '../../stores/session/useConnection';
import { createStore } from '../../stores/store';
import { pushToast } from '../../stores/toasts';
import { doneByFacts, onForsakenLands, type GetStartedFacts, type StepId } from './steps';

// Where Get started stands in the main window. profiles.toml keeps the
// install's place in it, read with getStartedGet as the window mounts.
// A missing table keeps the card shut, and at_launch opens it.
// The world follows the saved target, so the steps match where Connect
// dials.
//
// The card shows its list or a step's page, folds to the notice in the
// corner and comes back where you left it. Close or Done ends it, which
// clears at_launch, and the Help menu, the palette and Help open it on
// its list again. The first connect clears at_launch and adds
// `connect` to done, and each later step adds its id. Steps finish only
// once Get started has a place to keep them, so a player who never
// opened it leaves profiles.toml as it is.

/** Where the card is: shut, open on its list or a page, or folded to
 *  the notice. */
export type CardShows = 'shut' | 'open' | 'folded';

export interface GetStartedView {
  /** What profiles.toml keeps, or null when it has no table and you
   *  have not opened Get started. */
  saved: GetStartedState | null;
  shows: CardShows;
  /** The step whose page is open, or null for the list. */
  page: StepId | null;
  /** The saved world, which picks the steps. */
  target: ConnectionTarget;
}

const store = createStore<GetStartedView>({
  saved: null,
  shows: 'shut',
  page: null,
  target: loadTarget(),
});

function update(change: Partial<GetStartedView>): void {
  store.set({ ...store.get(), ...change });
}

/** Keep `saved` and send it to profiles.toml. */
function save(saved: GetStartedState): void {
  update({ saved });
  getStartedSet(saved).catch((e: unknown) => console.error('[get started] save failed', e));
}

/** Mark a step done, once. The first connect also stops the card
 *  opening at launch. */
export function markDone(id: StepId): void {
  const { saved } = store.get();
  if (!saved || saved.done.includes(id)) return;
  save({ atLaunch: id === 'connect' ? false : saved.atLaunch, done: [...saved.done, id] });
}

/** Mark the steps the live facts finish. */
export function noteFacts(facts: GetStartedFacts): void {
  for (const id of doneByFacts(store.get().target.host, facts)) markDone(id);
}

/** Open the card on its list, as the Help menu, the palette and Help
 *  do. A player with no table yet starts one that never opens at
 *  launch. */
export function openList(): void {
  const saved = store.get().saved ?? { atLaunch: false, done: [] };
  update({ saved, shows: 'open', page: null });
}

/** Open a step's page, or the list with null. */
export function showPage(page: StepId | null): void {
  update({ shows: 'open', page });
}

/** Fold the card to its notice. */
export function fold(): void {
  if (store.get().shows === 'open') update({ shows: 'folded' });
}

/** Bring the card back from its notice where you left it. */
export function unfold(): void {
  if (store.get().shows === 'folded') update({ shows: 'open' });
}

/** End Get started, from Close or Done, on the card or the notice. It
 *  no longer opens at launch, and a toast says where it lives. */
export function end(): void {
  const { saved } = store.get();
  update({ shows: 'shut', page: null });
  if (saved?.atLaunch) save({ ...saved, atLaunch: false });
  pushToast({ kind: 'info', message: 'Get started closed', meta: 'Help opens it again' });
}

/** Whether a game line or packet may still finish the connect step. */
function connectOpen(): boolean {
  const { saved } = store.get();
  return !!saved && !saved.done.includes('connect');
}

/** Read the install's place in Get started and start following the
 *  world, Help and the first connect. Returns the cleanup. */
export function mountGetStarted(): () => void {
  let alive = true;
  const unlisteners: (() => void)[] = [];
  const keep = (p: Promise<() => void>) => {
    void p
      .then((un) => (alive ? unlisteners.push(un) : un()))
      .catch((e: unknown) => console.error('[get started] listen failed', e));
  };
  getStartedGet()
    .then((saved) => {
      // Help opened it before the read landed, so keep what it started.
      if (!alive || store.get().saved) return;
      update({ saved, shows: saved?.atLaunch ? 'open' : 'shut' });
    })
    .catch((e: unknown) => console.error('[get started] read failed', e));
  unlisteners.push(subscribeConnectionTarget((target) => update({ target })));
  keep(subscribeGetStartedOpen(openList));
  const playing = () => {
    if (connectOpen() && onForsakenLands(store.get().target.host)) markDone('connect');
  };
  keep(onGmcpPackage('Char.Status', playing));
  keep(onGmcpPackage('Char.Vitals', playing));
  keep(
    onGameLine(
      () => connectOpen() && !onForsakenLands(store.get().target.host),
      () => markDone('connect'),
    ),
  );
  return () => {
    alive = false;
    for (const un of unlisteners) un();
  };
}

/** Where Get started stands now. */
export const getGetStarted = store.get;

export function useGetStarted(): GetStartedView {
  return useSyncExternalStore(store.subscribe, store.get, store.get);
}
