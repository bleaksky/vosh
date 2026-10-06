import { pluginSetEnabled, type PluginRow } from '../../ipc/scripts';
import { errorText } from '../../lib/text';

/** Where a switch reports to: the list to show, the error line, and a
 *  read of the list again after a change that failed. */
export interface SwitchReport {
  onPlugins: (plugins: PluginRow[]) => void;
  onError: (message: string | null) => void;
  onChanged: () => void;
}

/** Turn the plugin `name` on or off for the profile you play, from its
 *  row under Scripts or from its page. The switch moves as you press
 *  it, and the list the command hands back settles it. */
export function switchPlugin(
  plugins: PluginRow[] | null,
  name: string,
  on: boolean,
  report: SwitchReport,
): void {
  if (plugins) report.onPlugins(plugins.map((p) => (p.name === name ? { ...p, on } : p)));
  pluginSetEnabled(name, on)
    .then((list) => {
      report.onPlugins(list);
      report.onError(null);
    })
    .catch((e: unknown) => {
      report.onError(errorText(e));
      report.onChanged();
    });
}
