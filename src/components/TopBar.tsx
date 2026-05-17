import { getCurrentWindow } from '@tauri-apps/api/window';

// Frameless-window top strip. Drag region across most of its width
// with a discrete cluster of text-style window controls on the right.
// Cross-platform substitute for native traffic lights.
export function TopBar() {
  const win = () => getCurrentWindow();

  return (
    <div className="topbar" data-tauri-drag-region>
      <span className="brand-block" data-tauri-drag-region>
        [vosh]
      </span>
      <span className="topbar-spacer" data-tauri-drag-region />
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
