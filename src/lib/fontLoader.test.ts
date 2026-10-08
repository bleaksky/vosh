import { beforeEach, describe, expect, it, vi } from 'vitest';
import fixture from '../../fixtures/font-stacks/cases.json';

type ConvertFileSrc = (filePath: string, protocol?: string) => string;
const calls: Parameters<ConvertFileSrc>[] = [];
let convert: ConvertFileSrc = () => '';
vi.mock('@tauri-apps/api/core', () => ({
  convertFileSrc: (filePath: string, protocol?: string) => {
    calls.push([filePath, protocol]);
    return convert(filePath, protocol);
  },
}));

const actual = await vi.importActual<typeof import('@tauri-apps/api/core')>('@tauri-apps/api/core');
const { fontFaceCss, fontUrl, renderFontStack } = await import('./fontLoader');

describe('font scheme URLs', () => {
  beforeEach(() => {
    calls.length = 0;
  });

  it('hands the raw family to convertFileSrc on the font scheme', () => {
    // Tauri encodes the family into the path itself, and the backend
    // reads the family from the path on every platform.
    convert = () => 'font://localhost/Fira%20Code';
    expect(fontUrl('Fira Code')).toBe('font://localhost/Fira%20Code');
    expect(calls).toEqual([['Fira Code', 'font']]);
  });

  it('gives no URL outside Tauri', () => {
    convert = actual.convertFileSrc;
    expect(fontUrl('Menlo')).toBeNull();
  });

  it('points the @font-face block at the URL under the family name', () => {
    const css = fontFaceCss('Fira Code', 'http://font.localhost/Fira%20Code');
    expect(css).toContain('font-family: "Fira Code";');
    expect(css).toContain('src: url("http://font.localhost/Fira%20Code");');
    expect(css).not.toContain('font://Fira');
  });
});

describe('renderFontStack', () => {
  const families = (stack: string) =>
    stack
      .split(',')
      .map((piece) => piece.trim().replace(/^["']|["']$/g, ''))
      .filter((name) => name !== '');

  // The same cases run against rendered_families in
  // src-tauri/src/native/gpu/atlas.rs, so xterm and the native atlas try
  // the same families in the same order.
  for (const c of fixture.cases) {
    it(c.name, () => {
      const rendered = renderFontStack(c.stack);
      expect(families(rendered)).toEqual(c.families);
      expect(renderFontStack(rendered)).toBe(rendered);
    });
  }

  it('quotes each family but the generic ones', () => {
    expect(renderFontStack('"BerkeleyMono Bundled", Menlo, monospace')).toBe(
      '"JetBrainsMono Bundled", "Menlo", monospace',
    );
  });

  it('hands back an ordinary list exactly as saved', () => {
    const stack = "'Fira Code',Menlo,  monospace";
    expect(renderFontStack(stack)).toBe(stack);
  });
});
