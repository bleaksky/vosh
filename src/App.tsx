import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

function App() {
  const [version, setVersion] = useState<string>('loading');

  useEffect(() => {
    invoke<string>('app_version')
      .then(setVersion)
      .catch(() => setVersion('unknown'));
  }, []);

  return (
    <main className="app">
      <h1>mudclient</h1>
      <p>Phase 0 scaffold. Backend version {version}.</p>
      <p>Phase 1 lands telnet and ANSI rendering.</p>
    </main>
  );
}

export default App;
