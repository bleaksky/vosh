import { pluginSetEnabled, type PluginRow } from '../../ipc/scripts';
import { errorText } from '../../lib/text';
import { Button, IconButton, MoreIcon, PlusIcon, Row, Section, Toggle } from '../../ui';

// The Plugins section of Scripts (boards 2 and 4). One row for each
// plugin in your plugins folder, its name in the MUD font over its
// description, with Stopped while Vosh holds it off, the switch that
// turns it on or off for the profile you play, and its more button.

/** The card's line before you have a plugin. */
export const NO_PLUGINS =
  'You have no plugins yet. A plugin is a folder of Lua that runs while you play.';

interface Props {
  /** Your plugins, or null until the list arrives. */
  plugins: PluginRow[] | null;
  /** Show the list a change handed back. */
  onPlugins: (plugins: PluginRow[]) => void;
  onError: (message: string | null) => void;
  /** Read the list again, after a change that failed. */
  onChanged: () => void;
}

export function PluginList({ plugins, onPlugins, onError, onChanged }: Props) {
  // The switch moves as you press it, and the list the command hands
  // back settles it.
  const setOn = (name: string, on: boolean) => {
    if (plugins) onPlugins(plugins.map((p) => (p.name === name ? { ...p, on } : p)));
    pluginSetEnabled(name, on)
      .then((list) => {
        onPlugins(list);
        onError(null);
      })
      .catch((e: unknown) => {
        onError(errorText(e));
        onChanged();
      });
  };

  return (
    <Section
      title="Plugins"
      id="plugins"
      actions={<Button icon={<PlusIcon />}>New plugin</Button>}
      help={{ topic: 'automate.lua-scripts', subject: 'Lua scripts' }}
    >
      {plugins?.length === 0 && <p className="st-plugin-empty">{NO_PLUGINS}</p>}
      {plugins?.map((plugin) => (
        <Row
          key={plugin.name}
          className="st-plugin-row"
          label={plugin.name}
          description={plugin.description}
        >
          {plugin.stopped && (
            <span className="st-meta" data-tone="warn">
              Stopped
            </span>
          )}
          <Toggle checked={plugin.on} onChange={(on) => setOn(plugin.name, on)} />
          <IconButton label={`${plugin.name} options`} icon={<MoreIcon />} />
        </Row>
      ))}
    </Section>
  );
}
