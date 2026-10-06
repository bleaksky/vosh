// The Scripts page in Settings: your plugins, the Output ring of a
// session and the console, behind the commands in
// src-tauri/src/ipc/scripts.rs. Each command takes the session it acts
// on and falls back to the selected one, which is the one Settings
// works on, so these wrappers name none.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { LUA_OUTPUT, PLUGINS_CHANGED } from './events';

/** Why Vosh stopped a plugin: one call ran past 100 ms, one call used
 *  more than 32 MB, or your scripts held more than 128 MB. */
export type PluginStop = 'time' | 'call_memory' | 'state_memory';

/** The tag a plugin's lines carry in the Output ring, like
 *  `plugin:vitals_alert`, as the app's Owner::tag names it. */
export function pluginOwner(name: string): string {
  return `plugin:${name}`;
}

/** One plugin in your plugins folder, as the selected session sees it. */
export interface PluginRow {
  name: string;
  version: string;
  author: string;
  description: string;
  /** The file it runs first. */
  entry: string;
  /** The profile the session plays turns it on. */
  on: boolean;
  /** Why Vosh stopped it in the session, while it holds it off. */
  stopped: PluginStop | null;
  /** Its folder name breaks the rule New plugin shows, which only a
   *  folder named by hand can do. It loads all the same, and the page
   *  cannot open it but can turn it off. */
  misnamed: boolean;
}

/** Your plugins, sorted by name. */
export async function pluginsList(): Promise<PluginRow[]> {
  return invoke('plugins_list');
}

/** What manifest.toml in a plugin's folder says. */
export interface PluginManifest {
  name: string;
  version: string;
  description: string;
  author: string;
  /** The file it runs first, by its path inside the folder. */
  entry: string;
}

/** A plugin's folder as its page shows it. */
export interface PluginFolder {
  manifest: PluginManifest;
  /** The code of the file it runs first, or of the file asked for. */
  code: string;
  /** Every Lua file in the folder, by its path inside it, sorted. */
  files: string[];
  /** The folder as the Manifest tab names it, like
   *  `plugins/vitals_alert`. */
  folder: string;
}

/** The plugin `name` as its folder holds it. `file`, one of its Lua
 *  files, hands back that file's code in place of the code of the file
 *  it runs first, for a new pick under Runs first. */
export async function pluginRead(name: string, file?: string): Promise<PluginFolder> {
  return invoke('plugin_read', { name, file });
}

/** Make the plugin `name`, a manifest and a main.lua in a folder of
 *  that name, turned on for the profile the selected session plays.
 *  Resolves to the list with it. */
export async function pluginCreate(name: string): Promise<PluginRow[]> {
  return invoke('plugin_create', { name });
}

/** Write `code` to the file `manifest` runs first and the manifest to
 *  the plugin `name`, then load it again in every session whose profile
 *  turns it on, which clears a stop. Resolves to the list after it. */
export async function pluginSave(
  name: string,
  manifest: PluginManifest,
  code: string,
): Promise<PluginRow[]> {
  return invoke('plugin_save', { name, manifest, code });
}

/** Show the folder of the plugin `name` in the system's file manager. */
export async function pluginReveal(name: string): Promise<void> {
  await invoke('plugin_reveal', { name });
}

/** Turn a plugin on or off in the profile the selected session plays,
 *  which loads or unloads it in every session on that profile. Resolves
 *  to the list as it reads after the change. */
export async function pluginSetEnabled(name: string, on: boolean): Promise<PluginRow[]> {
  return invoke('plugin_set_enabled', { name, on });
}

/** Read the plugin `name` from its folder again and load it in every
 *  session whose profile turns it on, which clears a stop. Resolves to
 *  the list after it. */
export async function pluginReload(name: string): Promise<PluginRow[]> {
  return invoke('plugin_reload', { name });
}

/** Write the plugin `name` to a .zip in your Downloads folder. Resolves
 *  to the name of the file, like `vitals_alert.zip`. */
export async function pluginExport(name: string): Promise<string> {
  return invoke('plugin_export', { name });
}

/** Delete the folder of the plugin `name` and take it off the list of
 *  every profile. Resolves to the list without it. */
export async function pluginRemove(name: string): Promise<PluginRow[]> {
  return invoke('plugin_remove', { name });
}

/** One file of a folder you dropped, by its path inside what you
 *  dropped, like `weather_pane/main.lua`. Bytes travel as numbers. */
export interface DroppedFile {
  path: string;
  bytes: number[];
}

/** What Install reads a plugin from: a .zip you picked as its bytes, or
 *  the files of a folder you dropped, under the name of the file or the
 *  folder, which every sentence about the install names. */
export interface PluginPackage {
  fileName: string;
  bytes?: number[];
  files?: DroppedFile[];
}

/** What Install asks about before it installs a plugin. */
export interface PluginInstallCheck {
  name: string;
  version: string;
  author: string;
  /** The plugin of that name you have, which the install replaces,
   *  with the profiles that turn it on by the names Settings shows. */
  existing: { version: string; on_in: string[] } | null;
}

/** Read and check the plugin in `pkg` without installing it. Rejects
 *  with the sentence that says why Vosh refuses it. */
export async function pluginInstallCheck(pkg: PluginPackage): Promise<PluginInstallCheck> {
  return invoke('plugin_install_check', {
    fileName: pkg.fileName,
    bytes: pkg.bytes,
    files: pkg.files,
  });
}

/** Install the plugin in `pkg` in place of the one of that name you
 *  have. It starts off in every profile. Resolves to the list with it. */
export async function pluginInstall(pkg: PluginPackage): Promise<PluginRow[]> {
  return invoke('plugin_install', { fileName: pkg.fileName, bytes: pkg.bytes, files: pkg.files });
}

/** What a line in the Output ring is. */
export type LuaKind = 'print' | 'error' | 'note' | 'input';

/** One line of the Output ring. */
export interface LuaLine {
  /** When Vosh printed it, in milliseconds since the Unix epoch. */
  ts_ms: number;
  /** Whose Lua it is about, like `plugin:vitals_alert` or `#lua`. */
  owner: string;
  kind: LuaKind;
  text: string;
  /** Where in your Lua an error happened. */
  at?: { source: string; line: number };
}

/** The newest lines the selected session printed about Lua, oldest
 *  first. */
export async function luaOutputGet(): Promise<LuaLine[]> {
  return invoke('lua_output_get');
}

/** Let go of the lines of `owner`, a tag like `plugin:vitals_alert`, in
 *  the Output ring of the selected session, or of every line with no
 *  owner named. */
export async function luaOutputClear(owner?: string): Promise<void> {
  await invoke('lua_output_clear', { owner });
}

/** Run `code` in the selected session, inside the plugin `plugin` when
 *  one is named, or as a `#lua` line runs. What it prints comes back on
 *  `session://lua-output`. */
export async function luaRun(code: string, plugin?: string): Promise<void> {
  await invoke('lua_run', { code, plugin });
}

/** The lines one step added to a session's Output ring. */
export interface LuaOutputPayload {
  session: number;
  lines: LuaLine[];
}

export async function subscribeLuaOutput(
  cb: (payload: LuaOutputPayload) => void,
): Promise<UnlistenFn> {
  return listen<LuaOutputPayload>(LUA_OUTPUT, (event) => {
    cb(event.payload);
  });
}

/** Hear that a plugin was made, saved, installed, removed, turned on or
 *  off, loaded again or stopped, or that a profile switch turned a
 *  session's plugins over. */
export async function subscribePluginsChanged(cb: () => void): Promise<UnlistenFn> {
  return listen(PLUGINS_CHANGED, () => {
    cb();
  });
}
