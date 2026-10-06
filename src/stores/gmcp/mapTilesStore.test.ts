import { beforeEach, describe, expect, it, vi } from 'vitest';
import { aabahranMapPacket } from '../../test/aabahranGmcp';

// Drives the store through a fake Tauri event bus with the Map.Tiles
// packets in fixtures/gmcp/aabahran/map. Each test loads a fresh store
// module, since the store keeps the last push at module scope.

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();

vi.mock('@tauri-apps/api/event', () => ({
  listen: async (event: string, cb: Handler) => {
    let set = handlers.get(event);
    if (!set) handlers.set(event, (set = new Set()));
    set.add(cb);
    return () => set.delete(cb);
  },
  emit: async () => undefined,
}));

/** Send a Map.Tiles packet as the backend emits it from session 1. */
function push(data: unknown): void {
  for (const cb of handlers.get('session://gmcp/Map-Tiles') ?? []) {
    cb({ payload: { session: 1, data } });
  }
}

/** A map fixture's data, parsed fresh on each call. */
const tiles = (name: string) => aabahranMapPacket(name).data;
const CARANDUIN = 'caranduin-west-of-the-fountain.gmcp';
const VAL_MIRAN = 'val-miran-central-square.gmcp';

async function load() {
  const store = await import('./mapTilesStore');
  store.startMapTiles();
  await new Promise((resolve) => setTimeout(resolve, 0));
  return store;
}

beforeEach(() => {
  vi.resetModules();
  handlers.clear();
});

describe('mapTilesStore', () => {
  it('notifies no one when a push repeats the last JSON', async () => {
    const store = await load();
    const heard = vi.fn();
    store.subscribeMapTiles(heard);
    push(tiles(CARANDUIN));
    const first = store.getMapTiles();
    push(tiles(CARANDUIN));
    expect(heard).toHaveBeenCalledTimes(1);
    expect(store.getMapTiles()).toBe(first);
  });

  it('notifies a push that differs from the last', async () => {
    const store = await load();
    const heard = vi.fn();
    store.subscribeMapTiles(heard);
    push(tiles(CARANDUIN));
    push(tiles(VAL_MIRAN));
    expect(heard).toHaveBeenCalledTimes(2);
    expect(heard).toHaveBeenLastCalledWith({
      payload: tiles(VAL_MIRAN),
      json: JSON.stringify(tiles(VAL_MIRAN)),
    });
  });

  it('keeps the last push for a subscriber that comes later', async () => {
    const store = await load();
    expect(store.getMapTiles()).toBeNull();
    push(tiles(CARANDUIN));
    push(tiles(VAL_MIRAN));
    const later = vi.fn();
    store.subscribeMapTiles(later);
    expect(later).not.toHaveBeenCalled();
    expect(store.getMapTiles()).toEqual({
      payload: tiles(VAL_MIRAN),
      json: JSON.stringify(tiles(VAL_MIRAN)),
    });
  });

  it('reads a null packet as an empty one', async () => {
    const store = await load();
    push(null);
    expect(store.getMapTiles()).toEqual({ payload: {}, json: '{}' });
  });

  it('stops notifying once you unsubscribe', async () => {
    const store = await load();
    const heard = vi.fn();
    const stop = store.subscribeMapTiles(heard);
    push(tiles(CARANDUIN));
    stop();
    push(tiles(VAL_MIRAN));
    expect(heard).toHaveBeenCalledTimes(1);
  });
});
