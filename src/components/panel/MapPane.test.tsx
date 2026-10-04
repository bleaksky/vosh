import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { roomNameColor } from '../../lib/roomName';
import {
  parseRoomInfo,
  type RoomInfo,
  type RoomInfoBase,
  type RoomPerson,
} from '../../lib/stores/roomStore';
import { findTheme, themeTokens } from '../../lib/themes';
import panelCss from '../../styles/panel.css?raw';
import { aabahranPacket } from '../../test/aabahranGmcp';
import { MapBandRows } from './MapPane';

// The room store and the map view reach the Tauri bridge when they
// start. MapBandRows, under test, draws from plain room data and never
// calls it.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

const kansoTheme = findTheme('kanso-zen');
const vellumTheme = findTheme('vellum');
const kanso = themeTokens(kansoTheme);
const vellum = themeTokens(vellumTheme);

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

/** The declarations of one rule in panel.css. */
function rule(selector: string): string {
  const at = panelCss.indexOf(`\n${selector} {`);
  expect(at, selector).toBeGreaterThanOrEqual(0);
  return panelCss.slice(at, panelCss.indexOf('}', at));
}

describe('the band under the map', () => {
  it('names the room in its terminal color, with the terrain and region under it', () => {
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
        'Between Ice Bars</span>' +
        '<span class="pane-row-value pane-map-exits" title="west">west</span>',
    );
    expect(where.html).toBe(
      '<span class="pane-map-terrain">Inside</span>' +
        '<span class="pane-map-region" title="Coastal North">' +
        '<span class="pane-map-sep" aria-hidden="true">·</span>Coastal North</span>',
    );
    expect(person.text).toBe('The Baron Helgardium');
  });

  it('draws the name from the theme the terminal draws it in', () => {
    const [onKanso] = band(ICE_BARS);
    const [onVellum] = band(ICE_BARS, { theme: vellumTheme });
    expect(onKanso.color).toBe(roomNameColor(0, kansoTheme.xterm, kanso));
    expect(onVellum.color).toBe(roomNameColor(0, vellumTheme.xterm, vellum));
    expect(onKanso.color).not.toBe(onVellum.color);
    // The 256 color tint do_look prints first never shows, since the
    // name's own code resets it.
    expect(onKanso.color).not.toBe('#eeeeee');
  });

  it('shows the terrain alone for an older build that sends no region', () => {
    const [name, where] = band(MISTY_LAKE);
    expect(name.color).toBe(roomNameColor(7, kansoTheme.xterm, kanso));
    expect(name.text).toBe('A Misty Lakenorth east south west');
    expect(where.html).toBe('<span class="pane-map-terrain">Deep Water</span>');
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

  it('keeps a floor under the room name and lets the exits give way after it', () => {
    const exits = rule('.pane-map-exits');
    expect(exits).toContain('max-width: calc(100% - 3em - 8px);');
    expect(exits).toContain('min-width: 0;');
    expect(exits).toContain('overflow: hidden;');
    expect(exits).toContain('text-overflow: ellipsis;');
    // The 8px is the gap the value class leaves before the exits.
    expect(rule('.pane-row-value')).toContain('margin-left: 8px;');
  });
});
