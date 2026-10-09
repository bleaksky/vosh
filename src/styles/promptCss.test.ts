import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { MenuButton } from '../prompt/MenuButton';
import controlsCss from './controls.css?raw';
import overlaysCss from './overlays.css?raw';
import promptCss from './prompt.css?raw';
import settingsCss from './settings.css?raw';
import tokensCss from './tokens.css?raw';

// prompt.css draws the prompt card, the bands and the pinned dock on
// the shared tokens only: every color comes from a token, never a
// literal, so each theme and appearance gives the prompt its own
// colors.

const bare = (text: string) => text.replace(/\/\*[\s\S]*?\*\//g, '');

const defined = (text: string) =>
  new Set([...text.matchAll(/(--[a-z0-9-]+)\s*:/g)].map((m) => m[1]));

describe('prompt.css', () => {
  const prompt = bare(promptCss);

  it('reads the stylesheets whole', () => {
    for (const text of [promptCss, tokensCss, controlsCss, settingsCss, overlaysCss]) {
      expect(text.length).toBeGreaterThan(1000);
    }
  });

  it('names no color of its own', () => {
    expect(prompt.match(/#[0-9a-f]{3,8}\b|\b(?:rgba?|hsla?|oklch|color-mix)\(/gi)).toBeNull();
  });

  it('reads only the One Window tokens and the recipes it reuses', () => {
    // The tokens, then the control fills in controls.css (.st-controls),
    // what settings.css defines, and the overlay recipes the card reuses.
    const known = new Set([
      ...defined(bare(tokensCss)),
      ...defined(bare(controlsCss)),
      ...defined(bare(settingsCss)),
      ...defined(bare(overlaysCss)),
    ]);
    const read = [...new Set([...prompt.matchAll(/var\((--[a-z0-9-]+)/g)].map((m) => m[1]))];
    expect(read.filter((name) => !known.has(name))).toEqual([]);
  });
});

describe('the foot under your design', () => {
  const prompt = bare(promptCss);
  /** The declarations of one rule in prompt.css. */
  const rule = (selector: string) => {
    const at = prompt.indexOf(`${selector} {`);
    expect(at, selector).toBeGreaterThanOrEqual(0);
    return prompt.slice(at, prompt.indexOf('}', at));
  };

  it('keeps the foot under your design one row 52 tall, with drawing on or off', () => {
    // The Preview menu took the place of the four segments, so the foot
    // fits one row and never wraps. 28 px controls sit centered in it.
    const foot = rule('.pc-foot');
    expect(foot).toContain('display: flex;');
    expect(foot).toContain('align-items: center;');
    expect(foot).toContain('height: 52px;');
    expect(foot).not.toContain('flex-wrap');
    expect(foot).not.toContain('min-height');
    // No rule lets any foot wrap or grow past the row.
    expect(prompt).not.toContain('.pc-foot.is-design');
    for (const at of prompt.matchAll(/\.pc-foot[\w.-]*[^{]*\{([^}]*)\}/g)) {
      expect(at[1]).not.toContain('flex-wrap');
      expect(at[1]).not.toMatch(/(?:^|[^-])height: auto/);
    }
    // The preview and Done hold the right end.
    const end = rule('.pc-foot-end');
    expect(end).toContain('margin-left: auto;');
    expect(end).toContain('flex: none;');
  });

  it('keeps only the chevron padding on the small menu buttons at the foot', () => {
    // The small button sets 24 tall at 12/500, so the rule names no face.
    const html = renderToStaticMarkup(
      createElement(MenuButton, {
        name: 'Preview',
        choices: [{ value: 'now', label: 'Now' }],
        value: 'now',
        place: 'below-end',
        onChange: () => {},
      }),
    );
    expect(html).toMatch(/<button[^>]*class="btn is-small pc-menu-button"/);
    const button = rule('.btn.pc-menu-button');
    expect(button).not.toContain('font-size');
    expect(button).toContain('padding: 0 6px 0 10px;');
    expect(rule(".pc-menu-button[aria-disabled='true']")).toContain('opacity: 0.45;');
    // Preview: reads in the secondary tone, the preview in the text color.
    expect(rule('.pc-menu-button-lead')).toContain('color: var(--secondary);');
    expect(rule('.pc-menu-button svg')).toContain('color: var(--secondary);');
  });
});
