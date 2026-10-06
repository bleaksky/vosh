import type { PluginRow } from '../../ipc/scripts';

/** True when `name` keeps the rule every plugin name keeps, one or more
 *  ASCII letters, digits and underscores, as plugin_name_ok in
 *  src-tauri/src/app/plugins/folder.rs holds it. */
export function pluginNameOk(name: string): boolean {
  return /^[A-Za-z0-9_]+$/.test(name);
}

/** The plugin you have under `name` in any case, since the disks of
 *  macOS and Windows ignore case, or null when the name is free. */
export function takenPluginName(plugins: readonly PluginRow[], name: string): string | null {
  const want = name.toLowerCase();
  return plugins.find((p) => p.name.toLowerCase() === want)?.name ?? null;
}
