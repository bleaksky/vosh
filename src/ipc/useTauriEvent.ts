// How a component hears a Tauri event while it is mounted.

import { useEffect, useRef } from 'react';
import type { UnlistenFn } from '@tauri-apps/api/event';

/** Hear an event from mount to unmount through `subscribe`, which takes
 *  a callback and resolves to its unlisten, as the subscribe functions in
 *  src/ipc do. It subscribes once, at mount, so a subscribe built from
 *  render values keeps the first render's. Each payload goes to the
 *  newest render's `handler`, so a handler can read props and state
 *  without subscribing again. A subscription that resolves after unmount
 *  is dropped as it lands, and no payload reaches the handler once the
 *  component is gone. An event that carries nothing has a payload of
 *  void. */
export function useTauriEvent<T = void>(
  subscribe: (cb: (payload: T) => void) => Promise<UnlistenFn>,
  handler: (payload: T) => void,
): void {
  const subscribeRef = useRef(subscribe);
  const handlerRef = useRef(handler);
  handlerRef.current = handler;
  useEffect(() => {
    let cancelled = false;
    let unlisten: UnlistenFn | undefined;
    subscribeRef
      .current((payload) => {
        if (!cancelled) handlerRef.current(payload);
      })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {
        // No event bridge, as on a page outside Tauri. Nothing arrives.
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);
}
