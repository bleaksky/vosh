import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { TopBarLoadouts } from './TopBarLoadouts';

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
}

// Frameless-window top strip. Drag region across most of its width
// with text-style chrome buttons on the right (settings, map, then
// the window controls). Cross-platform substitute for native
// traffic lights.
export function TopBar({
  showAuxButtons = true,
  brand = '[vosh]',
  mapOpen,
  onToggleMap,
  onOpenHelp,
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
      <span className="topbar-spacer" data-tauri-drag-region />
      {showAuxButtons && (
        <div className="topbar-aux">
          <TopBarLoadouts />
          {onToggleMap && (
            <button
              type="button"
              className={`topbar-aux-btn${mapOpen ? ' topbar-aux-btn-pressed' : ''}`}
              aria-pressed={mapOpen}
              onClick={onToggleMap}
            >
              map
            </button>
          )}
          {onOpenHelp && (
            <button type="button" className="topbar-aux-btn" onClick={onOpenHelp}>
              help
            </button>
          )}
          <button type="button" className="topbar-aux-btn" onClick={openSettings}>
            settings
          </button>
        </div>
      )}
      <div className="topbar-controls">
        <button
          type="button"
          className="topbar-btn"
          aria-label="minimize"
          onClick={() => void win().minimize()}
        >
          –
        </button>
        <button
          type="button"
          className="topbar-btn"
          aria-label={IS_MAC ? 'enter full screen' : 'maximize'}
          onClick={() => void handleMaximize()}
        >
          ▢
        </button>
        <button
          type="button"
          className="topbar-btn topbar-btn-close"
          aria-label="close"
          onClick={() => void win().close()}
        >
          ✕
        </button>
      </div>
    </div>
  );
}
