import { useSyncExternalStore } from 'react';
import { getTarget, onState, onTarget, type TargetPayload } from '../session';
import { createStore } from './store';

// The client target you set with the target command, for the status
// line and the target marker in the room rows. The backend owns it and
// broadcasts session://target on every change, including the clear on
// disconnect. This is not the Char.Combat opponent (see combatStore).
// Lifted from StatusBar.

const EMPTY: TargetPayload = { name: null, room_idx: null, quick_keys: [] };

const store = createStore<TargetPayload>(EMPTY);
let started = false;
// Set once a broadcast lands, so the initial target_get cannot replace
// a newer value if it resolves late.
let heard = false;

export function startTargetStore(): void {
  if (started) return;
  started = true;
  getTarget()
    .then((snap) => {
      if (!heard) store.set(snap);
    })
    .catch(() => undefined);
  void onTarget((payload) => {
    heard = true;
    store.set(payload);
  });
  void onState((payload) => {
    // The backend emits its own clear, but only when a target was set.
    // Clearing here too keeps a stale name off the status line if that
    // event is missed. Quick keys are profile config and stay.
    if (payload.kind === 'disconnected') {
      const prev = store.get();
      if (prev.name !== null || prev.room_idx !== null) {
        store.set({ ...prev, name: null, room_idx: null });
      }
    }
  });
}

export function getTargetState(): TargetPayload {
  return store.get();
}

export function subscribeTargetState(cb: () => void): () => void {
  startTargetStore();
  return store.subscribe(cb);
}

export function useTarget(): TargetPayload {
  return useSyncExternalStore(subscribeTargetState, getTargetState);
}
