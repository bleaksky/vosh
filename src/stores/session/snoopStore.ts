import { useCallback, useEffect, useSyncExternalStore } from 'react';
import {
  onSnoop,
  onSnoopOutput,
  snoopGet,
  type SnoopList,
  type SnoopSnapshot,
  type SnoopTab,
} from '../../ipc/snoop';
import { getSelected, getSessions, subscribeSessions } from './sessionsStore';
import { createSessionStore } from '../sessionStore';

// The players each session snoops, as the backend keeps them, with the
// tab in front and the tabs with lines you have not seen. The backend
// sends the tab list on session://snoop once per read that changed it,
// and a window that shows a session for the first time asks snoop_get
// for every tab with its text. A start puts its tab in front, a repeat
// snoop of the same player too, and a tab that goes hands the front to
// its neighbor. A disconnect keeps the tabs, as the backend does, which
// marks them ended.
//
// The text goes to the terminals that subscribe to it, never through the
// state, so React draws nothing for each packet. Each terminal hears its
// player's text once from the snapshot, whole, and then each new piece.
// The store keeps each tab's newest 5,000 lines too, as the backend
// does, so a terminal that mounts later, when you pick a tab or a
// session, starts from everything the tab holds.

export interface Snoops {
  /** Every tab, in the order they started. */
  tabs: readonly SnoopTab[];
  /** The tabs show in the snoop window, and the split stays closed. */
  windowed: boolean;
  /** The name of the tab in front, null with no tab. */
  selected: string | null;
  /** The tabs that got lines you have not seen, those behind and every
   *  tab while the split is folded. */
  unread: ReadonlySet<string>;
}

const NOTHING: ReadonlySet<string> = new Set();
const NONE: Snoops = { tabs: [], windowed: false, selected: null, unread: NOTHING };

/** The split is folded to its strip, so no tab shows its lines. The fold
 *  is the profile's, so it holds for every session. */
let folded = false;

/** The tab in front once `tabs` replaces `before`: a tab that started or
 *  came back live since, else the one in front if it stayed, else the one
 *  at its place, else the last. */
function front(before: Snoops, tabs: readonly SnoopTab[]): string | null {
  const was = new Map(before.tabs.map((tab) => [tab.name, tab]));
  const started = tabs.filter((tab) => tab.live && !was.get(tab.name)?.live);
  if (started.length > 0) return started[started.length - 1].name;
  const { selected } = before;
  if (tabs.some((tab) => tab.name === selected)) return selected;
  if (tabs.length === 0) return null;
  const at = before.tabs.findIndex((tab) => tab.name === selected);
  return tabs[at < 0 ? tabs.length - 1 : Math.min(at, tabs.length - 1)].name;
}

/** The state once the backend sends `list`. A tab that goes takes its
 *  unread mark along, and the tab in front shows its lines unless the
 *  split is folded. */
function foldSnoopList(now: Snoops, list: SnoopList): Snoops {
  const selected = front(now, list.tabs);
  const names = new Set(list.tabs.map((tab) => tab.name));
  const keep = (name: string) => names.has(name) && (folded || name !== selected);
  let unread = now.unread;
  if (![...unread].every(keep)) unread = new Set([...unread].filter(keep));
  return { tabs: list.tabs, windowed: list.windowed, selected, unread };
}

/** The state once `name` gets lines. */
function heard(now: Snoops, name: string): Snoops {
  if (now.unread.has(name) || (name === now.selected && !folded)) return now;
  return { ...now, unread: new Set([...now.unread, name]) };
}

/** Put `name` in front, which shows its lines unless the split is
 *  folded. */
function select(now: Snoops, name: string): Snoops {
  if (!now.tabs.some((tab) => tab.name === name)) return now;
  const seen = !folded && now.unread.has(name);
  if (now.selected === name && !seen) return now;
  const unread = seen ? new Set([...now.unread].filter((n) => n !== name)) : now.unread;
  return { ...now, selected: name, unread };
}

/** The lines a tab keeps, as src-tauri/src/session/snoop.rs keeps
 *  them, and its terminal's scrollback. */
export const SNOOP_LINES = 5_000;

/** The lines a tab may hold past SNOOP_LINES before it drops the oldest,
 *  so a trim runs once in a while and not for each packet. */
const SLACK = 500;

/** One tab's text and how many line ends it holds. */
interface Kept {
  text: string;
  lines: number;
}

/** The text of each tab, by session and then by player. */
const texts = new Map<number, Map<string, Kept>>();

/** `text` from the line after its `drop`th line end on. The game ends
 *  its lines with `\n\r`, so the `\r` goes with its `\n`. */
function dropLines(text: string, drop: number): string {
  let at = -1;
  for (let i = 0; i < drop; i++) at = text.indexOf('\n', at + 1);
  return text.slice(text[at + 1] === '\r' ? at + 2 : at + 1);
}

function keep(session: number, name: string, text: string, whole: boolean): void {
  let tabs = texts.get(session);
  if (!tabs) texts.set(session, (tabs = new Map()));
  const was = whole ? undefined : tabs.get(name);
  let kept: Kept = {
    text: (was?.text ?? '') + text,
    lines: (was?.lines ?? 0) + (text.match(/\n/g)?.length ?? 0),
  };
  if (kept.lines > SNOOP_LINES + (whole ? 0 : SLACK)) {
    kept = { text: dropLines(kept.text, kept.lines - SNOOP_LINES), lines: SNOOP_LINES };
  }
  tabs.set(name, kept);
}

/** Drop the text of every tab `session` no longer has. */
function forget(session: number, tabs: readonly SnoopTab[]): void {
  const kept = texts.get(session);
  if (!kept) return;
  for (const name of kept.keys()) {
    if (!tabs.some((tab) => tab.name === name)) kept.delete(name);
  }
}

type Listener = (session: number, name: string, text: string, whole: boolean) => void;
const listeners = new Set<Listener>();

function hand(session: number, name: string, text: string, whole: boolean): void {
  keep(session, name, text, whole);
  for (const cb of listeners) cb(session, name, text, whole);
}

/** The snapshot of a session, with the session it is for. */
interface Asked {
  session: number;
  snapshot: SnoopSnapshot;
}

/** Every tab from the snapshot. The store takes it only when nothing
 *  was heard since the ask, so its text goes to the terminals whole,
 *  in place of what they show. */
function fromSnapshot(now: Snoops, { session, snapshot }: Asked): Snoops {
  for (const { name, text } of snapshot.tabs) hand(session, name, text, true);
  const tabs = snapshot.tabs.map(({ name, live, ended_at, last_output_at }) => ({
    name,
    live,
    ended_at,
    last_output_at,
  }));
  forget(session, tabs);
  return foldSnoopList(now, { tabs, windowed: snapshot.windowed });
}

const store = createSessionStore<Snoops>({
  state: NONE,
  events: [
    (apply) =>
      onSnoop((list, session) => {
        forget(session, list.tabs);
        apply(session, (now) => foldSnoopList(now, list));
      }),
    // A session that closes takes its tabs' text along.
    () =>
      subscribeSessions(() => {
        const open = new Set(getSessions().map((row) => row.id));
        for (const session of texts.keys()) if (!open.has(session)) texts.delete(session);
      }),
    (apply) =>
      onSnoopOutput(({ name, text }, session) => {
        apply(session, (now) => heard(now, name));
        hand(session, name, text, false);
      }),
  ],
  connection: (now) => now,
  snapshot: {
    ask: (session) => snoopGet(session).then((snapshot): Asked => ({ session, snapshot })),
    take: (now, data) => fromSnapshot(now, data as Asked),
  },
});

export const startSnoopStore = store.start;
export const getSnoops = store.get;
/** The snoops of the session in front. */
export const useSnoops = store.use;

/** How many snoops run in `session`. */
export function liveSnoops(session: number): number {
  return store.stateOf(session).tabs.filter((tab) => tab.live).length;
}

/** Hear each change to the snoops of `session`. */
function useMoves(session: number): (cb: () => void) => () => void {
  return useCallback(
    (cb: () => void) =>
      store.subscribeStates((moved) => {
        if (moved === session) cb();
      }),
    [session],
  );
}

/** How many snoops run in `session`, kept current, for its row. */
export function useLiveSnoops(session: number): number {
  const count = () => liveSnoops(session);
  return useSyncExternalStore(useMoves(session), count, count);
}

/** The snoops of `session` whatever is selected, for its snoop window,
 *  which reads every tab with its text the first time it shows them. */
export function useSnoopsOf(session: number): Snoops {
  useEffect(() => store.ask(session), [session]);
  const now = () => store.stateOf(session);
  return useSyncExternalStore(useMoves(session), now, now);
}

/** Put the tab of `name` in front in `session`, the session in front
 *  unless it names another. */
export function selectSnoop(name: string, session: number = getSelected()): void {
  store.apply(session, (now) => select(now, name));
}

/** The split folded to its strip, or opened again, which shows the tab
 *  in front once more. */
export function setSnoopsFolded(on: boolean): void {
  if (folded === on) return;
  folded = on;
  if (!on) store.apply(getSelected(), (now) => (now.selected ? select(now, now.selected) : now));
}

/** Every line the tab of `name` in `session` holds, as the game sent
 *  them. */
export function snoopText(session: number, name: string): string {
  return texts.get(session)?.get(name)?.text ?? '';
}

/** Hear the text of every snooped player, with its session and name.
 *  `whole` marks a snapshot, every line the tab holds, in place of what
 *  the terminal shows. */
export function subscribeSnoopOutput(cb: Listener): () => void {
  store.start();
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}
