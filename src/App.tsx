import { useEffect, useRef, useState } from 'react';
import { Terminal, type TerminalHandle } from './components/Terminal';
import { Input } from './components/Input';
import { Connect, type ConnectionStatus } from './components/Connect';
import { onState, type StatePayload } from './lib/session';

function App() {
  const [status, setStatus] = useState<ConnectionStatus>({ kind: 'idle' });
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

  const handleError = (message: string) => {
    setStatus({ kind: 'error', message });
    termRef.current?.write(`\r\n\x1b[31m[${message}]\x1b[0m\r\n`);
  };

  const inputEnabled = status.kind === 'connected';

  return (
    <main className="app">
      <Connect status={status} onError={handleError} />
      <Terminal
        onReady={(handle) => {
          termRef.current = handle;
        }}
      />
      <Input enabled={inputEnabled} onError={handleError} />
    </main>
  );
}

export default App;
