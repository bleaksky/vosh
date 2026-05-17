import { useCallback, useEffect, useRef, useState, type MouseEvent } from 'react';
import {
  getAreaSnapshot,
  onMap,
  onState,
  setRoomAvoid,
  setRoomNote,
  walkToRoom,
  type AreaSnapshot,
  type MapRoom,
} from '../lib/session';
import { MAP_COLORS, SECTORS, hexToRgba, sectorForTerrain } from '../lib/mapPalette';
import { drawTerrainDecorations } from '../lib/terrainDecor';

// Pitch in pixels per cell. Matches the server view so the two modes
// render at the same scale.
const ROOM_SPACING = 20;
const PADDING = 10;

type Style = 'squares' | 'tileset';

const STYLE_KEY = 'vosh.layout.mappingMapStyle';
// Same key as ServerMapView so loading a tileset in either mode makes it
// available in both.
const TILESET_KEY = 'vosh.layout.serverMapTileset';

// Order used by the tileset PNG: one tile per sector index in this slot
// position. Matches the SECTORS table in the palette module.
const TILE_INDEX_FOR_SECTOR: number[] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];

function terrainToTileIndex(terrain: string): number {
  // Tileset PNG slot positions match sector ids 0..12 directly.
  const id = terrainToSectorId(terrain);
  return TILE_INDEX_FOR_SECTOR[id] ?? 0;
}

function terrainToSectorId(terrain: string): number {
  const sector = sectorForTerrain(terrain);
  for (const [k, v] of Object.entries(SECTORS)) {
    if (v === sector) return Number(k);
  }
  return 0;
}

// Depth fade keyed to BFS hop distance from the player. The current
// room renders at full alpha; each ring out drops a step until distant
// rooms barely peek above the bg. Unreachable rooms (no path through
// the visible cluster) get the dimmest treatment.
function depthAlphaFor(dist: number | undefined): number {
  if (dist === undefined) return 0.22;
  if (dist === 0) return 1;
  if (dist <= 2) return 0.9;
  if (dist <= 5) return 0.72;
  if (dist <= 9) return 0.55;
  if (dist <= 14) return 0.4;
  return 0.28;
}

function loadStyle(): Style {
  try {
    const value = localStorage.getItem(STYLE_KEY);
    return value === 'tileset' ? 'tileset' : 'squares';
  } catch {
    return 'squares';
  }
}

function loadTileset(): string | null {
  try {
    return localStorage.getItem(TILESET_KEY);
  } catch {
    return null;
  }
}

interface ContextMenu {
  x: number;
  y: number;
  room: MapRoom;
}

interface Layout {
  ox: number;
  oy: number;
  pitch: number;
  roomSize: number;
  currentZ: number;
}

function findRoomAtPoint(px: number, py: number, rooms: MapRoom[], layout: Layout): MapRoom | null {
  const half = layout.roomSize / 2;
  for (const room of rooms) {
    if (room.z !== layout.currentZ) continue;
    const cx = layout.ox + room.x * layout.pitch;
    const cy = layout.oy + room.y * layout.pitch;
    if (px >= cx - half && px <= cx + half && py >= cy - half && py <= cy + half) {
      return room;
    }
  }
  return null;
}

export function MappingMapView() {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [snapshot, setSnapshot] = useState<AreaSnapshot | null>(null);
  const [hoverInfo, setHoverInfo] = useState<string | null>(null);
  const [menu, setMenu] = useState<ContextMenu | null>(null);
  const [style, setStyle] = useState<Style>(loadStyle);
  const [tilesetUrl, setTilesetUrl] = useState<string | null>(loadTileset);
  const [tilesetImage, setTilesetImage] = useState<HTMLImageElement | null>(null);
  const lastRefreshRef = useRef<number>(0);
  // Latest layout computed by draw(); read by mouse handlers so hit-testing
  // matches the current pitch and origin even after a resize.
  const layoutRef = useRef<Layout | null>(null);

  useEffect(() => {
    try {
      localStorage.setItem(STYLE_KEY, style);
    } catch {
      // ignore
    }
  }, [style]);

  // Sync tileset URL with localStorage on focus so loading from the server
  // view also lights up the mapping view next time it renders.
  useEffect(() => {
    const sync = () => setTilesetUrl(loadTileset());
    sync();
    window.addEventListener('storage', sync);
    window.addEventListener('focus', sync);
    return () => {
      window.removeEventListener('storage', sync);
      window.removeEventListener('focus', sync);
    };
  }, []);

  useEffect(() => {
    if (!tilesetUrl) {
      setTilesetImage(null);
      return;
    }
    const img = new Image();
    img.onload = () => setTilesetImage(img);
    img.onerror = () => setTilesetImage(null);
    img.src = tilesetUrl;
  }, [tilesetUrl]);

  const refresh = useCallback(async () => {
    try {
      const snap = await getAreaSnapshot();
      setSnapshot(snap);
    } catch {
      setSnapshot(null);
    }
  }, []);

  useEffect(() => {
    let unsubMap: (() => void) | undefined;
    let unsubState: (() => void) | undefined;

    onMap(() => {
      const now = Date.now();
      if (now - lastRefreshRef.current < 80) return;
      lastRefreshRef.current = now;
      void refresh();
    }).then((fn) => {
      unsubMap = fn;
    });

    onState((payload) => {
      if (payload.kind === 'connected') {
        void refresh();
      } else if (payload.kind === 'disconnected') {
        setSnapshot(null);
      }
    }).then((fn) => {
      unsubState = fn;
    });

    void refresh();

    return () => {
      unsubMap?.();
      unsubState?.();
    };
  }, [refresh]);

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    if (!canvas || !container) return;

    const dpr = window.devicePixelRatio || 1;
    const cssWidth = container.clientWidth;
    const cssHeight = container.clientHeight;
    if (canvas.width !== cssWidth * dpr || canvas.height !== cssHeight * dpr) {
      canvas.width = Math.max(1, cssWidth * dpr);
      canvas.height = Math.max(1, cssHeight * dpr);
      canvas.style.width = `${cssWidth}px`;
      canvas.style.height = `${cssHeight}px`;
    }

    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.fillStyle = MAP_COLORS.bg;
    ctx.fillRect(0, 0, cssWidth, cssHeight);

    if (!snapshot) {
      ctx.fillStyle = '#6e7681';
      ctx.font = '12px monospace';
      ctx.fillText('connect and walk to populate', PADDING, PADDING + 12);
      return;
    }

    const current = snapshot.rooms.find((r) => r.id === snapshot.current_room_id);
    const currentZ = current?.z ?? 0;
    const visible = snapshot.rooms.filter((r) => r.z === currentZ);
    if (visible.length === 0) {
      ctx.fillStyle = '#6e7681';
      ctx.font = '12px monospace';
      ctx.fillText('no rooms on this floor yet', PADDING, PADDING + 12);
      return;
    }

    // Camera follows the player. The view re-centers on the player every
    // frame, but every cell renders at an integer pixel position so tiles
    // snap to the grid instead of sliding between half-pixel positions.
    // pitch is integer; the player's world coords are integer; ox/oy are
    // floored; every cell coord is therefore an exact multiple of pitch
    // away from a known integer origin.
    const pitch = ROOM_SPACING;
    const roomSize = Math.max(8, Math.floor(pitch * 0.65));
    const focus = current ?? visible[0];
    const ox = Math.floor(cssWidth / 2 - focus.x * pitch);
    const oy = Math.floor(cssHeight / 2 - focus.y * pitch);
    layoutRef.current = { ox, oy, pitch, roomSize, currentZ };

    // Bounding box of the visible rooms — used to position terrain
    // decorations. The view does NOT anchor on the bbox; that would
    // let new rooms shift every existing tile.
    let minX = Infinity;
    let maxX = -Infinity;
    let minY = Infinity;
    let maxY = -Infinity;
    for (const r of visible) {
      if (r.x < minX) minX = r.x;
      if (r.x > maxX) maxX = r.x;
      if (r.y < minY) minY = r.y;
      if (r.y > maxY) maxY = r.y;
    }

    // Compute the dominant sector for the terrain decorations painted
    // in the void below.
    const sectorCounts: Record<number, number> = {};
    for (const r of visible) {
      const sid = terrainToSectorId(r.terrain);
      sectorCounts[sid] = (sectorCounts[sid] ?? 0) + 1;
    }
    let dominantId = 0;
    let bestCount = 0;
    for (const [id, count] of Object.entries(sectorCounts)) {
      if (count > bestCount) {
        bestCount = count;
        dominantId = Number(id);
      }
    }
    const dominantHalo = SECTORS[dominantId]?.halo ?? SECTORS[0].halo;

    // Terrain decorations: trees in the forest, peaks in the mountains,
    // wavy lines over water. Painted between the bg and the corridors so
    // rooms always sit on top.
    {
      const bboxCx = ((minX + maxX) / 2) * pitch + ox;
      const bboxCy = ((minY + maxY) / 2) * pitch + oy;
      const halfW = ((maxX - minX) * pitch) / 2 + pitch * 0.5;
      const halfH = ((maxY - minY) * pitch) / 2 + pitch * 0.5;
      drawTerrainDecorations(
        ctx,
        [
          {
            cx: bboxCx,
            cy: bboxCy,
            hw: halfW,
            hh: halfH,
            sector: dominantId,
            haloColor: dominantHalo,
          },
        ],
        { cssWidth, cssHeight, pitch },
      );
    }

    // Corridors first, drawn under rooms. Single thin gray pass like the
    // web map; up/down exits skip this and show as small arrows on the
    // cell instead.
    const visibleIds = new Set(visible.map((r) => r.id));
    ctx.strokeStyle = MAP_COLORS.corridor;
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    const drawnEdges = new Set<string>();
    for (const exit of snapshot.exits) {
      if (!visibleIds.has(exit.from_room) || !visibleIds.has(exit.to_room)) continue;
      const lo = Math.min(exit.from_room, exit.to_room);
      const hi = Math.max(exit.from_room, exit.to_room);
      const key = `${lo}:${hi}`;
      if (drawnEdges.has(key)) continue;
      drawnEdges.add(key);
      const from = visible.find((r) => r.id === exit.from_room);
      const to = visible.find((r) => r.id === exit.to_room);
      if (!from || !to) continue;
      ctx.moveTo(ox + from.x * pitch, oy + from.y * pitch);
      ctx.lineTo(ox + to.x * pitch, oy + to.y * pitch);
    }
    ctx.stroke();

    // BFS hop distance from the player to every visible room. Drives a
    // depth fade so closer rooms read brightly and far rooms recede
    // toward the bg, the way TinTin++ shades distance.
    const distance = new Map<number, number>();
    if (snapshot.current_room_id !== null) {
      const adj = new Map<number, number[]>();
      for (const exit of snapshot.exits) {
        if (!visibleIds.has(exit.from_room) || !visibleIds.has(exit.to_room)) continue;
        const list = adj.get(exit.from_room);
        if (list) list.push(exit.to_room);
        else adj.set(exit.from_room, [exit.to_room]);
      }
      distance.set(snapshot.current_room_id, 0);
      const queue: number[] = [snapshot.current_room_id];
      while (queue.length > 0) {
        const cur = queue.shift()!;
        const d = distance.get(cur)!;
        for (const nb of adj.get(cur) ?? []) {
          if (!distance.has(nb)) {
            distance.set(nb, d + 1);
            queue.push(nb);
          }
        }
      }
    }

    // Rooms.
    const useTiles = style === 'tileset' && tilesetImage !== null;
    for (const room of visible) {
      const cx = ox + room.x * pitch;
      const cy = oy + room.y * pitch;
      const isCurrent = room.id === snapshot.current_room_id;
      const sector = sectorForTerrain(room.terrain);
      const depth = depthAlphaFor(distance.get(room.id));

      if (useTiles && tilesetImage) {
        const tileSize = tilesetImage.naturalHeight;
        const tilesInImage = Math.max(1, Math.floor(tilesetImage.naturalWidth / tileSize));
        const idx = Math.min(terrainToTileIndex(room.terrain), tilesInImage - 1);
        ctx.save();
        ctx.globalAlpha = depth;
        ctx.drawImage(
          tilesetImage,
          idx * tileSize,
          0,
          tileSize,
          tileSize,
          cx - pitch / 2,
          cy - pitch / 2,
          pitch,
          pitch,
        );
        ctx.restore();
      } else {
        ctx.save();
        if (isCurrent) {
          // Player cell: dim pink fill + bright pink border, full
          // alpha so it stays bright against depth-faded neighbors.
          ctx.fillStyle = MAP_COLORS.originFill;
          ctx.fillRect(cx - roomSize / 2, cy - roomSize / 2, roomSize, roomSize);
          ctx.strokeStyle = MAP_COLORS.origin;
          ctx.lineWidth = 1;
        } else {
          ctx.globalAlpha = depth;
          ctx.fillStyle = sector.fill;
          ctx.fillRect(cx - roomSize / 2, cy - roomSize / 2, roomSize, roomSize);
          if (room.avoid) {
            ctx.strokeStyle = MAP_COLORS.dest;
            ctx.lineWidth = 1.5;
          } else {
            ctx.strokeStyle = hexToRgba(sector.border, 0.8);
            ctx.lineWidth = 1;
          }
        }
        ctx.strokeRect(cx - roomSize / 2, cy - roomSize / 2, roomSize, roomSize);
        ctx.restore();
      }

      if (room.notes) {
        ctx.fillStyle = '#f1c232';
        ctx.beginPath();
        ctx.arc(cx + roomSize / 2 - 3, cy - roomSize / 2 + 3, 2.5, 0, Math.PI * 2);
        ctx.fill();
      }
    }
  }, [snapshot, style, tilesetImage]);

  useEffect(() => {
    draw();
    const handleResize = () => draw();
    window.addEventListener('resize', handleResize);
    if (!containerRef.current) {
      return () => window.removeEventListener('resize', handleResize);
    }
    const observer = new ResizeObserver(() => draw());
    observer.observe(containerRef.current);
    return () => {
      window.removeEventListener('resize', handleResize);
      observer.disconnect();
    };
  }, [draw]);

  const pickRoom = (event: MouseEvent<HTMLCanvasElement>): MapRoom | null => {
    if (!snapshot || !containerRef.current || !layoutRef.current) return null;
    const rect = containerRef.current.getBoundingClientRect();
    const px = event.clientX - rect.left;
    const py = event.clientY - rect.top;
    return findRoomAtPoint(px, py, snapshot.rooms, layoutRef.current);
  };

  const handleClick = (event: MouseEvent<HTMLCanvasElement>) => {
    setMenu(null);
    const room = pickRoom(event);
    if (!room) return;
    if (room.id === snapshot?.current_room_id) return;
    void walkToRoom(room.id).catch(() => {});
  };

  const handleContextMenu = (event: MouseEvent<HTMLCanvasElement>) => {
    event.preventDefault();
    if (!containerRef.current) return;
    const rect = containerRef.current.getBoundingClientRect();
    const room = pickRoom(event);
    if (!room) {
      setMenu(null);
      return;
    }
    setMenu({ x: event.clientX - rect.left, y: event.clientY - rect.top, room });
  };

  const handleMouseMove = (event: MouseEvent<HTMLCanvasElement>) => {
    const room = pickRoom(event);
    if (room) {
      const note = room.notes ? ` — ${room.notes}` : '';
      setHoverInfo(`#${room.id} ${room.name}${note}`);
    } else {
      setHoverInfo(null);
    }
  };

  const handleEditNote = async () => {
    if (!menu) return;
    const next = window.prompt('Note for this room', menu.room.notes);
    setMenu(null);
    if (next === null) return;
    try {
      await setRoomNote(menu.room.id, next);
      void refresh();
    } catch {
      // ignore
    }
  };

  const handleToggleAvoid = async () => {
    if (!menu) return;
    const target = !menu.room.avoid;
    setMenu(null);
    try {
      await setRoomAvoid(menu.room.id, target);
      void refresh();
    } catch {
      // ignore
    }
  };

  return (
    <div className="mapping-view">
      <div className="map-subhead">
        <span>
          {snapshot?.area ? `${snapshot.area}` : 'unknown area'}
          {snapshot ? ` · ${snapshot.rooms.length} room(s)` : ''}
        </span>
        <div className="map-mode-toggle">
          <button
            type="button"
            aria-pressed={style === 'squares'}
            onClick={() => setStyle('squares')}
          >
            squares
          </button>
          <button
            type="button"
            aria-pressed={style === 'tileset'}
            onClick={() => setStyle('tileset')}
          >
            tileset
          </button>
        </div>
      </div>
      <div ref={containerRef} className="map-canvas-host" onMouseLeave={() => setHoverInfo(null)}>
        <canvas
          ref={canvasRef}
          onClick={handleClick}
          onContextMenu={handleContextMenu}
          onMouseMove={handleMouseMove}
        />
        {hoverInfo && <div className="map-hover">{hoverInfo}</div>}
        {menu && (
          <div className="map-menu" style={{ left: menu.x, top: menu.y }}>
            <div className="map-menu-title">
              #{menu.room.id} {menu.room.name}
            </div>
            <button type="button" onClick={handleEditNote}>
              edit note
            </button>
            <button type="button" onClick={handleToggleAvoid}>
              {menu.room.avoid ? 'unmark avoid' : 'mark avoid'}
            </button>
            <button
              type="button"
              onClick={() => {
                const id = menu.room.id;
                setMenu(null);
                void walkToRoom(id).catch(() => {});
              }}
            >
              walk here
            </button>
            <button type="button" onClick={() => setMenu(null)}>
              cancel
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
