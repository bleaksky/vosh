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

const ROOM_SIZE = 18;
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

interface ContextMenu {
  x: number;
  y: number;
  room: MapRoom;
}

function findRoomAtPoint(
  px: number,
  py: number,
  rooms: MapRoom[],
  origin: { ox: number; oy: number },
  currentZ: number,
): MapRoom | null {
  for (const room of rooms) {
    if (room.z !== currentZ) continue;
    const cx = origin.ox + room.x * ROOM_SPACING;
    const cy = origin.oy + room.y * ROOM_SPACING;
    if (
      px >= cx - ROOM_SIZE / 2 &&
      px <= cx + ROOM_SIZE / 2 &&
      py >= cy - ROOM_SIZE / 2 &&
      py <= cy + ROOM_SIZE / 2
    ) {
      return room;
    }
  }
  return null;
}

export function MapPane() {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [snapshot, setSnapshot] = useState<AreaSnapshot | null>(null);
  const [hoverInfo, setHoverInfo] = useState<string | null>(null);
  const [menu, setMenu] = useState<ContextMenu | null>(null);
  const lastRefreshRef = useRef<number>(0);

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

    const ox = current ? cssWidth / 2 - current.x * ROOM_SPACING : cssWidth / 2;
    const oy = current ? cssHeight / 2 - current.y * ROOM_SPACING : cssHeight / 2;

    // Faint grid for orientation.
    ctx.strokeStyle = COLOR_GRID;
    ctx.lineWidth = 1;
    for (let x = ox % ROOM_SPACING; x < cssWidth; x += ROOM_SPACING) {
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, cssHeight);
      ctx.stroke();
    }
    for (let y = oy % ROOM_SPACING; y < cssHeight; y += ROOM_SPACING) {
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
      ctx.moveTo(ox + from.x * ROOM_SPACING, oy + from.y * ROOM_SPACING);
      ctx.lineTo(ox + to.x * ROOM_SPACING, oy + to.y * ROOM_SPACING);
      ctx.stroke();
    }

    // Rooms.
    for (const room of visible) {
      const cx = ox + room.x * ROOM_SPACING;
      const cy = oy + room.y * ROOM_SPACING;
      const isCurrent = room.id === snapshot.current_room_id;
      ctx.fillStyle = isCurrent ? COLOR_CURRENT : room.avoid ? COLOR_ROOM_AVOID : COLOR_ROOM;
      ctx.strokeStyle = isCurrent ? COLOR_TEXT : COLOR_ROOM_BORDER;
      ctx.lineWidth = isCurrent ? 2 : 1;
      ctx.beginPath();
      ctx.rect(cx - ROOM_SIZE / 2, cy - ROOM_SIZE / 2, ROOM_SIZE, ROOM_SIZE);
      ctx.fill();
      ctx.stroke();

      if (room.notes) {
        ctx.fillStyle = '#f1c232';
        ctx.beginPath();
        ctx.arc(cx + ROOM_SIZE / 2 - 3, cy - ROOM_SIZE / 2 + 3, 2.5, 0, Math.PI * 2);
        ctx.fill();
      }
    }
  }, [snapshot]);

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

  const computeOrigin = (current: MapRoom | null, w: number, h: number) => ({
    ox: current ? w / 2 - current.x * ROOM_SPACING : w / 2,
    oy: current ? h / 2 - current.y * ROOM_SPACING : h / 2,
  });

  const handleClick = (event: MouseEvent<HTMLCanvasElement>) => {
    if (!snapshot || !containerRef.current) return;
    setMenu(null);
    const rect = containerRef.current.getBoundingClientRect();
    const px = event.clientX - rect.left;
    const py = event.clientY - rect.top;
    const current = snapshot.rooms.find((r) => r.id === snapshot.current_room_id) ?? null;
    const origin = computeOrigin(current, rect.width, rect.height);
    const currentZ = current?.z ?? 0;
    const room = findRoomAtPoint(px, py, snapshot.rooms, origin, currentZ);
    if (!room) return;
    if (room.id === snapshot.current_room_id) return;
    void walkToRoom(room.id).catch(() => {});
  };

  const handleContextMenu = (event: MouseEvent<HTMLCanvasElement>) => {
    event.preventDefault();
    if (!snapshot || !containerRef.current) return;
    const rect = containerRef.current.getBoundingClientRect();
    const px = event.clientX - rect.left;
    const py = event.clientY - rect.top;
    const current = snapshot.rooms.find((r) => r.id === snapshot.current_room_id) ?? null;
    const origin = computeOrigin(current, rect.width, rect.height);
    const currentZ = current?.z ?? 0;
    const room = findRoomAtPoint(px, py, snapshot.rooms, origin, currentZ);
    if (!room) {
      setMenu(null);
      return;
    }
    setMenu({ x: px, y: py, room });
  };

  const handleMouseMove = (event: MouseEvent<HTMLCanvasElement>) => {
    if (!snapshot || !containerRef.current) return;
    const rect = containerRef.current.getBoundingClientRect();
    const px = event.clientX - rect.left;
    const py = event.clientY - rect.top;
    const current = snapshot.rooms.find((r) => r.id === snapshot.current_room_id) ?? null;
    const origin = computeOrigin(current, rect.width, rect.height);
    const currentZ = current?.z ?? 0;
    const room = findRoomAtPoint(px, py, snapshot.rooms, origin, currentZ);
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
    <section className="map-pane" aria-label="map">
      <header className="pane-header">
        map {snapshot?.area ? `· ${snapshot.area}` : ''}
        {snapshot ? ` · ${snapshot.rooms.length}` : ''}
      </header>
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
    </section>
  );
}
