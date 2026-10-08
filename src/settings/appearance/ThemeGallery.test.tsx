import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { deltaEOk, parseHex } from '../../theme/color';
import { checks, seenBy, type ColorVision } from '../../theme/gameFit';
import { galleryThemes, themeThumb } from '../../theme/themeThumb';
import { BUILTIN_THEMES, customToAppTheme, findTheme } from '../../theme/themes';
import { ThemeGallery } from './ThemeGallery';

const custom = (id: string, label: string) =>
  customToAppTheme({ id, label, description: '', xterm: {}, chrome: {} });

/** Each radio's value and the caption its label reads. The thumbnail
 *  is hidden from assistive tech, so the caption is the radio's name. */
function radios(html: string): { value: string; name: string }[] {
  return [
    ...html.matchAll(/value="([^"]*)"[^>]*\/>.*?<span class="st-theme-name">([^<]*)<\/span>/g),
  ].map((m) => ({ value: m[1], name: m[2] }));
}

describe('ThemeGallery', () => {
  it('names every radio, a custom theme with a blank name included', () => {
    const themes = galleryThemes(BUILTIN_THEMES, [custom('nord-copy', ''), custom('dusk', '  ')]);
    const html = renderToStaticMarkup(
      <ThemeGallery themes={themes} selected="nord" onPick={() => {}} />,
    );
    const list = radios(html);
    expect(list).toHaveLength(themes.length);
    for (const radio of list) expect(radio.name.trim()).not.toBe('');
    expect(list.slice(-2)).toEqual([
      { value: 'nord-copy', name: 'nord-copy' },
      { value: 'dusk', name: 'dusk' },
    ]);
  });

  it('shows the High Contrast pair side by side', () => {
    const html = renderToStaticMarkup(
      <ThemeGallery themes={galleryThemes(BUILTIN_THEMES, [])} selected="nord" onPick={() => {}} />,
    );
    const list = radios(html).map((r) => r.value);
    const at = list.indexOf('high-contrast');
    expect(at).toBeGreaterThan(-1);
    expect(list[at + 1]).toBe('high-contrast-light');
    expect(radios(html)[at + 1].name).toBe('High Contrast Light');
  });
});

describe('the Vision preview', () => {
  const themes = galleryThemes(BUILTIN_THEMES, []);
  const draw = (vision?: ColorVision, withSwitch = true) =>
    renderToStaticMarkup(
      <ThemeGallery
        themes={themes}
        selected="triad"
        onPick={() => {}}
        vision={vision}
        onVision={withSwitch ? () => {} : undefined}
      />,
    );
  /** The colors each tile paints, its ground, panel, line, dot and three
   *  text bars, by theme id. */
  const tiles = (html: string) => {
    const out: Record<string, string[]> = {};
    for (const m of html.matchAll(/value="([^"]*)"[^>]*\/>(.*?)<span class="st-theme-name">/g)) {
      out[m[1]] = [...m[2].matchAll(/background:(#[0-9a-f]{6})/g)].map((c) => c[1]);
    }
    return out;
  };

  it('draws the Vision switch above the tiles, labeled, on the vision it shows', () => {
    const html = draw('deuteranopia');
    expect(html.indexOf('st-gallery-bar')).toBeLessThan(html.indexOf('st-gallery-grid'));
    const label = /<span id="([^"]+)" class="st-meta">Vision<\/span>/.exec(html);
    expect(label).not.toBeNull();
    expect(html).toContain(`role="group" aria-labelledby="${label?.[1]}"`);
    const segments = [
      ...html.matchAll(/<button type="button" class="st-seg-item" aria-pressed="(\w+)">(\w+)</g),
    ].map((m) => [m[2], m[1]]);
    expect(segments).toEqual([
      ['Typical', 'false'],
      ['Deuteranopia', 'true'],
      ['Protanopia', 'false'],
      ['Tritanopia', 'false'],
    ]);
    // Without a handler the gallery draws no switch.
    expect(draw(undefined, false)).not.toContain('st-gallery-bar');
  });

  it('paints the tiles as they are under Typical', () => {
    const typical = tiles(draw('typical'));
    expect(tiles(draw(undefined, false))).toEqual(typical);
    for (const theme of themes) {
      const t = themeThumb(theme);
      expect(typical[theme.id], theme.id).toEqual([
        t.bg,
        t.panel,
        t.sep,
        t.accent,
        t.text,
        t.text,
        t.text,
      ]);
    }
  });

  it('paints every tile as each vision sees it', () => {
    for (const vision of ['deuteranopia', 'protanopia', 'tritanopia'] as const) {
      const seen = tiles(draw(vision));
      expect(Object.keys(seen)).toHaveLength(themes.length);
      for (const theme of themes) {
        const t = themeThumb(theme);
        const want = [t.bg, t.panel, t.sep, t.accent, t.text, t.text, t.text].map((c) =>
          seenBy(c, vision),
        );
        expect(seen[theme.id], `${vision} ${theme.id}`).toEqual(want);
      }
    }
  });

  // The preview and the checks see through the same matrices. Two
  // colors the preview shows sit as far apart as the T7 check measures
  // them, less the rounding of each color to whole channels.
  it('sees through the matrices the game color checks measure with', () => {
    const pairs = [
      ['deuteranopia', 'T7 deutan red/green', 'red', 'green'],
      ['protanopia', 'T7 protan red/green', 'red', 'green'],
      ['deuteranopia', 'T7 deutan red/yellow', 'red', 'yellow'],
      ['tritanopia', 'T7 tritan cyan/green', 'cyan', 'green'],
    ] as const;
    for (const theme of BUILTIN_THEMES) {
      const p = theme.xterm;
      for (const [vision, id, a, b] of pairs) {
        const measured = checks(p).find((c) => c.id === id)?.value ?? NaN;
        const shown = deltaEOk(
          parseHex(seenBy(p[a], vision)) ?? { r: 0, g: 0, b: 0 },
          parseHex(seenBy(p[b], vision)) ?? { r: 0, g: 0, b: 0 },
        );
        expect(Math.abs(shown - measured), `${theme.id} ${id}`).toBeLessThan(0.6);
      }
    }
  });

  // Board 9 of the Themes review draws Triad as a deuteranope and a
  // protanope see it, from the review's own simulation.
  it('draws Triad as board 9 does', () => {
    const triad = findTheme('triad').xterm;
    const board: [string, string, string][] = [
      [triad.background, '#071021', '#061023'],
      [triad.red, '#b3a353', '#8d8255'],
      [triad.green, '#adacaa', '#bfbaa7'],
      [triad.yellow, '#e5d274', '#dcc869'],
      [triad.blue, '#8199d4', '#8ca3d7'],
      [triad.magenta, '#969fb8', '#8b99bc'],
      [triad.brightRed, '#cabe8d', '#b1a98d'],
      [triad.brightGreen, '#ebe8df', '#f9f2db'],
      [triad.brightCyan, '#c2d2ff', '#d5e0ff'],
    ];
    for (const [color, deutan, protan] of board) {
      expect(seenBy(color, 'deuteranopia'), color).toBe(deutan);
      expect(seenBy(color, 'protanopia'), color).toBe(protan);
    }
  });
});
