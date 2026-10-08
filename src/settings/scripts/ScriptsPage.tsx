import { useCallback, useEffect, useState } from 'react';
import { subscribeProfileSwitched } from '../../ipc/profiles';
import {
  luaOutputGet,
  pluginOwner,
  pluginsList,
  subscribeLuaOutput,
  subscribePluginsChanged,
  type LuaLine,
  type PluginRow,
} from '../../ipc/scripts';
import { useTauriEvent } from '../../ipc/useTauriEvent';
import { errorText } from '../../lib/text';
import { useSelected } from '../../stores/session/sessionsStore';
import type { SettingsPageProps } from '../pageTypes';
import { LuaConsole } from './LuaConsole';
import { NewPluginDialog } from './NewPluginDialog';
import { PluginList } from './PluginList';
import { PluginPage } from './PluginPage';
import type { PluginSave } from './pluginState';

// Settings > Scripts. Your plugins as the selected session sees them,
// and the Console with every [lua] line it printed. The page reads both
// as it opens and again when you select another session, and follows
// them through the plugin and Lua output events, taking the lines of
// the selected session alone, so its own state holds them and no store
// does. A plugin opens on a page of its own inside the group,
// `scripts:<name>`, which takes its share of both.

/** The lines the page keeps, as many as a session's Output ring. */
const CONSOLE_LINES = 500;

export function ScriptsPage({ target, onError, navigate, setLeaveGuard }: SettingsPageProps) {
  const session = useSelected();
  const [plugins, setPlugins] = useState<PluginRow[] | null>(null);
  const [lines, setLines] = useState<LuaLine[]>([]);
  const [creating, setCreating] = useState(false);
  // Each plugin's last save in this window, which its save bar tells.
  const [saves, setSaves] = useState<Readonly<Record<string, PluginSave>>>({});

  const readPlugins = useCallback(() => {
    pluginsList(session)
      .then(setPlugins)
      .catch((e: unknown) => onError(errorText(e)));
  }, [session, onError]);

  useEffect(() => {
    readPlugins();
    luaOutputGet(session)
      .then(setLines)
      .catch((e: unknown) => onError(errorText(e)));
  }, [readPlugins, session, onError]);

  useTauriEvent(subscribePluginsChanged, readPlugins);
  // A switch turns over the plugins of the session it changes.
  useTauriEvent(subscribeProfileSwitched, readPlugins);
  useTauriEvent(subscribeLuaOutput, (payload) => {
    if (payload.session !== session) return;
    setLines((prev) => [...prev, ...payload.lines].slice(-CONSOLE_LINES));
  });

  const open = (name: string) => navigate({ group: 'scripts', section: name });

  const name = target.section;
  if (name !== undefined) {
    return (
      <PluginPage
        key={name}
        name={name}
        plugins={plugins}
        lines={lines}
        saved={saves[name]}
        onSaved={(save) => setSaves((prev) => ({ ...prev, [name]: save }))}
        onPlugins={setPlugins}
        onChanged={readPlugins}
        onCleared={() => setLines((prev) => prev.filter((l) => l.owner !== pluginOwner(name)))}
        onError={onError}
        setLeaveGuard={setLeaveGuard}
      />
    );
  }

  return (
    <>
      <PluginList
        plugins={plugins}
        onPlugins={setPlugins}
        onError={onError}
        onChanged={readPlugins}
        onNew={() => setCreating(true)}
        onOpen={open}
      />
      <LuaConsole lines={lines} onCleared={() => setLines([])} onError={onError} />
      {creating && (
        <NewPluginDialog
          plugins={plugins ?? []}
          onCreated={(list, created) => {
            setPlugins(list);
            setCreating(false);
            open(created);
          }}
          onCancel={() => setCreating(false)}
          onError={onError}
        />
      )}
    </>
  );
}
