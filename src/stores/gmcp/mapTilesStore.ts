import { onGmcpPackage } from '../../ipc/session';
import type { MapTilesPayload } from '../../panel/map/mapTiles';

export type TilesSnap = { payload: MapTilesPayload; json: string };

// The last Map.Tiles push, kept at module scope. Map.Tiles arrives
// only when you move, so a map that remounts (moved in the panel, or
// shown again after you hide the panel) draws the last map at once
// instead of a blank box until your next step. It also outlives a
// disconnect, so the panel keeps showing where you logged out.
let lastTiles: TilesSnap | null = null;
const tilesListeners = new Set<(snap: TilesSnap) => void>();
let tilesStarted = false;

/** Start hearing Map.Tiles. MapView calls this as it mounts and
 *  startStores does not, so the store holds nothing until a map shows. */
export function startMapTiles(): void {
  if (tilesStarted) return;
  tilesStarted = true;
  void onGmcpPackage<MapTilesPayload>('Map.Tiles', (data) => {
    const payload = data ?? ({} as MapTilesPayload);
    const json = JSON.stringify(payload);
    if (lastTiles && lastTiles.json === json) return;
    lastTiles = { payload, json };
    for (const cb of tilesListeners) cb(lastTiles);
  });
}

export function getMapTiles(): TilesSnap | null {
  return lastTiles;
}

/** Hear each push whose JSON differs from the last. */
export function subscribeMapTiles(cb: (snap: TilesSnap) => void): () => void {
  tilesListeners.add(cb);
  return () => {
    tilesListeners.delete(cb);
  };
}
