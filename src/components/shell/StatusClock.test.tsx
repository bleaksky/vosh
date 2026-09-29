import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { ChipStyle } from '../../lib/session';
import { BUILTIN_THEMES, themeTokens } from '../../lib/themes';
import frameCss from '../../styles/frame.css?raw';
import { daylightTint } from './daylight';
import { StatusClock, type ClockTick, type ClockTime } from './StatusClock';

const HOURS = Array.from({ length: 24 }, (_, h) => h);
const STYLES: ChipStyle[] = ['value_only', 'caption_value', 'icon_value'];

function draw(style: ChipStyle, tick: ClockTick | null, time: ClockTime | null): string {
  return renderToStaticMarkup(<StatusClock style={style} tick={tick} time={time} />);
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
      { secs: 14, warn: false },
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
            { secs: 27, warn: true },
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
    const html = draw('icon_value', { secs: 3, warn: false }, null);
    expect(readings(html)).toEqual([['shell-status-part']]);
  });
});
