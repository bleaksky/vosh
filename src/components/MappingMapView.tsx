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

// Maximum spacing in pixels per cell. The renderer shrinks below this to
// keep the area's bounding box inside the canvas.
const ROOM_SPACING = 28;
const PADDING = 10;
const COLOR_BG = '#0d1117';
const COLOR_GRID = '#1c2229';
const COLOR_EDGE = '#3a444f';
const COLOR_ROOM = '#21262d';
const COLOR_ROOM_BORDER = '#444c56';
const COLOR_ROOM_AVOID = '#572323';
const COLOR_CURRENT = '#1f6feb';
const COLOR_TEXT = '#c9d1d9';

type Style = 'squares' | 'tileset';

const STYLE_KEY = 'mudclient.layout.mappingMapStyle';
// Same key as ServerMapView so loading a tileset in either mode makes it
// available in both.
const TILESET_KEY = 'mudclient.layout.serverMapTileset';

const SECTOR_ORDER: string[] = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c'];

// Map a free-form terrain string from Room.Info onto the same sector code
// table used by the server view. Lets a single tileset PNG drive both.
function terrainToSectorIndex(terrain: string): number {
  const t = terrain.toLowerCase();
  if (!t) return 0;
  if (t.includes('inside') || t.includes('road') || t.includes('indoor')) return 0;
  if (t.includes('city') || t.includes('street')) return 1;
  if (t.includes('pasture')) return SECTOR_ORDER.indexOf('a');
  if (t.includes('field') || t.includes('grass')) return 2;
  if (t.includes('forest') || t.includes('wood')) return 3;
  if (t.includes('hill')) return 4;
  if (t.includes('mountain')) return 5;
  if (t.includes('underwater')) return SECTOR_ORDER.indexOf('c');
  if (t.includes('water')) return t.includes('noswim') ? 7 : 6;
  if (t.includes('air')) return 8;
  if (t.includes('desert') || t.includes('sand')) return 9;
  if (t.includes('ice') || t.includes('arctic') || t.includes('tundra')) {
    return SECTOR_ORDER.indexOf('b');
  }
  return 0;
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

function findRoomAtPoint(
  px: number,
  py: number,
  rooms: MapRoom[],
  layout: Layout,
): MapRoom | null {
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
    ctx.fillStyle = COLOR_BG;
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

    // Anchor on the bounding box of every room on this floor so a tile
    // keeps its on-screen position when the player walks. Only changes to
    // the floor or to the discovered area (a new room outside the current
    // bbox) shift the layout. The pitch shrinks to fit when the bbox grows
    // past the canvas, but stays within sane bounds for small areas.
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
    const bboxW = maxX - minX + 1;
    const bboxH = maxY - minY + 1;
    const fitX = bboxW > 0 ? cssWidth / (bboxW + 1) : ROOM_SPACING;
    const fitY = bboxH > 0 ? cssHeight / (bboxH + 1) : ROOM_SPACING;
    const pitch = Math.max(12, Math.min(ROOM_SPACING, Math.floor(Math.min(fitX, fitY))));
    const roomSize = Math.max(8, Math.floor(pitch * 0.65));
    const centerWorldX = (minX + maxX) / 2;
    const centerWorldY = (minY + maxY) / 2;
    const ox = cssWidth / 2 - centerWorldX * pitch;
    const oy = cssHeight / 2 - centerWorldY * pitch;
    layoutRef.current = { ox, oy, pitch, roomSize, currentZ };

    // Faint grid for orientation.
    ctx.strokeStyle = COLOR_GRID;
    ctx.lineWidth = 1;
    const gridStartX = ox - Math.ceil(ox / pitch) * pitch;
    const gridStartY = oy - Math.ceil(oy / pitch) * pitch;
    for (let x = gridStartX; x < cssWidth; x += pitch) {
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, cssHeight);
      ctx.stroke();
    }
    for (let y = gridStartY; y < cssHeight; y += pitch) {
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(cssWidth, y);
      ctx.stroke();
    }

    // Edges.
    const visibleIds = new Set(visible.map((r) => r.id));
    ctx.strokeStyle = COLOR_EDGE;
    ctx.lineWidth = 1.5;
    for (const exit of snapshot.exits) {
      if (!visibleIds.has(exit.from_room) || !visibleIds.has(exit.to_room)) continue;
      const from = visible.find((r) => r.id === exit.from_room);
      const to = visible.find((r) => r.id === exit.to_room);
      if (!from || !to) continue;
      ctx.beginPath();
      ctx.moveTo(ox + from.x * pitch, oy + from.y * pitch);
      ctx.lineTo(ox + to.x * pitch, oy + to.y * pitch);
      ctx.stroke();
    }

    // Rooms.
    const useTiles = style === 'tileset' && tilesetImage !== null;
    for (const room of visible) {
      const cx = ox + room.x * pitch;
      const cy = oy + room.y * pitch;
      const isCurrent = room.id === snapshot.current_room_id;

      if (useTiles && tilesetImage) {
        const tileSize = tilesetImage.naturalHeight;
        const tilesInImage = Math.max(1, Math.floor(tilesetImage.naturalWidth / tileSize));
        const idx = Math.min(terrainToSectorIndex(room.terrain), tilesInImage - 1);
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
        if (isCurrent) {
          ctx.strokeStyle = COLOR_TEXT;
          ctx.lineWidth = 2;
          ctx.beginPath();
          ctx.rect(cx - pitch / 2, cy - pitch / 2, pitch, pitch);
          ctx.stroke();
        }
        if (room.avoid) {
          ctx.strokeStyle = '#f85149';
          ctx.lineWidth = 2;
          ctx.beginPath();
          ctx.moveTo(cx - pitch / 2, cy - pitch / 2);
          ctx.lineTo(cx + pitch / 2, cy + pitch / 2);
          ctx.moveTo(cx + pitch / 2, cy - pitch / 2);
          ctx.lineTo(cx - pitch / 2, cy + pitch / 2);
          ctx.stroke();
        }
      } else {
        ctx.fillStyle = isCurrent ? COLOR_CURRENT : room.avoid ? COLOR_ROOM_AVOID : COLOR_ROOM;
        ctx.strokeStyle = isCurrent ? COLOR_TEXT : COLOR_ROOM_BORDER;
        ctx.lineWidth = isCurrent ? 2 : 1;
        ctx.beginPath();
        ctx.rect(cx - roomSize / 2, cy - roomSize / 2, roomSize, roomSize);
        ctx.fill();
        ctx.stroke();
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
