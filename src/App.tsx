import { useEffect, useRef, useState, type MouseEvent } from 'react';
import { Terminal, type TerminalHandle } from './components/Terminal';
import { Input, type InputHandle } from './components/Input';
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
  const inputRef = useRef<InputHandle | null>(null);

  // Click anywhere in the middle area focuses the input. Skip if the user
  // is in the middle of selecting text (so they can still copy from the
  // terminal pane), if they clicked an actual interactive element (button,
  // input, textarea), or if the click landed inside the triggers drawer.
  const handleMiddleMouseDown = (event: MouseEvent<HTMLDivElement>) => {
    const target = event.target as HTMLElement;
    if (target.closest('button, input, textarea, select, .drawer')) return;
    const selection = window.getSelection?.();
    if (selection && selection.toString().length > 0) return;
    inputRef.current?.focus();
  };

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
      <div className="middle" onMouseUp={handleMiddleMouseDown}>
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
      <Input ref={inputRef} enabled onError={handleError} />
    </main>
  );
}

export default App;
