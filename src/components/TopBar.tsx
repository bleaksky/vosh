import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

interface Props {
  // When true, the auxiliary chrome buttons (settings/map) render.
  // Set false on auxiliary windows so they only show window controls.
  showAuxButtons?: boolean;
  // Label rendered inside the brand block. Defaults to "[vosh]".
  brand?: string;
}

// Frameless-window top strip. Drag region across most of its width
// with text-style chrome buttons on the right (settings, map, then
// the window controls). Cross-platform substitute for native
// traffic lights.
export function TopBar({ showAuxButtons = true, brand = '[vosh]' }: Props) {
  const win = () => getCurrentWindow();

  const openSettings = () => {
    invoke('open_settings_window').catch((e) => {
      console.error('[topbar] open_settings_window failed', e);
    });
  };

  return (
    <div className="topbar" data-tauri-drag-region>
      <span className="brand-block" data-tauri-drag-region>
        {brand}
      </span>
      <span className="topbar-spacer" data-tauri-drag-region />
      {showAuxButtons && (
        <div className="topbar-aux">
          <button
            type="button"
            className="topbar-aux-btn"
            onClick={openSettings}
          >
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
          aria-label="maximize"
          onClick={() => void win().toggleMaximize()}
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
