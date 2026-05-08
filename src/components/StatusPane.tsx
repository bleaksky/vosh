import { useEffect, useState } from 'react';
import { onGmcp, onState } from '../lib/session';

interface RoomInfo {
  name?: string;
  area?: string;
  id?: number | string;
}

interface CharStatus {
  name?: string;
  fullname?: string;
  level?: number | string;
  class?: string;
  race?: string;
  alignment?: string | number;
  // Aabahran sometimes nests fields differently across Char.* packages.
  // Allow extra keys so any Char.* push enriches the displayed line.
  [key: string]: unknown;
}

export function StatusPane() {
  const [room, setRoom] = useState<RoomInfo>({});
  const [char, setChar] = useState<CharStatus>({});

  useEffect(() => {
    let unsubGmcp: (() => void) | undefined;
    let unsubState: (() => void) | undefined;

    onGmcp((payload) => {
      if (payload.package === 'Room.Info') {
        setRoom(payload.data ?? {});
        return;
      }
      // Any Char.* push enriches the identity snapshot.
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

  const charLine = char.fullname || char.name || '-';

  return (
    <section className="status-pane" aria-label="status">
      <header className="pane-header">status</header>
      <div className="status-body">
        <div className="status-row">
          <span className="status-label">char</span>
          <span className="status-value">{charLine}</span>
        </div>
        {char.class && (
          <div className="status-row">
            <span className="status-label">class</span>
            <span className="status-value">{char.class}</span>
          </div>
        )}
        {char.race && (
          <div className="status-row">
            <span className="status-label">race</span>
            <span className="status-value">{char.race}</span>
          </div>
        )}
        {char.level !== undefined && (
          <div className="status-row">
            <span className="status-label">level</span>
            <span className="status-value">{String(char.level)}</span>
          </div>
        )}
        {char.alignment !== undefined && (
          <div className="status-row">
            <span className="status-label">align</span>
            <span className="status-value">{String(char.alignment)}</span>
          </div>
        )}
        <div className="status-row">
          <span className="status-label">room</span>
          <span className="status-value">{room.name ?? '-'}</span>
        </div>
        <div className="status-row">
          <span className="status-label">area</span>
          <span className="status-value">{room.area ?? '-'}</span>
        </div>
      </div>
    </section>
  );
}
