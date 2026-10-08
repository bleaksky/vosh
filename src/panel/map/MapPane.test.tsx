import { act, createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { roomNameColor } from './roomName';
import {
  parseRoomInfo,
  type RoomInfo,
  type RoomInfoBase,
  type RoomPerson,
} from '../../stores/gmcp/roomStore';
import { findTheme, themeTokens } from '../../theme/themes';
import mapCss from '../../styles/map.css?raw';
import panelCss from '../../styles/panel.css?raw';
import { aabahranMapPacket, aabahranPacket } from '../../test/aabahranGmcp';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../../test/fakeDom';
import { MapBandRows } from './MapPane';
import { getCell, offFloorLayers, type MapTilesPayload } from './mapTiles';

// The room store and the map view reach the Tauri bridge when they
// start. MapBandRows draws from plain room data and never calls it. The
// map view hears its packets through the fake event bus here.
type Handler = (event: { payload: unknown }) => void;
const bus = vi.hoisted(() => new Map<string, Set<(event: { payload: unknown }) => void>>());
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn((event: string, cb: Handler) => {
    let set = bus.get(event);
    if (!set) bus.set(event, (set = new Set()));
    set.add(cb);
    return Promise.resolve(() => set.delete(cb));
  }),
}));

const kansoTheme = findTheme('kanso-zen');
const rubricTheme = findTheme('rubric');
const kanso = themeTokens(kansoTheme);
const rubric = themeTokens(rubricTheme);

function room(data: Record<string, unknown>): RoomInfo {
  const base: RoomInfoBase | null = parseRoomInfo(data);
  if (!base) throw new Error('no room');
  return { ...base, areaVnum: null, areaColor: null };
}

// Rooms from the area files, as Room.Info on the new build sends them.
// Room 10874 in winsteel.are, Faction of Steel, an inside room with one
// exit west, in the Coastal North region area_regions.txt gives the
// area. The area file stores its name as `8Between Ice Bars``, so the
// terminal draws it in the theme's bright black.
const ICE_BARS = room({
  num: 10874,
  name: 'Between Ice Bars',
  area: 'Faction of Steel',
  terrain: 'inside',
  sector: 0,
  region: 1,
  climate: 'Coastal North',
  exits: { west: 10873 },
});
// Mob 10801 in winsteel.are, whose short text Room.Chars names.
const BARON: RoomPerson = { name: 'The Baron Helgardium', npc: true };

// Room 10720 in avalon.are, Tale of Avalon, a lake you cannot swim, as
// an older build sends it, with the terrain alone.
const MISTY_LAKE = room({
  num: 10720,
  name: 'A Misty Lake',
  area: 'Tale of Avalon',
  terrain: 'water_noswim',
  exits: { north: 10721, east: 10723, south: 10716, west: 10722 },
});

interface Row {
  className: string;
  color: string | null;
  title: string | null;
  html: string;
  text: string;
}

/** Each row the band draws, read back out of its markup. */
function band(
  info: RoomInfo | null,
  { people = [] as RoomPerson[], rows = 4, theme = kansoTheme } = {},
): Row[] {
  const html = renderToStaticMarkup(
    <ul>
      <MapBandRows
        info={info}
        people={people}
        rows={rows}
        palette={theme.xterm}
        ground={themeTokens(theme)}
      />
    </ul>,
  );
  return Array.from(html.matchAll(/<li class="([^"]*)">([\s\S]*?)<\/li>/g), ([, cls, body]) => ({
    className: cls,
    color: /<span class="pane-row-name"[^>]*style="color:([^";]+)"/.exec(body)?.[1] ?? null,
    title: /<span class="pane-row-name" title="([^"]*)"/.exec(body)?.[1] ?? null,
    html: body,
    text: body.replace(/<[^>]+>/g, ''),
  }));
}

/** The declarations of one rule in `css`, map.css unless named. */
function rule(selector: string, css = mapCss): string {
  const at = css.indexOf(`\n${selector} {`);
  expect(at, selector).toBeGreaterThanOrEqual(0);
  return css.slice(at, css.indexOf('}', at));
}

describe('the band under the map', () => {
  // The exits moved from the name row to the end of the terrain row, so
  // the room name has its whole row, as the theme boards draw it.
  it('names the room in its terminal color, with the terrain, region and exits under it', () => {
    const rows = band(ICE_BARS, { people: [BARON] });
    expect(rows.map((r) => r.className)).toEqual([
      'pane-row pane-map-room',
      'pane-row pane-map-where',
      'pane-row pane-map-person',
    ]);
    const [name, where, person] = rows;
    const ink = roomNameColor(0, kansoTheme.xterm, kanso);
    expect(name.html).toBe(
      `<span class="pane-row-name" title="Between Ice Bars" style="color:${ink}">` +
        'Between Ice Bars</span>',
    );
    expect(where.html).toBe(
      '<span class="pane-map-terrain">Inside</span>' +
        '<span class="pane-map-region" title="Coastal North">' +
        '<span class="pane-map-sep" aria-hidden="true">·</span>Coastal North</span>' +
        '<span class="pane-map-exits" title="west">west</span>',
    );
    expect(person.text).toBe('The Baron Helgardium');
  });

  it('draws the name from the theme the terminal draws it in', () => {
    const [onKanso] = band(ICE_BARS);
    const [onRubric] = band(ICE_BARS, { theme: rubricTheme });
    expect(onKanso.color).toBe(roomNameColor(0, kansoTheme.xterm, kanso));
    expect(onRubric.color).toBe(roomNameColor(0, rubricTheme.xterm, rubric));
    expect(onKanso.color).not.toBe(onRubric.color);
    // The 256 color tint do_look prints first never shows, since the
    // name's own code resets it.
    expect(onKanso.color).not.toBe('#eeeeee');
  });

  it('shows the terrain alone for an older build that sends no region', () => {
    const [name, where] = band(MISTY_LAKE);
    expect(name.color).toBe(roomNameColor(7, kansoTheme.xterm, kanso));
    expect(name.text).toBe('A Misty Lake');
    expect(where.html).toBe(
      '<span class="pane-map-terrain">Deep Water</span>' +
        '<span class="pane-map-exits" title="north east south west">north east south west</span>',
    );
  });

  it('keeps the made up rhapsody room in the text color with an empty terrain row', () => {
    const packet = aabahranPacket('room-info-rhapsody.gmcp');
    const [name, where] = band(room(packet.data as Record<string, unknown>));
    expect(name.title).toBe('A Wondrous Place');
    expect(name.color).toBeNull();
    expect(where.html).toBe('');
  });

  it('holds both room rows empty before the first Room.Info', () => {
    expect(band(null).map((r) => [r.className, r.text])).toEqual([
      ['pane-row pane-map-room', ''],
      ['pane-row pane-map-where', ''],
    ]);
  });

  it('gives up people before the terrain row, and that row before the name', () => {
    const people = [BARON, { name: 'Tolliver', npc: false }];
    const classes = (rows: number) => band(ICE_BARS, { people, rows }).map((r) => r.className);
    expect(classes(4)).toEqual([
      'pane-row pane-map-room',
      'pane-row pane-map-where',
      'pane-row pane-map-person',
      'pane-row pane-map-person',
    ]);
    expect(classes(3)).toEqual([
      'pane-row pane-map-room',
      'pane-row pane-map-where',
      'pane-row pane-map-more',
    ]);
    expect(classes(2)).toEqual(['pane-row pane-map-room', 'pane-row pane-map-where']);
    expect(classes(1)).toEqual(['pane-row pane-map-room']);
  });

  it('keeps the exits beside the name when the band has no terrain row', () => {
    const [name] = band(ICE_BARS, { rows: 1 });
    expect(name.html).toContain(
      '</span><span class="pane-row-value pane-map-exits" title="west">west</span>',
    );
  });

  it('sets the terrain row in the quiet tier, and lets the region give way first', () => {
    expect(rule('.pane-map-where')).toContain('color: var(--tertiary);');
    // 11 px at a 12 px panel, scaled with the size (paneTextSize.test.ts).
    expect(rule('.pane-map-where')).toContain('font-size: round(11px * var(--mud-scale), 1px);');
    expect(rule('.pane-map-terrain')).toContain('flex: none;');
    const region = rule('.pane-map-region');
    expect(region).toContain('min-width: 0;');
    expect(region).toContain('text-overflow: ellipsis;');
    expect(region).toContain('white-space: nowrap;');
  });

  it('sets the exits at the right of the terrain row', () => {
    const exits = rule('.pane-map-where .pane-map-exits');
    expect(exits).toContain('margin-left: auto;');
    expect(exits).toContain('padding-left: 8px;');
    expect(exits).toContain('white-space: nowrap;');
    // On a narrow panel they give way with an ellipsis.
    expect(exits).toContain('min-width: 0;');
    expect(exits).toContain('text-overflow: ellipsis;');
  });

  it('keeps a floor under the room name when the exits sit beside it', () => {
    const exits = rule('.pane-map-room .pane-map-exits');
    expect(exits).toContain('max-width: calc(100% - 3em - 8px);');
    expect(exits).toContain('min-width: 0;');
    expect(exits).toContain('overflow: hidden;');
    expect(exits).toContain('text-overflow: ellipsis;');
    // The 8px is the gap the value class leaves before the exits.
    expect(rule('.pane-row-value', panelCss)).toContain('margin-left: 8px;');
  });

  it('fades the rooms the map box clips at its sides over 12 px', () => {
    const fade =
      'linear-gradient(90deg, transparent, #000 12px, #000 calc(100% - 12px), transparent);';
    expect(rule('.pane-map-box')).toContain(`mask-image: ${fade}`);
  });
});

// Click to walk, board 9 of the Scripts and Panels review. The map view
// draws in a fake DOM with no canvas, 404 by 300, so Squares at zoom 1
// puts the room at [row][col] of the Val Miran packet at (2 + 20 col,
// -50 + 20 row), and you stand in The Central Square at [10][10].
describe('a walk on the map', () => {
  const doc = new FakeDocument();
  const WIDTH = 404;
  const HEIGHT = 300;
  const VAL_MIRAN = aabahranMapPacket('val-miran-central-square.gmcp').data as MapTilesPayload;
  /** The listeners each element of the drawing added. */
  const heard = new WeakMap<object, Map<string, Set<(e: unknown) => void>>>();
  const patched: [string, PropertyDescriptor | undefined][] = [];

  function patch(name: string, value: PropertyDescriptor) {
    patched.push([name, Object.getOwnPropertyDescriptor(FakeElement.prototype, name)]);
    Object.defineProperty(FakeElement.prototype, name, { configurable: true, ...value });
  }

  beforeAll(() => {
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    vi.stubGlobal('document', doc);
    vi.stubGlobal('window', {
      document: doc,
      devicePixelRatio: 1,
      location: { protocol: 'about:' },
      HTMLIFrameElement: class {},
      addEventListener() {},
      removeEventListener() {},
      setTimeout: (fn: () => void, ms: number) => setTimeout(fn, ms),
      clearTimeout: (id: ReturnType<typeof setTimeout>) => clearTimeout(id),
    });
    vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
    vi.stubGlobal('Node', FakeNode);
    vi.stubGlobal('Element', FakeElement);
    vi.stubGlobal('HTMLElement', FakeElement);
    vi.stubGlobal('getComputedStyle', () => ({ getPropertyValue: () => '', minHeight: '' }));
    vi.stubGlobal('localStorage', { getItem: () => null, setItem() {}, removeItem() {} });
    vi.stubGlobal('requestAnimationFrame', () => 0);
    vi.stubGlobal('cancelAnimationFrame', () => undefined);
    vi.stubGlobal(
      'ResizeObserver',
      class {
        observe() {}
        disconnect() {}
      },
    );
    vi.stubGlobal(
      'MutationObserver',
      class {
        observe() {}
        disconnect() {}
      },
    );
    patch('getContext', { value: () => null });
    patch('clientWidth', { get: () => WIDTH });
    patch('clientHeight', { get: () => HEIGHT });
    patch('getBoundingClientRect', { value: () => ({ left: 0, top: 0 }) });
    patch('addEventListener', {
      value(this: object, type: string, fn: (e: unknown) => void) {
        const byType = heard.get(this) ?? new Map<string, Set<(e: unknown) => void>>();
        heard.set(this, byType);
        const set = byType.get(type) ?? new Set();
        byType.set(type, set.add(fn));
      },
    });
    patch('removeEventListener', {
      value(this: object, type: string, fn: (e: unknown) => void) {
        heard.get(this)?.get(type)?.delete(fn);
      },
    });
  });

  afterAll(() => {
    for (const [name, was] of patched.reverse()) {
      if (was) Object.defineProperty(FakeElement.prototype, name, was);
      else delete (FakeElement.prototype as unknown as Record<string, unknown>)[name];
    }
    vi.unstubAllGlobals();
  });

  const cleanups: (() => Promise<void>)[] = [];
  afterEach(async () => {
    for (const clean of cleanups.splice(0)) await clean();
  });

  /** Send a GMCP package to the first session, as the backend does. */
  function gmcp(name: string, data: unknown) {
    for (const cb of bus.get(`session://gmcp/${name.replace(/\./g, '-')}`) ?? []) {
      cb({ payload: { session: 1, data } });
    }
  }

  /** Draw the map over the Val Miran packet, standing in room 20605. */
  async function map() {
    const { MapView } = await import('./MapView');
    const { startRoomStore } = await import('../../stores/gmcp/roomStore');
    startRoomStore();
    const container = doc.createElement('div');
    const root = createRoot(container as unknown as HTMLElement);
    await act(async () => {
      root.render(createElement(MapView));
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    await act(async () => {
      gmcp('Map.Tiles', VAL_MIRAN);
      gmcp('Room.Info', { num: 20605, name: 'The Central Square of Val Miran', exits: {} });
    });
    cleanups.push(async () => {
      await act(async () => root.unmount());
    });
    const classOf = (el: FakeElement) => el.getAttribute('class') ?? '';
    const [host] = findAll(container, (el) => classOf(el).startsWith('map-canvas-host'));
    const fire = async (type: string, x: number, y: number) =>
      act(async () => {
        const event = { type, button: 0, pointerId: 1, clientX: x, clientY: y, target: null };
        for (const fn of heard.get(host)?.get(type) ?? []) fn(event);
      });
    /** The room at [row][col] on the canvas. */
    const at = (row: number, col: number): [number, number] => [2 + 20 * col, -50 + 20 * row];
    return {
      hover: (row: number, col: number) => fire('pointermove', ...at(row, col)),
      leave: () => fire('pointerleave', 0, 0),
      click: async (row: number, col: number) => {
        await fire('pointerdown', ...at(row, col));
        await fire('pointerup', ...at(row, col));
      },
      /** The tip's words and its meta, or null with no tip. */
      tip: () => {
        const [tip] = findAll(container, (el) => classOf(el).startsWith('walk-tip'));
        if (!tip) return null;
        const part = (name: string) =>
          findAll(tip, (el) => classOf(el).startsWith(name))[0]?.textContent;
        return { msg: part('ov-toast-msg'), meta: part('ov-toast-meta') };
      },
    };
  }

  let createRoot: typeof import('react-dom/client').createRoot;
  beforeAll(async () => {
    ({ createRoot } = await import('react-dom/client'));
  });

  /** A room on another floor whose cell on yours is empty. */
  function otherFloor(): [number, number] {
    for (const entry of offFloorLayers(VAL_MIRAN).flat()) {
      if (!getCell(VAL_MIRAN, entry.y, entry.x)) return [entry.y, entry.x];
    }
    throw new Error('no room on another floor');
  }

  it('offers the walk to a room you can reach as a tip with its steps', async () => {
    const view = await map();
    expect(view.tip()).toBeNull();
    await view.hover(6, 12);
    expect(view.tip()).toEqual({ msg: 'Walk 6 steps', meta: '4n2e' });
    await view.hover(10, 11);
    expect(view.tip()).toEqual({ msg: 'Walk 1 step', meta: 'e' });
    await view.leave();
    expect(view.tip()).toBeNull();
  });

  it('says Walk to the door for a room behind a locked door', async () => {
    const view = await map();
    await view.hover(7, 15);
    expect(view.tip()).toEqual({ msg: 'Walk to the door', meta: '4n5e' });
  });

  /** Each walk the map sent the session. */
  async function routes() {
    const { invoke } = await import('@tauri-apps/api/core');
    return vi
      .mocked(invoke)
      .mock.calls.filter(([cmd]) => cmd === 'session_walk_route')
      .map(([, args]) => args);
  }

  it('walks a click from the room you stand in, through the rooms ex names', async () => {
    const view = await map();
    const before = (await routes()).length;
    await view.click(6, 12);
    expect((await routes()).slice(before)).toEqual([
      {
        steps: '4n2e',
        start: 20605,
        rooms: [20604, 20603, 20602, 20601, 20653, 20652],
        session: 1,
      },
    ]);
    const { getWalk } = await import('../../stores/session/walkStore');
    expect(getWalk().route?.target).toEqual({ row: 6, col: 12 });
  });

  it('walks a click past a locked door as far as the door', async () => {
    const view = await map();
    const before = (await routes()).length;
    await view.click(7, 15);
    expect((await routes()).slice(before).map((r) => (r as { steps: string }).steps)).toEqual([
      '4n5e',
    ]);
  });

  it('sends nothing for a click on your own room or a room on another floor', async () => {
    const view = await map();
    const before = (await routes()).length;
    await view.click(10, 10);
    await view.click(...otherFloor());
    expect((await routes()).length).toBe(before);
  });

  it('offers nothing over your own room or a room on another floor', async () => {
    const view = await map();
    await view.hover(10, 10);
    expect(view.tip()).toBeNull();
    await view.hover(...otherFloor());
    expect(view.tip()).toBeNull();
  });
});
