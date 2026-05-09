import { useEffect, useRef, useState } from 'react';
import { onGmcp, onState } from '../lib/session';
import { useDockTarget } from '../lib/docking';

interface RoomInfo {
  name?: string;
  area?: string;
  id?: number | string;
}

interface CharStatus {
  level?: number | string;
  // Aabahran nests fields differently across Char.* packages. Allow
  // extra keys so any push enriches the snapshot.
  [key: string]: unknown;
}

export function StatusPane() {
  const [room, setRoom] = useState<RoomInfo>({});
  const [char, setChar] = useState<CharStatus>({});
  const dockRef = useRef<HTMLElement>(null);
  useDockTarget(dockRef, 'status-pane');

  useEffect(() => {
    let unsubGmcp: (() => void) | undefined;
    let unsubState: (() => void) | undefined;

    onGmcp((payload) => {
      if (payload.package === 'Room.Info') {
        setRoom(payload.data ?? {});
        return;
      }
      if (
        payload.package.startsWith('Char.') &&
        payload.data &&
        typeof payload.data === 'object'
      ) {
        setChar((prev) => ({ ...prev, ...(payload.data as Record<string, unknown>) }));
      }
    }).then((fn) => {
      unsubGmcp = fn;
    });

    onState((payload) => {
      if (payload.kind === 'disconnected') {
        setRoom({});
        setChar({});
      }
    }).then((fn) => {
      unsubState = fn;
    });

    return () => {
      unsubGmcp?.();
      unsubState?.();
    };
  }, []);

  const level =
    typeof char.level === 'number'
      ? char.level
      : char.level !== undefined && char.level !== ''
        ? Number(char.level)
        : undefined;
  // Hide level once you hit the cap; pre-cap players use it for at-a-
  // glance "where am I in the curve" reads.
  const showLevel = level !== undefined && Number.isFinite(level) && level < 50;

  return (
    <section ref={dockRef} className="status-pane" aria-label="status">
      <header className="pane-header">status</header>
      <div className="status-body">
        <div className="status-row">
          <span className="status-label">room</span>
          <span className="status-value">{room.name ?? '-'}</span>
        </div>
        <div className="status-row">
          <span className="status-label">area</span>
          <span className="status-value">{room.area ?? '-'}</span>
        </div>
        {showLevel && (
          <div className="status-row">
            <span className="status-label">level</span>
            <span className="status-value">{level}</span>
          </div>
        )}
      </div>
    </section>
  );
}
