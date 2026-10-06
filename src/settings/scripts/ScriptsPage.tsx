import { useCallback, useEffect, useState } from 'react';
import { subscribeProfileSwitched } from '../../ipc/profiles';
import {
  luaOutputGet,
  pluginsList,
  subscribeLuaOutput,
  subscribePluginsChanged,
  type LuaLine,
  type PluginRow,
} from '../../ipc/scripts';
import { useTauriEvent } from '../../ipc/useTauriEvent';
import { errorText } from '../../lib/text';
import type { SettingsPageProps } from '../pageTypes';
import { LuaConsole } from './LuaConsole';
import { PluginList } from './PluginList';

// Settings > Scripts (Scripts and Panels, boards 2 and 4). Your plugins
// as the selected session sees them, and the Console with every [lua]
// line it printed. The page reads both as it opens and follows them
// through the plugin and Lua output events, so its own state holds
// them and no store does.

/** The lines the page keeps, as many as a session's Output ring. */
const CONSOLE_LINES = 500;

export function ScriptsPage({ onError }: SettingsPageProps) {
  const [plugins, setPlugins] = useState<PluginRow[] | null>(null);
  const [lines, setLines] = useState<LuaLine[]>([]);

  const readPlugins = useCallback(() => {
    pluginsList()
      .then(setPlugins)
      .catch((e: unknown) => onError(errorText(e)));
  }, [onError]);

  useEffect(() => {
    readPlugins();
    luaOutputGet()
      .then(setLines)
      .catch((e: unknown) => onError(errorText(e)));
  }, [readPlugins, onError]);

  useTauriEvent(subscribePluginsChanged, readPlugins);
  // A switch turns over the plugins of the session it changes.
  useTauriEvent(subscribeProfileSwitched, readPlugins);
  useTauriEvent(subscribeLuaOutput, (payload) =>
    setLines((prev) => [...prev, ...payload.lines].slice(-CONSOLE_LINES)),
  );

  return (
    <>
      <PluginList
        plugins={plugins}
        onPlugins={setPlugins}
        onError={onError}
        onChanged={readPlugins}
      />
      <LuaConsole lines={lines} onCleared={() => setLines([])} onError={onError} />
    </>
  );
}
