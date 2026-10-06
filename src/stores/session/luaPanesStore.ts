import { luaPanesGet, onLuaPanes, type LuaPane, type LuaPanesChange } from '../../ipc/panes';
import { createSessionStore } from '../sessionStore';

// The panes the plugins of each session draw with mud.pane, by plugin
// and id. Lua runs in each session on its own, so a Lua pane shows what
// the session in front draws. The backend sends what changed once per
// flush on session://lua-panes, and a window that shows a session for
// the first time asks lua_panes_get for every pane it holds. A
// disconnect keeps them, as the backend does, since the plugins keep
// running.

export type LuaPanes = ReadonlyMap<string, LuaPane>;

const NONE: LuaPanes = new Map();

/** The key of the pane `id` of `plugin`. */
function luaPaneKey(plugin: string, id: string): string {
  return JSON.stringify([plugin, id]);
}

/** The panes after `change`: each pane it carries in place of the one
 *  before, and each one it removes gone. */
export function foldLuaPanes(now: LuaPanes, change: LuaPanesChange): LuaPanes {
  if (change.panes.length === 0 && change.removed.length === 0) return now;
  const next = new Map(now);
  for (const { plugin, id } of change.removed) next.delete(luaPaneKey(plugin, id));
  for (const pane of change.panes) next.set(luaPaneKey(pane.plugin, pane.id), pane);
  return next;
}

/** Every pane a session holds, from the snapshot. */
function fromSnapshot(data: unknown): LuaPanes {
  if (!Array.isArray(data)) return NONE;
  return foldLuaPanes(NONE, { panes: data as LuaPane[], removed: [] });
}

const store = createSessionStore<LuaPanes>({
  state: NONE,
  events: [
    (apply) => onLuaPanes((change, session) => apply(session, (now) => foldLuaPanes(now, change))),
  ],
  connection: (now) => now,
  snapshot: { ask: luaPanesGet, take: (_now, data) => fromSnapshot(data) },
});

export const startLuaPanesStore = store.start;
export const getLuaPanes = store.get;

/** The pane `id` of `plugin` in the session in front, or undefined
 *  while it draws none. */
export function useLuaPane(plugin: string, id: string): LuaPane | undefined {
  return store.use().get(luaPaneKey(plugin, id));
}
