import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { ChipStyle } from '../../lib/session';
import { BUILTIN_THEMES, themeTokens } from '../../lib/themes';
import frameCss from '../../styles/frame.css?raw';
import { daylightTint } from './daylight';
import { StatusClock, type ClockMoons, type ClockTick, type ClockTime } from './StatusClock';

const HOURS = Array.from({ length: 24 }, (_, h) => h);
const STYLES: ChipStyle[] = ['value_only', 'caption_value', 'icon_value'];

function draw(
  style: ChipStyle,
  tick: ClockTick | null,
  time: ClockTime | null,
  moons: ClockMoons | null = null,
): string {
  return renderToStaticMarkup(<StatusClock style={style} tick={tick} time={time} moons={moons} />);
}

const SKY: ClockMoons = {
  moons: [
    { name: 'Lysenties', phase: 2, color: '#eceff4', label: 'Lysenties, half-lit and growing' },
    { name: 'Nercuros', phase: 5, color: '#8fbcbb', label: 'Nercuros, nearly full and fading' },
    { name: 'Dyphrities', phase: 1, color: '#bf616a', label: 'Dyphrities, a thin crescent' },
  ],
  alignment: null,
};

/** The moon labels in the order they draw. */
function moonLabels(html: string): string[] {
  return [...html.matchAll(/<svg class="shell-moon"[^>]*aria-label="([^"]*)"/g)].map((m) => m[1]);
}

/** The class lists of the readings, tick first. */
function readings(html: string): string[][] {
  return [...html.matchAll(/<span class="(shell-status-part[^"]*)"/g)].map((m) => m[1].split(' '));
}

/** The declarations of one rule in frame.css. */
function rule(selector: string): string {
  const at = frameCss.indexOf(`${selector} {`);
  expect(at, selector).toBeGreaterThanOrEqual(0);
  return frameCss.slice(at, frameCss.indexOf('}', at));
}

describe('StatusClock', () => {
  it('draws nothing while neither the tick nor the time is known', () => {
    expect(draw('value_only', null, null)).toBe('');
  });

  it('draws the tick first and the time after it', () => {
    const html = draw(
      'value_only',
      { secs: 14, warn: false, interval: 30 },
      { text: '8:42', tint: '#ebcb8b', daytime: true, hour: 8 },
    );
    expect(html.indexOf('14s')).toBeLessThan(html.indexOf('8:42'));
    expect(html).toContain('style="color:#ebcb8b"');
    expect(readings(html)).toEqual([['shell-status-part'], ['shell-status-part']]);
  });

  it('sets the warning tick on its own ground at every hour on every theme', () => {
    for (const theme of BUILTIN_THEMES) {
      const tokens = themeTokens(theme);
      for (const hour of HOURS) {
        const tint = daylightTint(hour, theme.xterm, tokens);
        for (const style of STYLES) {
          const html = draw(
            style,
            { secs: 27, warn: true, interval: 30 },
            { text: '12:00', tint, daytime: true, hour: 12 },
          );
          const [tick, time] = readings(html);
          const where = `${theme.id} at ${hour} in ${style}`;
          expect(tick, where).toContain('is-warn');
          expect(time, where).not.toContain('is-warn');
        }
      }
    }
  });

  it('gives the warn class a soft warn ground that does not move', () => {
    const ground = rule('.shell-status-part.is-warn');
    expect(ground).toMatch(/background:\s*color-mix\(in srgb, var\(--warn\) 14%, transparent\)/);
    expect(ground).toMatch(/border-radius:\s*4px/);
    expect(ground).toMatch(/padding:\s*0 4px/);
    expect(ground).toMatch(/margin:\s*0 -4px/);
    expect(ground).not.toMatch(/transition|animation/);
    expect(rule('.shell-statusline .is-warn')).toMatch(/color:\s*var\(--warn\)/);
  });

  it('keeps the tick plain outside the warn window', () => {
    const html = draw('icon_value', { secs: 3, warn: false, interval: 30 }, null);
    expect(readings(html)).toEqual([['shell-status-part']]);
  });

  it('draws the moons after the tick and the time, in the order given', () => {
    const html = draw(
      'value_only',
      { secs: 14, warn: false, interval: 30 },
      { text: '8:42', tint: null, daytime: true, hour: 8 },
      SKY,
    );
    expect(html.indexOf('14s')).toBeLessThan(html.indexOf('8:42'));
    expect(html.indexOf('8:42')).toBeLessThan(html.indexOf('<svg class="shell-moon"'));
    expect(moonLabels(html)).toEqual(SKY.moons.map((moon) => moon.label));
    expect(html).toContain('<title>Nercuros, nearly full and fading</title>');
    expect(html.match(/<svg class="shell-moon" width="14" height="14"/g)).toHaveLength(3);
  });

  it('shows the moons caption only in the Caption style and says it in all three', () => {
    expect(draw('caption_value', null, null, SKY)).toContain('<span>Moons</span>');
    for (const style of ['value_only', 'icon_value'] as const) {
      const html = draw(style, null, null, SKY);
      expect(html).toContain('<span class="shell-sr">Moons</span>');
      expect(moonLabels(html)).toHaveLength(3);
    }
  });

  it('draws nothing for an empty sky', () => {
    expect(draw('value_only', null, null, { moons: [], alignment: 'Triad' })).toBe('');
    const html = draw('value_only', { secs: 3, warn: false, interval: 30 }, null, {
      moons: [],
      alignment: null,
    });
    expect(html).not.toContain('Moons');
    expect(moonLabels(html)).toEqual([]);
  });

  it('puts the sky word after the moons in the warn tone', () => {
    const html = draw('icon_value', null, null, { ...SKY, alignment: 'Triad' });
    const word = html.indexOf('<span class="shell-status-alignment">Triad</span>');
    expect(word).toBeGreaterThan(html.lastIndexOf('<svg class="shell-moon"'));
    expect(rule('.shell-status-alignment')).toMatch(/color:\s*var\(--warn\)/);
    expect(draw('icon_value', null, null, SKY)).not.toContain('shell-status-alignment');
  });

  it('spaces the moons 4 px apart inside the 8 px item', () => {
    expect(rule('.shell-status-moons')).toMatch(/gap:\s*4px/);
    expect(rule('.shell-status-clock')).toMatch(/gap:\s*8px/);
  });
});

describe('StatusClock Value and Caption styles', () => {
  const tick: ClockTick = { secs: 14, warn: false, interval: 30 };
  const time: ClockTime = { text: '8:42', tint: '#ebcb8b', daytime: true, hour: 8 };

  it('shows each value alone in the Value style, with no icon', () => {
    expect(draw('value_only', tick, time)).toBe(
      '<span class="shell-status-clock">' +
        '<span class="shell-status-part"><span class="shell-sr">Tick</span>' +
        '<span class="shell-status-value">14s</span></span>' +
        '<span class="shell-status-part"><span class="shell-sr">Time</span>' +
        '<span class="shell-status-value" style="color:#ebcb8b">8:42</span></span>' +
        '</span>',
    );
  });

  it('puts the caption before each value in the Caption style, with no icon', () => {
    expect(draw('caption_value', { ...tick, secs: 27, warn: true }, time)).toBe(
      '<span class="shell-status-clock">' +
        '<span class="shell-status-part is-warn"><span>Tick</span>' +
        '<span class="shell-status-value">27s</span></span>' +
        '<span class="shell-status-part"><span>Time</span>' +
        '<span class="shell-status-value" style="color:#ebcb8b">8:42</span></span>' +
        '</span>',
    );
  });
});

/** The 12 px glyph in the reading at `index`, tick first. */
function glyph(html: string, index: number): string | null {
  const parts = html.split('<span class="shell-status-part').slice(1);
  const part = parts[index] ?? '';
  return /^[^>]*><svg [\s\S]*?<\/svg>/.exec(part)?.[0].replace(/^[^>]*>/, '') ?? null;
}

const GLYPH_OPEN =
  '<svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" ' +
  'stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">';
const KEEP = ' vector-effect="non-scaling-stroke"';
const TICK_TRACK = `<circle cx="8" cy="8" r="5.75" stroke-opacity="0.35"${KEEP}></circle>`;

describe('StatusClock tick ring', () => {
  it('draws the faint ring and the arc gone so far before the tick', () => {
    const html = draw('icon_value', { secs: 15, warn: false, interval: 60 }, null);
    expect(glyph(html, 0)).toBe(
      `${GLYPH_OPEN}${TICK_TRACK}<path d="M8 2.25A5.75 5.75 0 0 1 13.75 8"${KEEP}></path></svg>`,
    );
    expect(html.indexOf('<svg')).toBeLessThan(html.indexOf('<span class="shell-sr">Tick</span>'));
    expect(html.indexOf('<span class="shell-sr">Tick</span>')).toBeLessThan(html.indexOf('15s'));
  });

  it('fills the ring against the interval you set', () => {
    const ring = (secs: number, interval: number | null) =>
      glyph(draw('icon_value', { secs, warn: false, interval }, null), 0);
    expect(ring(0, 30)).toBe(`${GLYPH_OPEN}${TICK_TRACK}</svg>`);
    expect(ring(15, 30)).toContain('<path d="M8 2.25A5.75 5.75 0 0 1 8 13.75"');
    expect(ring(30, 30)).toBe(
      `${GLYPH_OPEN}${TICK_TRACK}<circle cx="8" cy="8" r="5.75"${KEEP}></circle></svg>`,
    );
    expect(ring(34, 30)).toBe(ring(30, 30));
  });

  it('draws the whole circle, not an empty ring, in the last second of a long interval', () => {
    const ring = (secs: number, interval: number) =>
      glyph(draw('icon_value', { secs, warn: false, interval }, null), 0);
    expect(ring(9999, 10000)).toBe(ring(10000, 10000));
    expect(ring(9999, 10000)).not.toContain('<path');
    expect(ring(1, 10000)).toBe(ring(0, 10000));
  });

  it('draws the faint ring alone while the interval is unknown', () => {
    const empty = `${GLYPH_OPEN}${TICK_TRACK}</svg>`;
    expect(glyph(draw('icon_value', { secs: 14, warn: false, interval: null }, null), 0)).toBe(
      empty,
    );
    expect(glyph(draw('icon_value', { secs: 14, warn: false, interval: 0 }, null), 0)).toBe(empty);
  });

  it('draws no stopwatch', () => {
    const html = draw('icon_value', { secs: 14, warn: false, interval: 30 }, null);
    expect(html).not.toContain('M6.25 1.75h3.5');
    expect(html.match(/<svg /g)).toHaveLength(1);
  });

  it('gives the whole tick reading, ring and all, the warn tone in its last seconds', () => {
    const html = draw('icon_value', { secs: 57, warn: true, interval: 60 }, null);
    expect(html).toMatch(
      /^<span class="shell-status-clock"><span class="shell-status-part is-warn"><svg /,
    );
    expect(glyph(html, 0)).toContain('stroke="currentColor"');
    expect(glyph(html, 0)).toContain('<path d="M8 2.25A5.75 5.75 0 1 1 6.22 2.53"');
    expect(rule('.shell-statusline .is-warn')).toMatch(/color:\s*var\(--warn\)/);
  });

  it('draws in the tertiary tone of the line outside the warn window', () => {
    const html = draw('icon_value', { secs: 3, warn: false, interval: 30 }, null);
    expect(html).toMatch(/^<span class="shell-status-clock"><span class="shell-status-part"><svg /);
    expect(rule('.shell-statusline')).toMatch(/color:\s*var\(--tertiary\)/);
  });

  it('keeps the caption for a screen reader', () => {
    const html = draw('icon_value', { secs: 3, warn: false, interval: 30 }, null);
    expect(html).toContain('<span class="shell-sr">Tick</span>');
    expect(html).toContain('aria-hidden="true"');
  });
});

const SUN_TRACK =
  `<path d="M2.5 10.5A5.5 5.5 0 0 1 13.5 10.5" stroke-opacity="0.35"${KEEP}></path>` +
  `<path d="M1.5 10.5h13"${KEEP}></path>`;
const SUN_UP_TOP = '<circle cx="8" cy="5" r="1.75" fill="currentColor" stroke="none"></circle>';
const SUN_DOWN = `<circle cx="8" cy="13.4" r="1.35"${KEEP}></circle>`;

/** The time reading's glyph in the Icon style. */
function sun(time: ClockTime): string | null {
  return glyph(draw('icon_value', null, time), 0);
}

describe('StatusClock sun path', () => {
  it('draws the horizon, the faint arc over it, and the sun at the top at midday', () => {
    expect(sun({ text: '12:00', tint: null, daytime: true, hour: 12 })).toBe(
      `${GLYPH_OPEN}${SUN_TRACK}${SUN_UP_TOP}</svg>`,
    );
  });

  it('sets the sun on the arc for the game hour', () => {
    expect(sun({ text: '5:00', tint: null, daytime: true, hour: 5 })).toContain(
      '<circle cx="2.53" cy="9.93" r="1.75" fill="currentColor" stroke="none"></circle>',
    );
    expect(sun({ text: '19:00', tint: null, daytime: true, hour: 19 })).toContain(
      '<circle cx="13.47" cy="9.93" r="1.75" fill="currentColor" stroke="none"></circle>',
    );
  });

  it('drops the sun under the horizon as an open dot while it is down', () => {
    expect(sun({ text: '20:00', tint: null, daytime: false, hour: 20 })).toBe(
      `${GLYPH_OPEN}${SUN_TRACK}${SUN_DOWN}</svg>`,
    );
    expect(sun({ text: '12:00', tint: null, daytime: false, hour: 12 })).toBe(
      `${GLYPH_OPEN}${SUN_TRACK}${SUN_DOWN}</svg>`,
    );
  });

  it('puts the sun at the top while it is up and the hour is unknown', () => {
    expect(sun({ text: '8:42', tint: null, daytime: true, hour: null })).toBe(
      `${GLYPH_OPEN}${SUN_TRACK}${SUN_UP_TOP}</svg>`,
    );
  });

  it('draws the horizon and the arc alone while neither is known', () => {
    expect(sun({ text: '8:42', tint: null, daytime: null, hour: null })).toBe(
      `${GLYPH_OPEN}${SUN_TRACK}</svg>`,
    );
  });

  it('draws no sun with rays and no moon before the time', () => {
    for (const daytime of [true, false, null]) {
      const html = sun({ text: '8:42', tint: null, daytime, hour: 8 }) ?? '';
      expect(html).not.toContain('M8 1.75v1.5');
      expect(html).not.toContain('M14.25 8.55');
    }
  });

  it('keeps the time tint on the value and the icon in the tertiary tone', () => {
    const html = draw('icon_value', null, {
      text: '12:00',
      tint: '#ebcb8b',
      daytime: true,
      hour: 12,
    });
    expect(html).toMatch(/^<span class="shell-status-clock"><span class="shell-status-part"><svg /);
    expect(html).toContain('<span class="shell-status-value" style="color:#ebcb8b">12:00</span>');
    expect(html).toContain('<span class="shell-sr">Time</span>');
  });

  it('draws the tick ring, then the sun path, each before its value', () => {
    const html = draw(
      'icon_value',
      { secs: 27, warn: true, interval: 30 },
      { text: '19:00', tint: null, daytime: true, hour: 19 },
    );
    expect(html.match(/<svg /g)).toHaveLength(2);
    expect(glyph(html, 0)).toContain(TICK_TRACK);
    expect(glyph(html, 1)).toContain(SUN_TRACK);
    expect(readings(html)).toEqual([['shell-status-part', 'is-warn'], ['shell-status-part']]);
    expect(html.indexOf('27s')).toBeLessThan(html.lastIndexOf('<svg '));
    expect(html.lastIndexOf('<svg ')).toBeLessThan(html.indexOf('19:00'));
    expect(html.match(/aria-hidden="true"/g)).toHaveLength(2);
  });
});
