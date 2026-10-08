import { describe, expect, it } from 'vitest';
import inputCss from './input.css?raw';
import settingsCss from './settings.css?raw';
import tokensCss from './tokens.css?raw';

// One caret geometry. The command line caret and the Settings picker
// samples are both one cell of your terminal font, 1ch by 1.2em to the
// pixel, read from two tokens.

const bare = (text: string) => text.replace(/\/\*[\s\S]*?\*\//g, '');

/** Every rule whose selector names `prefix`, with its body. */
function rules(text: string, prefix: string) {
  return [...bare(text).matchAll(/([^{}]+)\{([^{}]*)\}/g)]
    .filter((m) => m[1].includes(prefix))
    .map((m) => ({ selector: m[1].trim(), body: m[2] }));
}

/** One declaration's value in a rule body. */
const prop = (body: string, name: string) =>
  body.match(new RegExp(`(?:^|[;\\s])${name}:\\s*([^;]+);`))?.[1].trim();

describe('the caret cell', () => {
  it('is 1ch by 1.2em to the pixel in tokens.css', () => {
    const tokens = bare(tokensCss);
    expect(prop(tokens, '--caret-w')).toBe('1ch');
    expect(prop(tokens, '--caret-h')).toBe('round(1.2em, 1px)');
  });

  it('sizes the command line caret and the picker sample', () => {
    const caret = rules(inputCss, '.input-caret').find((r) => r.selector === '.input-caret');
    const sample = rules(settingsCss, '.st-caret').find((r) => r.selector === '.st-caret');
    for (const rule of [caret, sample]) {
      expect(prop(rule?.body ?? '', 'width')).toBe('var(--caret-w)');
      expect(prop(rule?.body ?? '', 'height')).toBe('var(--caret-h)');
    }
  });

  it('draws the picker sample in your terminal font at your size', () => {
    const sample = rules(settingsCss, '.st-caret').find((r) => r.selector === '.st-caret');
    expect(prop(sample?.body ?? '', 'font-family')).toBe('var(--app-font-family)');
    expect(prop(sample?.body ?? '', 'font-size')).toBe('var(--app-font-size)');
  });

  it('keeps no fixed caret size', () => {
    const all = [
      ...rules(inputCss, '.input-caret'),
      ...rules(inputCss, '.caret-shape--'),
      ...rules(settingsCss, '.st-caret'),
    ];
    expect(all.length).toBeGreaterThan(14);
    for (const { selector, body } of all) {
      expect(body, selector).not.toMatch(/(?<![\d.])(7|7\.8|15|17|8\.5)px/);
    }
  });
});

// The look you pick. The row keeps the caret and text colors in two
// tokens that start at the theme accent and text, and the caret, its
// outline and the password caret read them.

describe('the command line look', () => {
  const rule = (selector: string) =>
    rules(inputCss, selector).find((r) => r.selector === selector)?.body ?? '';

  it('starts the caret at the accent and the text at the theme text', () => {
    expect(prop(rule('.input-row'), '--caret')).toBe('var(--accent)');
    expect(prop(rule('.input-row'), '--line-text')).toBe('var(--text)');
    expect(prop(rule('.input-row'), 'caret-color')).toBe('var(--caret)');
    expect(prop(rule('.input-row'), 'background')).toBe('var(--inputband)');
  });

  it('draws the caret and its outline in --caret', () => {
    expect(prop(rule('.input-caret'), 'background')).toBe('var(--caret)');
    expect(prop(rule('.caret-shape--block_outline'), 'border')).toBe('1px solid var(--caret)');
  });

  it('draws what you type in --line-text', () => {
    expect(prop(rule('.input-row textarea'), 'color')).toBe('var(--line-text)');
    expect(prop(rule('.input-row input'), 'color')).toBe('var(--line-text)');
  });

  it('tints the band with 7 percent of the accent, or takes your own color', () => {
    expect(prop(rule('.input-row.is-tint'), 'background')).toBe(
      'color-mix(in srgb, var(--accent) 7%, var(--inputband))',
    );
    expect(prop(rule('.input-row.is-own'), 'background')).toBe(
      'var(--line-ground, var(--inputband))',
    );
  });

  it('holds the caret steady with Caret blinks off, and with Reduce motion', () => {
    expect(prop(rule('.input-caret.is-steady'), 'animation')).toBe('none');
    expect(bare(inputCss)).toMatch(
      /@media \(prefers-reduced-motion: reduce\) \{\s*\.input-caret \{\s*animation: none;/,
    );
  });
});
