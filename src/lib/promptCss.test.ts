import { describe, expect, it } from 'vitest';
import overlaysCss from '../styles/overlays.css?raw';
import promptCss from '../styles/prompt.css?raw';
import settingsCss from '../styles/settings.css?raw';
import tokensCss from '../styles/tokens.css?raw';

// prompt.css draws the prompt card, the bands, the pinned dock and the
// Settings Prompt section on One Window tokens only (section 8 of the
// prompt build spec): every color comes from a token, never a literal, so
// each theme and appearance gives the prompt its own colors.

const bare = (text: string) => text.replace(/\/\*[\s\S]*?\*\//g, '');

const defined = (text: string) =>
  new Set([...text.matchAll(/(--[a-z0-9-]+)\s*:/g)].map((m) => m[1]));

describe('prompt.css', () => {
  const prompt = bare(promptCss);

  it('reads the stylesheets whole', () => {
    for (const text of [promptCss, tokensCss, settingsCss, overlaysCss]) {
      expect(text.length).toBeGreaterThan(1000);
    }
  });

  it('names no color of its own', () => {
    expect(prompt.match(/#[0-9a-f]{3,8}\b|\b(?:rgba?|hsla?|oklch|color-mix)\(/gi)).toBeNull();
  });

  it('reads only the One Window tokens and the recipes it reuses', () => {
    // The tokens, then the Settings control fills (.st-controls) and the
    // overlay recipes the card reuses.
    const known = new Set([
      ...defined(bare(tokensCss)),
      ...defined(bare(settingsCss)),
      ...defined(bare(overlaysCss)),
    ]);
    const read = [...new Set([...prompt.matchAll(/var\((--[a-z0-9-]+)/g)].map((m) => m[1]))];
    expect(read.filter((name) => !known.has(name))).toEqual([]);
  });
});
