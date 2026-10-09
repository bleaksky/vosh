import { describe, expect, it } from 'vitest';
import helpWindow from '../help/HelpWindow.tsx?raw';
import settingsWindow from '../settings/SettingsWindow.tsx?raw';
import appShell from '../shell/AppShell.tsx?raw';
import snoopWindow from '../shell/SnoopWindow.tsx?raw';
import baseCss from './base.css?raw';
import controlsCss from './controls.css?raw';

// The button recipe and the window edge. The focus ring 2 px wide and
// 2 px out, the danger button on the plain ring, the small button 24
// tall with 12 px words and 10 px sides, and the 1 px edge every window
// root wears.

const bare = (text: string) => text.replace(/\/\*[\s\S]*?\*\//g, '');

/** The declarations of the rule whose selector list is exactly
 *  `selector`, one per line, without the braces. */
function rule(css: string, selector: string): string[] {
  const text = bare(css);
  const at = text.indexOf(`\n${selector} {`);
  expect(at, selector).toBeGreaterThanOrEqual(0);
  const body = text.slice(text.indexOf('{', at) + 1, text.indexOf('}', at));
  return body
    .split(';')
    .map((line) => line.trim())
    .filter(Boolean);
}

describe('the button recipe', () => {
  it('rings a focused button 2 px wide, 2 px out, in the accent', () => {
    expect(rule(controlsCss, '.btn:focus-visible')).toEqual([
      'outline: 2px solid var(--accent)',
      'outline-offset: 2px',
    ]);
  });

  it('keeps the plain ring on a danger button and only turns its words', () => {
    expect(rule(controlsCss, '.btn')).toContain('box-shadow: inset 0 0 0 1px var(--sep)');
    expect(rule(controlsCss, '.btn.is-danger')).toEqual(['color: var(--danger-text)']);
  });

  it('draws a small button 24 tall with 12 px words and 10 px sides', () => {
    const small = rule(controlsCss, '.btn.is-small');
    expect(small).toContain('height: 24px');
    expect(small).toContain('padding: 0 10px');
    expect(small).toContain('font-size: 12px');
  });
});

describe('the window edge', () => {
  it('draws a 1 px edge over the window, under its rounded corners', () => {
    const edge = rule(baseCss, '.window-edge::after');
    expect(edge).toContain('position: absolute');
    expect(edge).toContain('inset: 0');
    expect(edge).toContain('border-radius: inherit');
    expect(edge).toContain('box-shadow: inset 0 0 0 1px var(--edge)');
    expect(edge).toContain('pointer-events: none');
  });

  it('leaves the corners to the macOS frame and square on Windows, and the rim to the macOS frame', () => {
    expect(
      rule(
        baseCss,
        "[data-platform='macos'] .window-edge,\n[data-platform='windows'] .window-edge",
      ),
    ).toEqual(['border-radius: 0']);
    expect(rule(baseCss, "[data-platform='macos'] .window-edge::after")).toEqual(['display: none']);
  });

  it('is worn by the main, Settings, Help and snoop windows', () => {
    expect(appShell).toContain('className="shell window-edge"');
    expect(settingsWindow).toContain('className="st-app window-edge"');
    expect(helpWindow).toContain('className="st-app hp-app window-edge"');
    expect(snoopWindow).toContain('className="snoop-window window-edge"');
  });
});
