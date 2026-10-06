import type { MapTilesPayload } from '../../panel/map/mapTiles';
import { createGmcpStore } from './gmcpStore';

export type TilesSnap = { payload: MapTilesPayload; json: string };

// The last Map.Tiles push, kept at module scope. Map.Tiles arrives
// only when you move, so a map that remounts (moved in the panel, or
// shown again after you hide the panel) draws the last map at once
// instead of a blank box until your next step. It also outlives a
// disconnect, so the panel keeps showing where you logged out.
const store = createGmcpStore<TilesSnap | null>({
  state: null,
  packages: {
    'Map.Tiles': (last, data) => {
      const payload = (data ?? {}) as MapTilesPayload;
      const json = JSON.stringify(payload);
      return last?.json === json ? last : { payload, json };
    },
  },
  connection: (last) => last,
});

/** Start hearing Map.Tiles. MapView calls this as it mounts and
 *  startStores does not, so the store holds nothing until a map shows. */
export const startMapTiles = store.start;

export const getMapTiles = store.get;

/** Hear each push whose JSON differs from the last. */
export function subscribeMapTiles(cb: (snap: TilesSnap) => void): () => void {
  return store.subscribe(() => {
    const snap = store.get();
    if (snap) cb(snap);
  });
}
