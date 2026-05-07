import { useEffect, useState } from 'react';
import { MappingMapView } from './MappingMapView';
import { ServerMapView } from './ServerMapView';

type Mode = 'mapping' | 'server';

const STORAGE_KEY = 'mudclient.layout.mapMode';

function loadMode(): Mode {
  try {
    const value = localStorage.getItem(STORAGE_KEY);
    return value === 'server' ? 'server' : 'mapping';
  } catch {
    return 'mapping';
  }
}

export function MapPane() {
  const [mode, setMode] = useState<Mode>(loadMode);

  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, mode);
    } catch {
      // ignore
    }
  }, [mode]);

  return (
    <section className="map-pane" aria-label="map">
      <header className="pane-header">
        <span>map</span>
        <div className="map-mode-toggle">
          <button
            type="button"
            aria-pressed={mode === 'mapping'}
            onClick={() => setMode('mapping')}
          >
            mapping
          </button>
          <button
            type="button"
            aria-pressed={mode === 'server'}
            onClick={() => setMode('server')}
          >
            server
          </button>
        </div>
      </header>
      {mode === 'mapping' ? <MappingMapView /> : <ServerMapView />}
    </section>
  );
}
