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
import { cameraFor, project, roofAt, sceneOf } from './map3dScene';
import { DEFAULT_MAP_3D_VIEW, MAP_3D_VIEW_KEY, loadMap3dView } from './map3dView';
import { MAP_STYLE_KEY } from './mapStyle';

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
  /** What the map keeps in local storage. */
  const stored = new Map<string, string>();

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
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => stored.get(key) ?? null,
      setItem: (key: string, value: string) => stored.set(key, value),
      removeItem: (key: string) => stored.delete(key),
    });
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
    stored.clear();
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
    const { startWalkStore } = await import('../../stores/session/walkStore');
    startRoomStore();
    startWalkStore();
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
      // The walk store outlives the view, so each test starts idle.
      await progress({ kind: 'idle' });
      await act(async () => root.unmount());
    });
    const classOf = (el: FakeElement) => el.getAttribute('class') ?? '';
    const [host] = findAll(container, (el) => classOf(el).startsWith('map-canvas-host'));
    const fire = async (type: string, x: number, y: number, detail = 1) =>
      act(async () => {
        const event = {
          type,
          button: 0,
          pointerId: 1,
          clientX: x,
          clientY: y,
          detail,
          target: null,
          preventDefault: () => {},
        };
        for (const fn of heard.get(host)?.get(type) ?? []) fn(event);
      });
    /** The room at [row][col] on the canvas, the middle of its roof in
     *  3D. */
    const at = (row: number, col: number): [number, number] => {
      if (stored.get(MAP_STYLE_KEY) !== '3d') return [2 + 20 * col, -50 + 20 * row];
      const view = loadMap3dView(localStorage);
      const cam = cameraFor(WIDTH, HEIGHT, sceneOf(VAL_MIRAN, view.floors), view, 1);
      const p = project(cam, col, row, roofAt(0));
      return [p.x, p.y];
    };
    return {
      fire,
      at,
      hover: (row: number, col: number) => fire('pointermove', ...at(row, col)),
      leave: () => fire('pointerleave', 0, 0),
      click: async (row: number, col: number) => {
        await fire('pointerdown', ...at(row, col));
        await fire('pointerup', ...at(row, col));
      },
      /** The words in the chip at the bottom of the map, or null with
       *  none. */
      chip: () => {
        const [chip] = findAll(container, (el) => classOf(el).startsWith('walk-chip'));
        if (!chip) return null;
        return findAll(chip, (el) => /^ov-(update|toast)-(msg|meta)/.test(classOf(el))).map(
          (el) => el.textContent,
        );
      },
      /** Press the chip's Stop. */
      stop: async () => {
        const [chip] = findAll(container, (el) => classOf(el).startsWith('walk-chip'));
        const [button] = findAll(chip, (el) => el.tagName === 'BUTTON');
        const key = Object.keys(button).find((k) => k.startsWith('__reactProps$')) ?? '';
        const props = (button as unknown as Record<string, { onClick(): void }>)[key];
        await act(async () => props.onClick());
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

  /** Tell the page where the first session's walk stands, as the
   *  walker does. */
  async function progress(payload: object) {
    await act(async () => {
      for (const cb of bus.get('session://walk') ?? []) cb({ payload: { session: 1, ...payload } });
    });
  }

  it('shows the steps left and Stop while you walk, and clears on arrival', async () => {
    const view = await map();
    await view.click(6, 12);
    expect(view.chip()).toBeNull();
    await progress({ kind: 'walking', done: 2, total: 6, left: '2n2e', route: true });
    expect(view.chip()).toEqual(['4 steps left', '2n2e']);
    await progress({ kind: 'walking', done: 5, total: 6, left: 'e', route: true });
    expect(view.chip()).toEqual(['1 step left', 'e']);
    await progress({ kind: 'idle' });
    expect(view.chip()).toBeNull();
    const { getWalk } = await import('../../stores/session/walkStore');
    expect(getWalk().route).toBeNull();
  });

  it('stops the walk with Stop', async () => {
    const view = await map();
    await view.click(6, 12);
    await progress({ kind: 'walking', done: 2, total: 6, left: '2n2e', route: true });
    const { invoke } = await import('@tauri-apps/api/core');
    vi.mocked(invoke).mockClear();
    await view.stop();
    expect(vi.mocked(invoke).mock.calls).toEqual([['session_walk_stop', { session: 1 }]]);
  });

  it('says how far a stopped walk got while you stand on its route', async () => {
    const view = await map();
    await view.click(6, 12);
    await progress({ kind: 'stopped', done: 4, total: 6, why: 'lost_sight' });
    expect(view.chip()).toEqual(['Stopped after 4 of 6 steps']);
    await progress({ kind: 'stopped', done: 0, total: 6, why: 'lost_track' });
    expect(view.chip()).toEqual(['Stopped']);
  });

  it('plans a click mid walk from the room the step on its way lands in', async () => {
    const view = await map();
    await view.click(6, 12);
    await progress({ kind: 'walking', done: 0, total: 6, left: '4n2e', route: true });
    const before = (await routes()).length;
    await view.click(10, 11);
    // The step north to 20604 is on its way, and the walker lets the
    // click take over once it lands there.
    expect((await routes()).slice(before)).toEqual([
      { steps: 'se', start: 20604, rooms: [20605, 20610], session: 1 },
    ]);
    // A walk you typed plans from its first step left too.
    await progress({ kind: 'walking', done: 0, total: 2, left: '2n', route: false });
    await view.click(10, 11);
    expect((await routes()).slice(before + 1)).toEqual([
      { steps: 'se', start: 20604, rooms: [20605, 20610], session: 1 },
    ]);
  });

  it('walks once for a double click, and not again on the room the walk heads for', async () => {
    const view = await map();
    const before = (await routes()).length;
    // A double click presses twice, and the second mousedown counts 2.
    for (const detail of [1, 2]) {
      await view.fire('pointerdown', ...view.at(6, 12));
      await view.fire('mousedown', ...view.at(6, 12), detail);
      await view.fire('pointerup', ...view.at(6, 12));
    }
    await view.fire('dblclick', ...view.at(6, 12));
    expect((await routes()).length).toBe(before + 1);
    await progress({ kind: 'walking', done: 0, total: 6, left: '4n2e', route: true });
    await view.click(6, 12);
    expect((await routes()).length).toBe(before + 1);
  });

  it('says nothing of a stopped walk you typed', async () => {
    const view = await map();
    await progress({ kind: 'walking', done: 1, total: 3, left: '2w', route: false });
    expect(view.chip()).toEqual(['2 steps left', '2w']);
    await progress({ kind: 'stopped', done: 1, total: 3, why: 'plain' });
    expect(view.chip()).toBeNull();
  });

  describe('in 3D', () => {
    const turn = () => loadMap3dView(localStorage).turn;

    it('offers the walk to the room whose roof is under the pointer', async () => {
      stored.set(MAP_STYLE_KEY, '3d');
      const view = await map();
      await view.hover(6, 12);
      expect(view.tip()).toEqual({ msg: 'Walk 6 steps', meta: '4n2e' });
      await view.hover(10, 10);
      expect(view.tip()).toBeNull();
    });

    it('walks a press and release that moves 3 px', async () => {
      stored.set(MAP_STYLE_KEY, '3d');
      const view = await map();
      const before = (await routes()).length;
      const [x, y] = view.at(6, 12);
      await view.fire('pointerdown', x, y);
      await view.fire('pointermove', x + 3, y);
      await view.fire('pointerup', x + 3, y);
      expect((await routes()).slice(before).map((r) => (r as { steps: string }).steps)).toEqual([
        '4n2e',
      ]);
      expect(turn()).toBe(0);
    });

    it('walks once for a double click on a roof', async () => {
      stored.set(MAP_STYLE_KEY, '3d');
      const view = await map();
      const before = (await routes()).length;
      for (const detail of [1, 2]) {
        await view.fire('pointerdown', ...view.at(6, 12));
        await view.fire('mousedown', ...view.at(6, 12), detail);
        await view.fire('pointerup', ...view.at(6, 12));
      }
      await view.fire('dblclick', ...view.at(6, 12));
      expect((await routes()).slice(before).map((r) => (r as { steps: string }).steps)).toEqual([
        '4n2e',
      ]);
    });

    it('turns the map for a drag of 10 px and walks nowhere', async () => {
      stored.set(MAP_STYLE_KEY, '3d');
      const view = await map();
      const before = (await routes()).length;
      const [x, y] = view.at(6, 12);
      await view.fire('pointerdown', x, y);
      await view.fire('pointermove', x + 10, y);
      await view.fire('pointerup', x + 10, y);
      expect((await routes()).length).toBe(before);
      expect(turn()).toBe(5);
    });

    it('puts north back for a double click on bare ground, not on a room', async () => {
      stored.set(MAP_STYLE_KEY, '3d');
      stored.set(MAP_3D_VIEW_KEY, JSON.stringify({ ...DEFAULT_MAP_3D_VIEW, turn: 90 }));
      const view = await map();
      await view.fire('dblclick', ...view.at(10, 11));
      expect(turn()).toBe(90);
      await view.fire('dblclick', 3, 3);
      expect(turn()).toBe(0);
    });
  });
});
