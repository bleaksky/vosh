import { onGmcpPackage, onState } from './session';
import { getHidden, subscribeHidden } from './stores/hiddenStore';
import { isHiddenFlag } from './stores/store';

export interface GroupMember {
  /** Stable per character identity. Aabahran sends it because the name
   *  is masked (blind, doppelganger, shapeshifted) and two members can
   *  share one. Servers that omit it fall back to the name. */
  id?: number | string;
  name?: string;
  level?: number | string;
  class?: string;
  hp_pct?: number | string;
  mana_pct?: number | string;
  move_pct?: number | string;
  tnl?: number | string;
}

export interface GroupInfo {
  leader?: string;
  members?: GroupMember[];
  /** The game hides your group. Aabahran sends `{"hidden": true}` under
   *  lamented tears, with no leader and no members, until a Group.Info
   *  without the flag arrives. */
  hidden?: true;
}

export interface Worth {
  gold?: number | string;
  bank?: number | string;
  exp?: number | string;
  tnl?: number | string;
  trains?: number | string;
  practices?: number | string;
  cps?: number | string;
  rps?: number | string;
}

export interface GroupState {
  group: GroupInfo;
  worth: Worth;
  /** Logged-in character from Char.Status / Char.Name, so the pane
   *  can pick out the player's own row in the roster. */
  self?: string | undefined;
}

// Module-level store for Group.Info + Char.Worth. Mirrors chatStore
// so the ChatGroupPane can close and reopen without losing the
// last-pushed roster / worth snapshot. Subscribes to GMCP once on
// first read and keeps the latest state per package.
//
// An older server build sends the whole roster under lamented tears,
// your own row with its health included, and no flag. The backend
// works out that it is hidden, and while hiddenStore's `group` holds,
// the store reads as the hidden Group.Info the new build sends, so no
// roster shows.
let group: GroupInfo = {};
let worth: Worth = {};
let self: string | undefined;
let listeners: Array<(state: GroupState) => void> = [];
let started = false;
let hiddenByBackend = false;

/** The Group.Info a hidden group reads as. One object, so a pane that
 *  compares snapshots sees no change while the group stays hidden. */
const HIDDEN_GROUP: GroupInfo = { hidden: true };

/** The group the panes see, hidden while the backend says so. */
function shownGroup(): GroupInfo {
  return hiddenByBackend && group.hidden !== true ? HIDDEN_GROUP : group;
}

function notify() {
  const snapshot = { group: shownGroup(), worth, self };
  for (const l of listeners) l(snapshot);
}

/** Dedupe key for a member. The server `id` when present, else the
 *  name, so servers without ids keep the old behavior. */
export function memberKey(m: GroupMember): string {
  if ((typeof m.id === 'number' && Number.isFinite(m.id)) || (typeof m.id === 'string' && m.id)) {
    return `id:${m.id}`;
  }
  return `name:${m.name ?? '?'}`;
}

// Collapse duplicate member rows, keeping the LAST occurrence per key
// (Group.Info is a snapshot; later rows carry the freshest stats). Older
// Aabahran builds appended blinded characters as "someone" without
// deduping, so after a dirt kick the roster could arrive with dozens of
// stale duplicates and grow without bound. The server now sends a
// stable `id` per member, so distinct masked members that share a name
// stay apart while a repeated member still folds into one row. Order of
// first appearance is preserved.
export function dedupeMembers(info: GroupInfo): GroupInfo {
  if (!Array.isArray(info.members)) return info;
  const seen = new Map<string, GroupMember>();
  for (const m of info.members) {
    if (!m || typeof m !== 'object') continue;
    seen.set(memberKey(m), m);
  }
  if (seen.size === info.members.length) return info;
  return { ...info, members: [...seen.values()] };
}

/** Parse a Group.Info payload. A hidden one keeps only the flag, so no
 *  roster from it ever shows. Anything that is not an object reads as
 *  solo. */
export function parseGroupInfo(data: unknown): GroupInfo {
  if (!data || typeof data !== 'object' || Array.isArray(data)) return {};
  if (isHiddenFlag(data)) return { hidden: true };
  return dedupeMembers(data as GroupInfo);
}

export function startGroupStore(): void {
  if (started) return;
  started = true;
  void onGmcpPackage<unknown>('Group.Info', (data) => {
    group = parseGroupInfo(data);
    notify();
  });
  void onGmcpPackage<unknown>('Char.Worth', (data) => {
    if (data && typeof data === 'object') {
      worth = { ...worth, ...(data as Worth) };
      notify();
    }
  });
  const takeName = (data: { name?: unknown }) => {
    if (typeof data?.name === 'string' && data.name.trim().length > 0) {
      self = data.name.trim();
      notify();
    }
  };
  void onGmcpPackage<{ name?: unknown }>('Char.Status', takeName);
  void onGmcpPackage<{ name?: unknown }>('Char.Name', takeName);
  void onState((payload) => {
    if (payload.kind === 'disconnected') {
      group = {};
      worth = {};
      self = undefined;
      notify();
    }
  });
  hiddenByBackend = getHidden().group;
  subscribeHidden(() => {
    const now = getHidden().group;
    if (now === hiddenByBackend) return;
    hiddenByBackend = now;
    notify();
  });
}

export function getGroupState(): GroupState {
  startGroupStore();
  return { group: shownGroup(), worth, self };
}

export function subscribeGroupState(cb: (state: GroupState) => void): () => void {
  startGroupStore();
  listeners.push(cb);
  return () => {
    listeners = listeners.filter((l) => l !== cb);
  };
}
