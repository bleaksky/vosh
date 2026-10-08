import { describe, expect, it } from 'vitest';
import tokensCss from './tokens.css?raw';

// The seven z-index layers (board 07 of the merges review, Q15). Every
// z-index of 10 or more reads a --z- token, so what paints over what is
// one table in tokens.css. Values 1 to 9 only order a component's own
// children and stay plain numbers.

const sheets = import.meta.glob<string>('../**/*.css', {
  query: '?raw',
  import: 'default',
  eager: true,
});

const bare = (text: string) => text.replace(/\/\*[\s\S]*?\*\//g, '');

/** The writing box's ring and grip order the box's own children. */
const KEPT = new Set(['.wr-box::after', '.wr-box-grip']);

/** Every rule with a z-index across the sheets, its selector and value. */
const layered = Object.entries(sheets).flatMap(([file, text]) =>
  [...bare(text).matchAll(/([^{}]+)\{([^{}]*)\}/g)].flatMap((m) => {
    const value = m[2].match(/(?:^|[;\s])z-index:\s*([^;]+);/)?.[1].trim();
    return value ? [{ file, selector: m[1].trim(), value }] : [];
  }),
);

/** A layer token's value in tokens.css. */
function token(name: string): number {
  const found = bare(tokensCss).match(new RegExp(`${name}:\\s*(\\d+);`));
  expect(found, name).not.toBeNull();
  return Number(found?.[1]);
}

/** The z-index one selector paints at, its token and step resolved. */
function z(selector: string): number {
  const found = layered.find((r) => r.selector === selector);
  expect(found, selector).toBeDefined();
  const value = found?.value ?? '';
  const named = value.match(/^(?:calc\()?var\((--z-[a-z-]+)\)(?:\s*\+\s*(\d+)\))?$/);
  return named ? token(named[1]) + Number(named[2] ?? 0) : Number(value);
}

describe('the z-index layers', () => {
  it('reads every sheet', () => {
    expect(Object.keys(sheets).length).toBeGreaterThan(10);
    expect(layered.length).toBeGreaterThan(15);
  });

  it('pins the seven layers', () => {
    expect(token('--z-overlay')).toBe(30);
    expect(token('--z-in-pane')).toBe(40);
    expect(token('--z-edge')).toBe(60);
    expect(token('--z-card')).toBe(800);
    expect(token('--z-menu')).toBe(1000);
    expect(token('--z-coach')).toBe(1050);
    expect(token('--z-dialog')).toBe(1100);
  });

  it('puts every z-index of 10 or more on a layer token', () => {
    const loose = layered.filter(
      (r) => /^\d+$/.test(r.value) && Number(r.value) >= 10 && !KEPT.has(r.selector),
    );
    expect(loose.map((r) => `${r.file} ${r.selector} ${r.value}`)).toEqual([]);
  });

  it('keeps the writing box ring and grip as plain numbers inside the box', () => {
    expect(z('.wr-box::after')).toBe(201);
    expect(z('.wr-box-grip')).toBe(202);
  });

  it('paints a menu over the prompt card and the card over its marks', () => {
    expect(z('.menu')).toBeGreaterThan(z('.pc-card'));
    expect(z('.pc-card')).toBeGreaterThan(z('.pc-marks'));
    expect(z('.pc-marks')).toBeGreaterThan(z('.shell-sessions-card'));
  });

  it('puts the find bar and the depth chip on one layer', () => {
    expect(z('.ov-find')).toBe(z('.ov-depth'));
  });

  it('draws the window edge under every card', () => {
    expect(z('.window-edge::after')).toBeLessThan(z('.shell-sessions-card'));
    expect(z('.window-edge::after')).toBeLessThan(z('.pc-card'));
    expect(z('.window-edge::after')).toBe(z('.ov-corner'));
  });

  it('keeps the sessions toggle over the sidebar that slides in', () => {
    expect(z('.shell-lead')).toBe(z('.shell-sessions-overlay') + 1);
  });
});
