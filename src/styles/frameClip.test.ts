import { describe, expect, it } from 'vitest';
import frameCss from './frame.css?raw';
import settingsCss from './settings.css?raw';

// A frame that clips with overflow hidden still scrolls from script,
// and a search hit once slid the whole Help window up that way. The
// Help, Settings, and main window frames clip with overflow clip
// instead wherever the web view knows it, so nothing can slide them.

/** The declarations of `selector` inside the clip @supports block. */
function clipped(css: string, selector: string): string | null {
  const at = css.indexOf('@supports (overflow: clip)');
  if (at < 0) return null;
  const block = css.slice(at, css.indexOf('\n}', at));
  const rule = block.match(new RegExp(`${selector.replace(/\./g, '\\.')} \\{([^}]*)\\}`));
  return rule ? rule[1] : null;
}

describe('window frames', () => {
  it('clips the Settings frame, which Help wears too', () => {
    expect(clipped(settingsCss, '.st-app')).toMatch(/overflow: clip;/);
  });

  it('clips the main window frame', () => {
    expect(clipped(frameCss, '.shell')).toMatch(/overflow: clip;/);
  });

  it('keeps overflow hidden on the Settings frame for a web view without clip', () => {
    const base = settingsCss.match(/\n\.st-app \{([^}]*)\}/);
    expect(base?.[1]).toMatch(/overflow: hidden;/);
  });

  it('keeps overflow hidden on the main window frame for a web view without clip', () => {
    const base = frameCss.match(/\n\.shell \{([^}]*)\}/);
    expect(base?.[1]).toMatch(/overflow: hidden;/);
  });
});
