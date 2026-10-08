// What a plugin's page under Scripts says about the plugin, from the
// list and the Output ring: why Vosh stopped it, which line of its file
// its newest error names, what its empty Output says, and when you last
// saved it in this window.

import { pluginOwner, type LuaLine, type PluginStop } from '../../ipc/scripts';
import type { CodeMark } from '../../ui/codeEditorStyle';
import { saveTime } from './scriptTimes';

/** What each stop of the Scripts design says the call did. */
const STOP_REASON: Readonly<Record<PluginStop, string>> = {
  time: 'one call ran past 100 ms',
  call_memory: 'one call used more than 32 MB',
  state_memory: 'your scripts held more than 128 MB',
};

/** The warn note over the editor of a plugin Vosh stopped, as board 3
 *  writes it for the time limit. */
export function stopNote(name: string, stop: PluginStop): string {
  return `Vosh stopped ${name} because ${STOP_REASON[stop]}. It stays off until you save it or restart Vosh.`;
}

/** The note in a plugin's Output before it has a line. */
export function noPluginLines(name: string): string {
  return `Every print and error from ${name} shows here and in the terminal.`;
}

/** The line of `file` to mark in the editor of the plugin `name`: the
 *  place the newest error or stop of the plugin since it loaded at
 *  `loadedMs` names, with that line of Output for the hover. Null when
 *  the plugin has had none since, or when its newest names no place in
 *  `file`. A load prints its own errors no sooner than it starts, so a
 *  line from before then is about code that no longer runs. */
export function errorMark(
  lines: readonly LuaLine[],
  name: string,
  file: string,
  loadedMs: number | null,
): CodeMark | null {
  const owner = pluginOwner(name);
  for (let i = lines.length - 1; i >= 0; i--) {
    const line = lines[i];
    if (line.owner !== owner) continue;
    if (loadedMs !== null && line.ts_ms < loadedMs) return null;
    if (line.kind !== 'error') continue;
    // Lua names the chunk by the plugin's folder and the file in it.
    if (line.at?.source !== `${name}/${file}`) return null;
    return { line: line.at.line, message: line.text };
  }
  return null;
}

/** A save of a plugin in this window: when, and whether it loaded the
 *  plugin again, which it does where its profile turns it on. */
export interface PluginSave {
  at: number;
  reloaded: boolean;
}

/** What the save bar says after `save`, like `Reloaded at 21:14`. */
export function saveStatus(save: PluginSave): string {
  return `${save.reloaded ? 'Reloaded' : 'Saved'} at ${saveTime(save.at)}`;
}
