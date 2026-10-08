import { useEffect, useRef, useState } from 'react';
import {
  pluginExport,
  pluginInstall,
  pluginInstallCheck,
  pluginReload,
  pluginRemove,
  pluginReveal,
  type PluginInstallCheck,
  type PluginPackage,
  type PluginRow,
} from '../../ipc/scripts';
import { errorText } from '../../lib/text';
import { Button, Card, IconButton, MoreIcon, PlusIcon, Section, Toggle } from '../../ui';
import { ConfirmDialog } from '../../ui/ConfirmDialog';
import type { MenuCloseReason, MenuPlacement } from '../../ui/MenuSurface';
import { menuBelow } from '../../ui/menuPlacement';
import { InstallDialog } from './InstallDialog';
import { PluginMenu } from './PluginMenu';
import { droppedPackage, zipPackage } from './pluginPackage';
import { switchPlugin } from './switchPlugin';

// The Plugins section of Scripts. One row for each plugin in your
// plugins folder, its name in the MUD font over its description, with
// Stopped while Vosh holds it off, the switch that turns it on or off
// for the profile you play, and its more button. A press on the row
// opens the plugin's page, as a profile row does in Characters, and New
// plugin asks for the name of a new one. A plugin whose folder you
// named by hand outside the rule still loads, so its row shows too and
// says how to make it one the page can open. Its switch only turns it
// off, and its menu stays shut, since every other Scripts command holds
// a plugin name to the rule.
//
// Install takes a .zip you pick or a folder you drop anywhere on the
// list page, and asks once. Each row's menu reloads, shows, exports and
// removes its plugin, and a quiet line under the list says where an
// export went, as the Characters list does.

/** The card's line before you have a plugin. */
export const NO_PLUGINS =
  'You have no plugins yet. A plugin is a folder of Lua that runs while you play.';

/** The line under a plugin whose folder name breaks the rule. */
export const MISNAMED_NOTE =
  'Rename its folder with only letters, digits and underscores to open it or turn it on here.';

interface Props {
  /** Your plugins, or null until the list arrives. */
  plugins: PluginRow[] | null;
  /** Show the list a change handed back. */
  onPlugins: (plugins: PluginRow[]) => void;
  onError: (message: string | null) => void;
  /** Read the list again, after a change that failed. */
  onChanged: () => void;
  /** Ask for the name of a new plugin. */
  onNew: () => void;
  /** Open the page of the plugin `name`. */
  onOpen: (name: string) => void;
}

interface OpenMenu {
  name: string;
  at: MenuPlacement;
  /** The more button that opened it, which takes focus back. */
  anchor: HTMLButtonElement | null;
}

/** A plugin Install read and checked, waiting for your answer. */
interface Asking {
  pkg: PluginPackage;
  check: PluginInstallCheck;
}

export function PluginList({ plugins, onPlugins, onError, onChanged, onNew, onOpen }: Props) {
  const [menu, setMenu] = useState<OpenMenu | null>(null);
  const [removing, setRemoving] = useState<string | null>(null);
  const [asking, setAsking] = useState<Asking | null>(null);
  const [installing, setInstalling] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement | null>(null);

  const fail = (e: unknown) => onError(errorText(e));

  /** Show the list a change hands back, or its refusal and the list
   *  read again. */
  const change = (action: Promise<PluginRow[]>) =>
    action
      .then((list) => {
        onPlugins(list);
        onError(null);
      })
      .catch((e: unknown) => {
        fail(e);
        onChanged();
      });

  /** Check the plugin `read` holds and ask about it, or show the
   *  sentence that says why Vosh refuses it. */
  const ask = (read: Promise<PluginPackage>) => {
    read
      .then(async (pkg) => ({ pkg, check: await pluginInstallCheck(pkg) }))
      .then((next) => {
        setAsking(next);
        onError(null);
      })
      .catch(fail);
  };
  // The drop listeners stay put while the list shows, and call the
  // latest `ask`.
  const askRef = useRef(ask);
  askRef.current = ask;

  // A folder or a .zip dropped anywhere on the list page installs. The
  // Settings window leaves drops to the page (app/windows.rs), so a
  // drop that carries files is taken here and never opens the file.
  useEffect(() => {
    const carriesFiles = (e: DragEvent) => e.dataTransfer?.types.includes('Files') === true;
    const over = (e: DragEvent) => {
      if (!carriesFiles(e)) return;
      e.preventDefault();
      if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy';
    };
    const drop = (e: DragEvent) => {
      if (!carriesFiles(e)) return;
      e.preventDefault();
      // The entry is only there while the drop event runs.
      const entry = e.dataTransfer?.items[0]?.webkitGetAsEntry();
      if (entry) askRef.current(droppedPackage(entry));
    };
    document.addEventListener('dragover', over);
    document.addEventListener('drop', drop);
    return () => {
      document.removeEventListener('dragover', over);
      document.removeEventListener('drop', drop);
    };
  }, []);

  const install = () => {
    if (!asking) return;
    setInstalling(true);
    void change(pluginInstall(asking.pkg)).finally(() => {
      setInstalling(false);
      setAsking(null);
    });
  };

  const openMenu = (name: string, button: HTMLButtonElement | null, point?: MenuPlacement) => {
    const at = point ?? (button && menuBelow(button.getBoundingClientRect()));
    if (at) setMenu({ name, at, anchor: button });
  };

  const closeMenu = (reason: MenuCloseReason) => {
    const anchor = menu?.anchor;
    setMenu(null);
    if (reason === 'escape') anchor?.focus();
  };

  // Focus goes back to the row's more button, and the action runs.
  const pick = (action: (name: string) => void) => () => {
    if (!menu) return;
    setMenu(null);
    menu.anchor?.focus();
    action(menu.name);
  };

  const reveal = (name: string) => {
    pluginReveal(name)
      .then(() => onError(null))
      .catch(fail);
  };

  const exportPlugin = (name: string) => {
    pluginExport(name)
      .then((file) => {
        setStatus(`Vosh saved ${file} in your Downloads folder.`);
        onError(null);
      })
      .catch(fail);
  };

  const remove = (name: string) => {
    setRemoving(null);
    setStatus(null);
    void change(pluginRemove(name));
  };

  return (
    <Section
      title="Plugins"
      id="plugins"
      card={false}
      actions={
        <>
          <Button onClick={() => fileRef.current?.click()}>Install…</Button>
          <input
            ref={fileRef}
            type="file"
            accept=".zip"
            className="visually-hidden"
            tabIndex={-1}
            aria-hidden="true"
            onChange={(e) => {
              const file = e.target.files?.[0];
              // Clear the pick so the same file can install again.
              e.target.value = '';
              if (file) ask(zipPackage(file));
            }}
          />
          <Button icon={<PlusIcon />} onClick={onNew}>
            New plugin
          </Button>
        </>
      }
      help={{ topic: 'automate.lua-scripts', subject: 'Lua scripts' }}
    >
      <Card>
        {plugins?.length === 0 && <p className="st-plugin-empty">{NO_PLUGINS}</p>}
        {plugins?.map((plugin) => (
          <div
            key={plugin.name}
            className="st-row st-plugin-row"
            onContextMenu={(e) => {
              e.preventDefault();
              if (!plugin.misnamed) openMenu(plugin.name, null, { x: e.clientX, y: e.clientY });
            }}
          >
            {plugin.misnamed ? (
              <div className="st-row-text">
                <span className="st-row-label">{plugin.name}</span>
                <span className="st-row-desc">{MISNAMED_NOTE}</span>
              </div>
            ) : (
              <button
                type="button"
                className="st-row-text st-plugin-open"
                onClick={() => onOpen(plugin.name)}
              >
                <span className="st-row-label">{plugin.name}</span>
                {plugin.description !== '' && (
                  <span className="st-row-desc">{plugin.description}</span>
                )}
              </button>
            )}
            <div className="st-row-control">
              {plugin.stopped && (
                <span className="st-meta" data-tone="warn">
                  Stopped
                </span>
              )}
              <Toggle
                checked={plugin.on}
                disabled={plugin.misnamed && !plugin.on}
                aria-label={plugin.name}
                onChange={(on) =>
                  switchPlugin(plugins, plugin.name, on, { onPlugins, onError, onChanged })
                }
              />
              <IconButton
                label={`${plugin.name} options`}
                icon={<MoreIcon />}
                aria-haspopup="menu"
                aria-expanded={menu?.name === plugin.name}
                disabled={plugin.misnamed}
                onClick={(e) => {
                  if (menu?.name === plugin.name) setMenu(null);
                  else openMenu(plugin.name, e.currentTarget);
                }}
              />
            </div>
          </div>
        ))}
      </Card>

      <p className="st-plugin-status" role="status">
        {status}
      </p>

      {menu && (
        <PluginMenu
          name={menu.name}
          at={menu.at}
          anchor={menu.anchor}
          onClose={closeMenu}
          onReload={pick((name) => void change(pluginReload(name)))}
          onReveal={pick(reveal)}
          onExport={pick(exportPlugin)}
          onRemove={pick(setRemoving)}
        />
      )}

      {removing && (
        <ConfirmDialog
          title={`Remove ${removing}?`}
          body={`Vosh deletes the ${removing} folder and turns the plugin off in every profile. You cannot undo this.`}
          confirmLabel="Remove"
          onConfirm={() => remove(removing)}
          onCancel={() => setRemoving(null)}
        />
      )}

      {asking && (
        <InstallDialog
          check={asking.check}
          busy={installing}
          onInstall={install}
          onCancel={() => setAsking(null)}
        />
      )}
    </Section>
  );
}
