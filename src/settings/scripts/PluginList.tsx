import { type PluginRow } from '../../ipc/scripts';
import { Button, IconButton, MoreIcon, PlusIcon, Section, Toggle } from '../../ui';
import { switchPlugin } from './switchPlugin';

// The Plugins section of Scripts (boards 2 and 4). One row for each
// plugin in your plugins folder, its name in the MUD font over its
// description, with Stopped while Vosh holds it off, the switch that
// turns it on or off for the profile you play, and its more button. A
// press on the row opens the plugin's page, as a profile row does in
// Characters, and New plugin asks for the name of a new one.

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
  /** Ask for the name of a new plugin. */
  onNew: () => void;
  /** Open the page of the plugin `name`. */
  onOpen: (name: string) => void;
}

export function PluginList({ plugins, onPlugins, onError, onChanged, onNew, onOpen }: Props) {
  return (
    <Section
      title="Plugins"
      id="plugins"
      actions={
        <Button icon={<PlusIcon />} onClick={onNew}>
          New plugin
        </Button>
      }
      help={{ topic: 'automate.lua-scripts', subject: 'Lua scripts' }}
    >
      {plugins?.length === 0 && <p className="st-plugin-empty">{NO_PLUGINS}</p>}
      {plugins?.map((plugin) => (
        <div key={plugin.name} className="st-row st-plugin-row">
          <button
            type="button"
            className="st-row-text st-plugin-open"
            onClick={() => onOpen(plugin.name)}
          >
            <span className="st-row-label">{plugin.name}</span>
            {plugin.description !== '' && <span className="st-row-desc">{plugin.description}</span>}
          </button>
          <div className="st-row-control">
            {plugin.stopped && (
              <span className="st-meta" data-tone="warn">
                Stopped
              </span>
            )}
            <Toggle
              checked={plugin.on}
              aria-label={plugin.name}
              onChange={(on) =>
                switchPlugin(plugins, plugin.name, on, { onPlugins, onError, onChanged })
              }
            />
            <IconButton label={`${plugin.name} options`} icon={<MoreIcon />} />
          </div>
        </div>
      ))}
    </Section>
  );
}
