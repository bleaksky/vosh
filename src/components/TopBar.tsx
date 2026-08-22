import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { ReactNode } from 'react';
import { TopBarLoadouts } from './TopBarLoadouts';
import { Connect, type ConnectionStatus } from './Connect';

// Platform sniff. The square button means "make the window as big as
// possible." On macOS that idiom maps to native full-screen mode (the
// green traffic-light button since 10.10), not the toggle-maximize
// call that just enlarges within the desktop. We flip behavior here
// so the button does what the user expects on each OS.
const IS_MAC = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/i.test(navigator.platform);

async function handleMaximize() {
  const w = getCurrentWindow();
  if (IS_MAC) {
    const fs = await w.isFullscreen();
    await w.setFullscreen(!fs);
  } else {
    await w.toggleMaximize();
  }
}

interface Props {
  // When true, the auxiliary chrome buttons (settings / map) render.
  // Set false on auxiliary windows so they only show window controls.
  showAuxButtons?: boolean;
  // Label rendered inside the brand block. Defaults to "[vosh]".
  brand?: string;
  // Map pane toggle. When provided, a `map` button renders and reflects
  // pressed state.
  mapOpen?: boolean;
  onToggleMap?: () => void;
  // Help modal opener. When provided, a `help` button renders.
  onOpenHelp?: () => void;
  // Session chip. When provided, the connect chip renders next to the
  // brand block and the old connect row disappears.
  connectionStatus?: ConnectionStatus;
  onConnectionError?: (message: string) => void;
  // Extra chrome rendered just before the window controls. The
  // settings window mounts its search field here.
  titleExtra?: ReactNode;
}

// Frameless-window top strip. Drag region across most of its width
// with text-style chrome buttons on the right (settings, map, then
// the window controls). Cross-platform substitute for native
// traffic lights.
export function TopBar({
  showAuxButtons = true,
  brand = 'Vosh',
  mapOpen,
  onToggleMap,
  onOpenHelp,
  connectionStatus,
  onConnectionError,
  titleExtra,
}: Props) {
  const win = () => getCurrentWindow();

  const openSettings = () => {
    invoke('open_settings_window').catch((e) => {
      console.error('[topbar] open_settings_window failed', e);
    });
  };

  return (
    <div className="topbar" data-tauri-drag-region>
      <span className="brand-block" data-tauri-drag-region title={`Vosh ${__APP_VERSION__}`}>
        {brand}
      </span>
      {connectionStatus && onConnectionError && (
        <Connect status={connectionStatus} onError={onConnectionError} />
      )}
      <span className="topbar-spacer" data-tauri-drag-region />
      {showAuxButtons && (
        <div className="topbar-aux">
          <TopBarLoadouts />
          {onToggleMap && (
            <button
              type="button"
              className={`topbar-aux-btn${mapOpen ? ' topbar-aux-btn-pressed' : ''}`}
              aria-pressed={mapOpen}
              aria-label="toggle map pane"
              title="map"
              onClick={onToggleMap}
            >
              <svg width="16" height="16" viewBox="0 0 16 16" fill="none" strokeWidth="1.3">
                <path d="M2 4.5 6 3l4 1.5L14 3v8.5L10 13 6 11.5 2 13z" />
                <path d="M6 3v8.5M10 4.5V13" />
              </svg>
            </button>
          )}
          {onOpenHelp && (
            <button
              type="button"
              className="topbar-aux-btn"
              aria-label="open help"
              title="help"
              onClick={onOpenHelp}
            >
              <svg width="16" height="16" viewBox="0 0 16 16" fill="none" strokeWidth="1.3">
                <circle cx="8" cy="8" r="6.2" />
                <path d="M6.2 6.2a1.8 1.8 0 1 1 2.6 1.7c-.6.3-.8.7-.8 1.3" />
                <path d="M8 11.3h.01" />
              </svg>
            </button>
          )}
          <button
            type="button"
            className="topbar-aux-btn"
            aria-label="open settings"
            title="settings"
            onClick={openSettings}
          >
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none" strokeWidth="1.3">
              <circle cx="8" cy="8" r="2.4" />
              <path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M12.6 3.4l-1.4 1.4M4.8 11.2l-1.4 1.4" />
            </svg>
          </button>
        </div>
      )}
      {titleExtra}
      <div className="topbar-controls">
        <button
          type="button"
          className="topbar-btn"
          aria-label="minimize"
          onClick={() => void win().minimize()}
        >
          <svg width="10" height="10" viewBox="0 0 10 10" strokeWidth="1.2">
            <path d="M1 5h8" />
          </svg>
        </button>
        <button
          type="button"
          className="topbar-btn"
          aria-label={IS_MAC ? 'enter full screen' : 'maximize'}
          onClick={() => void handleMaximize()}
        >
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none" strokeWidth="1.2">
            <rect x="1.5" y="1.5" width="7" height="7" rx="1" />
          </svg>
        </button>
        <button
          type="button"
          className="topbar-btn topbar-btn-close"
          aria-label="close"
          onClick={() => void win().close()}
        >
          <svg width="10" height="10" viewBox="0 0 10 10" strokeWidth="1.2">
            <path d="M1.5 1.5l7 7M8.5 1.5l-7 7" />
          </svg>
        </button>
      </div>
    </div>
  );
}
