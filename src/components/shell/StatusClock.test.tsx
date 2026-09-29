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
      { text: '8:42', tint: '#ebcb8b', daytime: true },
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
            { text: '12:00', tint, daytime: true },
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
      { text: '8:42', tint: null, daytime: true },
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
  const time: ClockTime = { text: '8:42', tint: '#ebcb8b', daytime: true };

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
