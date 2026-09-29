import { useEffect, useRef, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import APP_SHORTCUTS from '../../lib/appShortcuts.json';
import { isMacPlatform, shortcutLabel } from '../../lib/palette';
import type { PaneSplit, PaneType } from '../../lib/paneLayout';
import { PANE_LABELS, paneTypesToAdd } from '../panel/paneTypes';
import type { Connection } from '../../lib/useConnection';
import { CloseIcon, MaximizeIcon, MinimizeIcon, PanelIcon, PlusIcon, SearchIcon } from './icons';
import { SessionMenu } from './SessionMenu';
import { ShellMenu, ShellMenuItem } from './ShellMenu';
import { TitleButton } from './TitleButton';

// The 32 px title band across the top of the window (SPEC 1 and 9). No
// fill and no line of its own: the terminal ground runs up under it and
// the panel ground runs up on the right. Its empty areas drag the
// window. The session button sits centered over the terminal column.
// Add a pane, Search commands, and the panel toggle sit at the right,
// over the panel. On macOS the native traffic lights own the left
// corner. Windows and Linux draw minimize, maximize, and close here.

const ADD_MENU_WIDTH = 200;

interface Props {
  connection: Connection;
  panelOpen: boolean;
  onTogglePanel: () => void;
  /** Open the palette, or close it when it is open. */
  onTogglePalette: () => void;
  /** The panel's pane tree. Add a pane lists the pane types it does
   *  not show yet. */
  paneTree: PaneSplit | null;
  onAddPane: (pane: PaneType) => void;
  /** Runs after a menu closes, to hand the caret back to the command
   *  line. */
  onMenuClosed: () => void;
}

export function TitleBand({
  connection,
  panelOpen,
  onTogglePanel,
  onTogglePalette,
  paneTree,
  onAddPane,
  onMenuClosed,
}: Props) {
  const mac = isMacPlatform();
  const [menu, setMenu] = useState<'session' | 'add' | null>(null);
  const sessionRef = useRef<HTMLButtonElement | null>(null);
  const addRef = useRef<HTMLButtonElement | null>(null);
  const closeMenu = () => {
    setMenu(null);
    onMenuClosed();
  };
  const toggleMenu = (which: 'session' | 'add') =>
    setMenu((current) => (current === which ? null : which));
  // Hiding the panel takes Add a pane with it, so its menu closes too
  // instead of coming back the next time the panel shows.
  useEffect(() => {
    if (!panelOpen) setMenu((current) => (current === 'add' ? null : current));
  }, [panelOpen]);
  const addable = menu === 'add' ? paneTypesToAdd(paneTree) : [];
  const panelLabel = panelOpen ? 'Hide panel' : 'Show panel';

  return (
    <div className="shell-band" data-tauri-drag-region>
      <div className="shell-band-title" data-tauri-drag-region>
        <TitleButton
          ref={sessionRef}
          connection={connection}
          open={menu === 'session'}
          onToggle={() => toggleMenu('session')}
        />
      </div>
      <div className="shell-band-actions">
        {panelOpen && (
          <button
            ref={addRef}
            type="button"
            className={`shell-icon-button${menu === 'add' ? ' is-open' : ''}`}
            aria-label="Add a pane"
            aria-haspopup="menu"
            aria-expanded={menu === 'add'}
            onClick={() => toggleMenu('add')}
          >
            <PlusIcon />
          </button>
        )}
        <button
          type="button"
          className="shell-icon-button"
          aria-label={`Search commands (${shortcutLabel(APP_SHORTCUTS.palette)})`}
          // The palette leaves presses on its own button to this toggle.
          data-palette-anchor=""
          onClick={onTogglePalette}
        >
          <SearchIcon />
        </button>
        <button
          type="button"
          className={`shell-icon-button${panelOpen ? '' : ' is-quiet'}`}
          // The label says what a press does, so no pressed state on top
          // of it ("Hide panel, pressed" reads backward).
          aria-label={panelLabel}
          title={`${panelLabel} (${shortcutLabel(APP_SHORTCUTS.panel)})`}
          onClick={onTogglePanel}
        >
          <PanelIcon />
        </button>
        {!mac && <WindowControls />}
      </div>
      {menu === 'session' && (
        <SessionMenu connection={connection} anchor={sessionRef.current} onClose={closeMenu} />
      )}
      {menu === 'add' && panelOpen && (
        <ShellMenu
          anchor={addRef.current}
          align="end"
          width={ADD_MENU_WIDTH}
          label="Add a pane"
          onClose={closeMenu}
        >
          {addable.length === 0 ? (
            <p className="shell-menu-note">Every pane is showing.</p>
          ) : (
            addable.map((pane) => (
              <ShellMenuItem
                key={pane}
                onSelect={() => {
                  closeMenu();
                  onAddPane(pane);
                }}
              >
                {PANE_LABELS[pane]}
              </ShellMenuItem>
            ))
          )}
        </ShellMenu>
      )}
    </div>
  );
}

// Minimize, maximize, and close for the frameless window on Windows and
// Linux, after the band's own buttons. Maximize reads Restore while the
// window is maximized.
function WindowControls() {
  const win = () => getCurrentWindow();
  const [maximized, setMaximized] = useState(false);
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    const w = getCurrentWindow();
    const read = () => {
      w.isMaximized()
        .then((m) => {
          if (!cancelled) setMaximized(m);
        })
        .catch(() => {});
    };
    read();
    w.onResized(read)
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);
  return (
    <div className="shell-window-controls">
      <button
        type="button"
        className="shell-icon-button"
        aria-label="Minimize"
        onClick={() => void win().minimize()}
      >
        <MinimizeIcon />
      </button>
      <button
        type="button"
        className="shell-icon-button"
        aria-label={maximized ? 'Restore' : 'Maximize'}
        onClick={() => void win().toggleMaximize()}
      >
        <MaximizeIcon />
      </button>
      <button
        type="button"
        className="shell-icon-button is-close"
        aria-label="Close"
        onClick={() => void win().close()}
      >
        <CloseIcon />
      </button>
    </div>
  );
}
