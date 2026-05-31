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

// Module-level store for Group.Info + Char.Worth. Mirrors chatStore
// so the ChatGroupPane can close and reopen without losing the
// last-pushed roster / worth snapshot. Subscribes to GMCP once on
// first read and keeps the latest state per package.
let group: GroupInfo = {};
let worth: Worth = {};
let listeners: Array<(state: { group: GroupInfo; worth: Worth }) => void> = [];
let started = false;

function notify() {
  const snapshot = { group, worth };
  for (const l of listeners) l(snapshot);
}

export function startGroupStore(): void {
  if (started) return;
  started = true;
  void onGmcpPackage<unknown>('Group.Info', (data) => {
    group = data && typeof data === 'object' ? (data as GroupInfo) : {};
    notify();
  });
  void onGmcpPackage<unknown>('Char.Worth', (data) => {
    if (data && typeof data === 'object') {
      worth = { ...worth, ...(data as Worth) };
      notify();
    }
  });
  void onState((payload) => {
    if (payload.kind === 'disconnected') {
      group = {};
      worth = {};
      notify();
    }
  });
}

export function getGroupState(): { group: GroupInfo; worth: Worth } {
  startGroupStore();
  return { group, worth };
}

export function subscribeGroupState(
  cb: (state: { group: GroupInfo; worth: Worth }) => void,
): () => void {
  startGroupStore();
  listeners.push(cb);
  return () => {
    listeners = listeners.filter((l) => l !== cb);
  };
}
