import { onGmcpPackage, onState } from './session';

export interface GroupMember {
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
let group: GroupInfo = {};
let worth: Worth = {};
let self: string | undefined;
let listeners: Array<(state: GroupState) => void> = [];
let started = false;

function notify() {
  const snapshot = { group, worth, self };
  for (const l of listeners) l(snapshot);
}

// Collapse duplicate member rows, keeping the LAST occurrence per name
// (Group.Info is a snapshot; later rows carry the freshest stats). The
// Aabahran server appends blinded characters as "someone" without
// deduping, so after a dirt kick the roster can arrive with dozens of
// stale duplicates and grow without bound. Collapsing by name keeps the
// pane sane; genuinely distinct blinded members do fold into one row,
// which is the lesser evil against unbounded growth. Order of first
// appearance is preserved.
function dedupeMembers(info: GroupInfo): GroupInfo {
  if (!Array.isArray(info.members)) return info;
  const seen = new Map<string, GroupMember>();
  for (const m of info.members) {
    if (!m || typeof m !== 'object') continue;
    seen.set(m.name ?? '?', m);
  }
  if (seen.size === info.members.length) return info;
  return { ...info, members: [...seen.values()] };
}

export function startGroupStore(): void {
  if (started) return;
  started = true;
  void onGmcpPackage<unknown>('Group.Info', (data) => {
    group = data && typeof data === 'object' ? dedupeMembers(data as GroupInfo) : {};
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
}

export function getGroupState(): GroupState {
  startGroupStore();
  return { group, worth, self };
}

export function subscribeGroupState(cb: (state: GroupState) => void): () => void {
  startGroupStore();
  listeners.push(cb);
  return () => {
    listeners = listeners.filter((l) => l !== cb);
  };
}
