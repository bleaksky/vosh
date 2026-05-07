import { useEffect, useRef, useState } from 'react';
import { Terminal, type TerminalHandle } from './components/Terminal';
import { Input } from './components/Input';
import { Connect, type ConnectionStatus } from './components/Connect';
import { TriggersDrawer } from './components/TriggersDrawer';
import { SidePanel } from './components/SidePanel';
import { onState, type StatePayload } from './lib/session';

const SIDE_PANEL_STORAGE_KEY = 'mudclient.layout.sidePanelOpen';

function loadSidePanelOpen(): boolean {
  try {
    const value = localStorage.getItem(SIDE_PANEL_STORAGE_KEY);
    if (value === null) return true;
    return value === '1';
  } catch {
    return true;
  }
}

function App() {
  const [status, setStatus] = useState<ConnectionStatus>({ kind: 'idle' });
  const [triggersOpen, setTriggersOpen] = useState(false);
  const [sidePanelOpen, setSidePanelOpen] = useState(loadSidePanelOpen);
  const termRef = useRef<TerminalHandle | null>(null);

  useEffect(() => {
    let unsub: (() => void) | undefined;
    onState((payload: StatePayload) => {
      if (payload.kind === 'disconnected') {
        setStatus({ kind: 'idle' });
        if (payload.reason && termRef.current) {
          termRef.current.write(`\r\n\x1b[31m[${payload.reason}]\x1b[0m\r\n`);
        }
      } else {
        setStatus(payload);
      }
    }).then((fn) => {
      unsub = fn;
    });
    return () => {
      unsub?.();
    };
  }, []);

  useEffect(() => {
    try {
      localStorage.setItem(SIDE_PANEL_STORAGE_KEY, sidePanelOpen ? '1' : '0');
    } catch {
      // ignore storage failures (private mode, quota)
    }
  }, [sidePanelOpen]);

  const handleError = (message: string) => {
    setStatus({ kind: 'error', message });
    termRef.current?.write(`\r\n\x1b[31m[${message}]\x1b[0m\r\n`);
  };

  return (
    <main className="app">
      <Connect
        status={status}
        onError={handleError}
        onToggleTriggers={() => setTriggersOpen((v) => !v)}
        triggersOpen={triggersOpen}
        onToggleSidePanel={() => setSidePanelOpen((v) => !v)}
        sidePanelOpen={sidePanelOpen}
      />
      <div className="middle">
        <Terminal
          onReady={(handle) => {
            termRef.current = handle;
          }}
        />
        {sidePanelOpen && <SidePanel />}
        <TriggersDrawer
          open={triggersOpen}
          onClose={() => setTriggersOpen(false)}
          onError={handleError}
        />
      </div>
      <Input enabled onError={handleError} />
    </main>
  );
}

export default App;
