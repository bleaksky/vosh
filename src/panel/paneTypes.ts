import type { PluginRow } from '../ipc/scripts';
import { getImmState } from '../stores/gmcp/immStore';
import { getLuaPanes, type LuaPanes } from '../stores/session/luaPanesStore';
import { getPluginRows } from '../stores/session/pluginRowsStore';
import {
  LUA_PANE,
  PANE_TYPES,
  allPanes,
  countPanes,
  paneCap,
  paneKey,
  paneRef,
  type PaneLeaf,
  type PaneRef,
  type OfferedPaneType,
  type PaneSplit,
  type PaneType,
  isOfferedPaneType,
} from './paneLayout';

// Names for each pane type, shared by the pane headers, the pane menu,
// and the title band's Add a pane menu so all three say the same thing.

export const PANE_LABELS: Record<PaneType, string> = {
  map: 'Map',
  affects: 'Affects',
  group: 'Group',
  chat: 'Chat',
  imm: 'Staff queues',
  writing: 'Writing',
};

/** The name a pane goes by: its type's for a built-in pane, and for a
 *  Lua pane the last title it showed, or its id before it shows one. */
export function paneLabel(ref: PaneRef): string {
  if (ref.pane !== LUA_PANE) return PANE_LABELS[ref.pane];
  const title = ref.props.title?.trim() ?? '';
  return title.length > 0 ? title : (ref.props.id ?? '');
}

/** Pane types this session offers. The staff queues pane only shows
 *  up once the server has sent Imm.Queues, which it does for
 *  immortals alone. */
export function offeredPaneTypes(): OfferedPaneType[] {
  const staff = getImmState().received;
  return PANE_TYPES.filter(isOfferedPaneType).filter((t) => t !== 'imm' || staff);
}

// Whether the tree has room for another pane of type `t`: one it does
// not show yet, or another Chat while fewer than four show.
function hasRoomFor(tree: PaneSplit | null, t: PaneType): boolean {
  const ref = paneRef(t);
  return tree === null || countPanes(tree, ref) < paneCap(ref);
}

/** Offered pane types the tree has room for, in menu order: each one
 *  it does not show yet, and Chat while fewer than four show. */
export function paneTypesToAdd(tree: PaneSplit | null): OfferedPaneType[] {
  return offeredPaneTypes().filter((t) => hasRoomFor(tree, t));
}

/** A Lua pane the menus offer: the plugin that draws it, its id, and
 *  the title it shows now. */
export interface LuaPaneOffer {
  plugin: string;
  id: string;
  title: string;
}

/** The reference that places `offer` in the tree. Its props keep the
 *  title, so the pane can name itself while its plugin is not running. */
export function luaPaneRef({ plugin, id, title }: LuaPaneOffer): PaneRef {
  return { pane: LUA_PANE, props: { plugin, id, title } };
}

/** Lua panes the session in front offers, in title order: each pane
 *  its plugins draw while the plugin is on and Vosh has not stopped it.
 *  Reads the stores unless handed what they hold. */
export function offeredLuaPanes(
  panes: LuaPanes = getLuaPanes(),
  rows: PluginRow[] | null = getPluginRows(),
): LuaPaneOffer[] {
  const running = new Set((rows ?? []).filter((r) => r.on && !r.stopped).map((r) => r.name));
  return [...panes.values()]
    .filter((p) => running.has(p.plugin))
    .map(({ plugin, id, title }) => ({ plugin, id, title }))
    .sort(
      (a, b) =>
        paneLabel(luaPaneRef(a)).localeCompare(paneLabel(luaPaneRef(b))) ||
        a.plugin.localeCompare(b.plugin) ||
        a.id.localeCompare(b.id),
    );
}

/** Offered Lua panes the tree does not show yet, in menu order. */
export function luaPanesToAdd(
  tree: PaneSplit | null,
  offered: LuaPaneOffer[] = offeredLuaPanes(),
): LuaPaneOffer[] {
  const shown = new Set(tree ? allPanes(tree) : []);
  return offered.filter((o) => !shown.has(paneKey(luaPaneRef(o))));
}

/** What Show here instead offers in place of a pane. */
export interface PanesToShowInstead {
  builtIns: OfferedPaneType[];
  lua: PaneRef[];
}

/** The offered panes other than `leaf`, built-in and then Lua, in menu
 *  order. A pane the tree holds once moves here from where it shows,
 *  and Chat joins the Chat panes shown while fewer than four show. */
export function panesToShowInstead(leaf: PaneLeaf, tree: PaneSplit | null): PanesToShowInstead {
  const key = paneKey(leaf);
  return {
    builtIns: offeredPaneTypes().filter(
      (t) => t !== leaf.pane && (t !== 'chat' || hasRoomFor(tree, t)),
    ),
    lua: offeredLuaPanes()
      .map(luaPaneRef)
      .filter((ref) => paneKey(ref) !== key),
  };
}
