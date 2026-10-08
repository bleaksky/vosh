import type { PluginInstallCheck } from '../../ipc/scripts';
import { listJoin } from '../../lib/text';
import { ConfirmDialog } from '../../ui/ConfirmDialog';

// Install asks once: the confirm recipe in the primary tone, naming the
// plugin with its version and author over the warning every plugin
// carries. Over a plugin you have, it says which one goes and that the
// install starts off everywhere.

/** The plugin as the manifest names it, like `weather_pane 0.2.0`. */
function named(name: string, version: string): string {
  return version === '' ? name : `${name} ${version}`;
}

/** The dialog's copy for the plugin `check` read. */
function body(check: PluginInstallCheck): string {
  const by = check.author === '' ? '' : ` by ${check.author}`;
  const parts = [
    `${named(check.name, check.version)}${by}.`,
    'A plugin can send commands to the game and read everything the game sends.',
    'Install plugins only from people you trust.',
  ];
  const old = check.existing;
  if (old) {
    const had = named(check.name, old.version);
    // A plugin no profile turns on has nothing to turn off.
    parts.push(
      old.on_in.length === 0
        ? `You have ${had}. Installing replaces it.`
        : `You have ${had}, on in ${listJoin(old.on_in)}. Installing replaces it and turns it off in every profile.`,
    );
  }
  return parts.join(' ');
}

interface Props {
  check: PluginInstallCheck;
  /** Holds Install off while the install runs. */
  busy: boolean;
  onInstall: () => void;
  onCancel: () => void;
}

export function InstallDialog({ check, busy, onInstall, onCancel }: Props) {
  return (
    <ConfirmDialog
      title={`Install ${check.name}?`}
      body={body(check)}
      confirmLabel="Install"
      tone="primary"
      confirmDisabled={busy}
      onConfirm={onInstall}
      onCancel={onCancel}
    />
  );
}
