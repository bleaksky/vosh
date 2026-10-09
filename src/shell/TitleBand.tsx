import { useEffect, useRef, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useTauriEvent } from '../ipc/useTauriEvent';
import APP_SHORTCUTS from '../lib/appShortcuts.json';
import { ADD_PANE_MENU_EVENT, SESSION_MENU_EVENT, type SessionMenuRequest } from '../lib/appMenu';
import { ariaKeyshortcuts, isMacPlatform, shortcutLabel } from '../lib/shortcuts';
import { paneKey, paneRef, type PaneRef, type PaneSplit } from '../panel/paneLayout';
import {
  PANE_LABELS,
  luaPaneRef,
  luaPanesToAdd,
  offeredLuaPanes,
  paneLabel,
  paneTypesToAdd,
} from '../panel/paneTypes';
import { useLuaPanes } from '../stores/session/luaPanesStore';
import { usePluginRows } from '../stores/session/pluginRowsStore';
import type { Connection } from '../stores/session/useConnection';
import {
  CloseIcon,
  MaximizeIcon,
  MinimizeIcon,
  PlusIcon,
  SearchIcon,
  ToothedGearIcon,
} from '../ui/icons';
import { PanelIcon } from './icons';
import { SessionMenu } from './SessionMenu';
import { MenuItem, MenuSeparator } from '../ui/MenuSurface';
import { ShellMenu } from './ShellMenu';
import { chatRefToAdd } from '../panel/paneActions';
import { TitleButton } from './TitleButton';

// The 32 px title band across the top of the window. No fill and no
// line of its own: the terminal ground runs up under it and the panel
// ground runs up on the right. Its empty areas drag the window. The
// session button sits centered over the terminal column. The sessions
// toggle belongs to the frame (AppShell), which holds it at the band's
// left end while the sidebar hides. Add a pane, Search commands, the
// panel toggle, and Settings sit at the right, over the panel. On macOS
// the native traffic lights own the left corner. Windows and Linux draw
// minimize, maximize, and close here, after Settings, and the panel
// draws at least 248 px wide there to keep all seven over it. They have
// no menu bar, so there the gear is how you find Settings.

interface Props {
  connection: Connection;
  panelOpen: boolean;
  onTogglePanel: () => void;
  /** Open the palette, or close it when it is open. */
  onTogglePalette: () => void;
  /** Open Settings, the same as its shortcut. */
  onOpenSettings: () => void;
  /** The panel's pane tree. Add a pane lists the pane types it does
   *  not show yet, then the Lua panes it does not show. */
  paneTree: PaneSplit | null;
  onAddPane: (ref: PaneRef) => void;
  /** Runs after a menu closes, or after the gear opens Settings, to
   *  hand the caret back to the command line. */
  onMenuClosed: () => void;
  /** Turn the selected session's name into a field in its row, while
   *  the sessions sidebar shows. */
  renameInRow?: (() => void) | undefined;
  /** The sessions sidebar folded with two or more sessions open, so the
   *  session popover lists them. */
  listSessions?: boolean;
  /** Close a session from that list, asking first while it is
   *  connected. */
  onCloseSession?: (session: number) => void;
}

export function TitleBand({
  connection,
  panelOpen,
  onTogglePanel,
  onTogglePalette,
  onOpenSettings,
  paneTree,
  onAddPane,
  onMenuClosed,
  renameInRow,
  listSessions = false,
  onCloseSession,
}: Props) {
  const mac = isMacPlatform();
  const [menu, setMenu] = useState<'session' | 'add' | null>(null);
  // What the session popover opens on. The menu bar's Edit connection
  // opens it straight on its form, Rename session… with no sidebar on
  // its own, New session… on the form of the session it opened, and the
  // key remounts it so a second request starts fresh.
  const [session, setSession] = useState<{ request: SessionMenuRequest; key: number }>({
    request: { mode: 'menu' },
    key: 0,
  });
  const sessionRef = useRef<HTMLButtonElement | null>(null);
  const addRef = useRef<HTMLButtonElement | null>(null);
  const closeMenu = () => {
    setMenu(null);
    onMenuClosed();
  };
  const toggleMenu = (which: 'session' | 'add') => {
    if (which === 'session') setSession((s) => ({ request: { mode: 'menu' }, key: s.key + 1 }));
    setMenu((current) => (current === which ? null : which));
  };
  useEffect(() => {
    const onRequest = (e: Event) => {
      const request = (e as CustomEvent<SessionMenuRequest | undefined>).detail;
      if (!request) return;
      setSession((s) => ({ request, key: s.key + 1 }));
      setMenu('session');
    };
    const onAddPane = () => setMenu('add');
    window.addEventListener(SESSION_MENU_EVENT, onRequest);
    window.addEventListener(ADD_PANE_MENU_EVENT, onAddPane);
    return () => {
      window.removeEventListener(SESSION_MENU_EVENT, onRequest);
      window.removeEventListener(ADD_PANE_MENU_EVENT, onAddPane);
    };
  }, []);
  // Hiding the panel takes Add a pane with it, so its menu closes too
  // instead of coming back the next time the panel shows.
  useEffect(() => {
    if (!panelOpen) setMenu((current) => (current === 'add' ? null : current));
  }, [panelOpen]);
  const panelLabel = panelOpen ? 'Hide panel' : 'Show panel';

  return (
    <div className="shell-band" data-tauri-drag-region>
      <div className="shell-band-title" data-tauri-drag-region>
        <TitleButton
          ref={sessionRef}
          connection={connection}
          open={menu === 'session'}
          folded={listSessions}
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
          aria-label="Search commands"
          aria-keyshortcuts={ariaKeyshortcuts(APP_SHORTCUTS.palette, mac)}
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
          aria-keyshortcuts={ariaKeyshortcuts(APP_SHORTCUTS.panel, mac)}
          onClick={onTogglePanel}
        >
          <PanelIcon />
        </button>
        <button
          type="button"
          className="shell-icon-button"
          aria-label="Settings"
          title={`Settings (${shortcutLabel(APP_SHORTCUTS.settings)})`}
          aria-keyshortcuts={ariaKeyshortcuts(APP_SHORTCUTS.settings, mac)}
          onClick={(e) => {
            onOpenSettings();
            // WebView2 and WebKitGTK focus a button on click. Left on the
            // gear, the caret would take your next Space and open Settings
            // again, so it goes back to the command line.
            if (document.activeElement === e.currentTarget) onMenuClosed();
          }}
        >
          <ToothedGearIcon />
        </button>
        {!mac && <WindowControls />}
      </div>
      {menu === 'session' && (
        <SessionMenu
          key={session.key}
          connection={connection}
          anchor={sessionRef.current}
          request={session.request}
          renameInRow={renameInRow}
          listSessions={listSessions}
          onCloseSession={onCloseSession}
          onClose={closeMenu}
        />
      )}
      {menu === 'add' && panelOpen && (
        <AddPaneMenu
          anchor={addRef.current}
          paneTree={paneTree}
          onAdd={(ref) => {
            closeMenu();
            onAddPane(ref);
          }}
          onClose={closeMenu}
        />
      )}
    </div>
  );
}

// Add a pane's menu: the pane types the tree has room for, then the
// Lua panes it does not show. It reads the Lua pane and plugin stores
// only while it is open, since a plugin can send its panes on every
// prompt and the band should not draw again for each one.
function AddPaneMenu({
  anchor,
  paneTree,
  onAdd,
  onClose,
}: {
  anchor: HTMLElement | null;
  paneTree: PaneSplit | null;
  onAdd: (ref: PaneRef) => void;
  onClose: () => void;
}) {
  const luaPanes = useLuaPanes();
  const pluginRows = usePluginRows();
  const builtIns = paneTypesToAdd(paneTree);
  const luaToAdd = luaPanesToAdd(paneTree, offeredLuaPanes(luaPanes, pluginRows));
  // A second Chat pane starts on tell, and the menu says so.
  const chatOnTell = chatRefToAdd(paneTree).props.channel === 'tell';
  return (
    <ShellMenu anchor={anchor} align="end" label="Add a pane" onClose={onClose}>
      {builtIns.length === 0 && luaToAdd.length === 0 && (
        <li role="none" className="menu-note">
          Every pane is showing.
        </li>
      )}
      {builtIns.map((pane) => (
        <MenuItem
          key={pane}
          trailing={
            pane === 'chat' && chatOnTell ? (
              <span className="menu-hint">starts on Tell</span>
            ) : undefined
          }
          onSelect={() => onAdd(paneRef(pane))}
        >
          {PANE_LABELS[pane]}
        </MenuItem>
      ))}
      {builtIns.length > 0 && luaToAdd.length > 0 && <MenuSeparator />}
      {luaToAdd.map((offer) => {
        const ref = luaPaneRef(offer);
        return (
          <MenuItem
            key={paneKey(ref)}
            trailing={<span className="menu-hint">{offer.plugin}</span>}
            onSelect={() => onAdd(ref)}
          >
            {paneLabel(ref)}
          </MenuItem>
        );
      })}
    </ShellMenu>
  );
}

// Minimize, maximize, and close for the frameless window on Windows and
// Linux, after the band's own buttons. Maximize reads Restore while the
// window is maximized.
function WindowControls() {
  const win = () => getCurrentWindow();
  const [maximized, setMaximized] = useState(false);
  const read = () => {
    getCurrentWindow()
      .isMaximized()
      .then(setMaximized)
      .catch(() => {});
  };
  useEffect(() => read(), []);
  useTauriEvent<unknown>((cb) => getCurrentWindow().onResized(cb), read);
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
