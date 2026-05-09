import { useEffect, useState } from 'react';
import {
  listPlugins,
  reloadPlugin,
  setPluginEnabled,
  type PluginInfo,
} from '../lib/session';

interface Props {
  onError: (message: string) => void;
}

export function PluginsPanel({ onError }: Props) {
  const [plugins, setPlugins] = useState<PluginInfo[]>([]);
  const [status, setStatus] = useState('');

  useEffect(() => {
    let cancelled = false;
    listPlugins()
      .then((list) => {
        if (cancelled) return;
        setPlugins(list);
      })
      .catch((e) => onError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [onError]);

  const refreshPlugins = async () => {
    try {
      setPlugins(await listPlugins());
      setStatus('plugin list refreshed');
    } catch (e) {
      setStatus(`refresh failed ${String(e)}`);
      onError(String(e));
    }
  };

  const handlePluginToggle = async (name: string, next: boolean) => {
    try {
      await setPluginEnabled(name, next);
      setPlugins(await listPlugins());
      setStatus(next ? `enabled ${name}` : `disabled ${name} (effective next launch)`);
    } catch (e) {
      setStatus(`plugin toggle failed ${String(e)}`);
    }
  };

  const handlePluginReload = async (name: string) => {
    try {
      await reloadPlugin(name);
      setStatus(`reloaded ${name}`);
    } catch (e) {
      setStatus(`reload failed ${String(e)}`);
    }
  };

  return (
    <div className="plugins-panel">
      {plugins.length === 0 ? (
        <p className="drawer-empty">
          no plugins discovered. Drop a plugin directory into{' '}
          <code>&lt;app_data_dir&gt;/plugins/</code> and click refresh.
        </p>
      ) : (
        <ul className="plugin-list">
          {plugins.map((plugin) => (
            <li key={plugin.name} className="plugin-row">
              <div className="plugin-meta">
                <strong>{plugin.name}</strong>
                {plugin.version && <span className="plugin-version">{plugin.version}</span>}
                {plugin.description && (
                  <div className="plugin-description">{plugin.description}</div>
                )}
              </div>
              <div className="plugin-actions">
                <label>
                  <input
                    type="checkbox"
                    checked={plugin.enabled}
                    onChange={(e) => void handlePluginToggle(plugin.name, e.target.checked)}
                  />
                  enabled
                </label>
                <button
                  type="button"
                  onClick={() => void handlePluginReload(plugin.name)}
                  disabled={!plugin.enabled}
                >
                  reload
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}
      <div className="drawer-actions">
        <button type="button" onClick={() => void refreshPlugins()}>
          refresh plugin list
        </button>
      </div>
      <div className="drawer-status">{status}</div>
    </div>
  );
}
