import { describe, expect, it } from 'vitest';
import {
  colorInputValue,
  isSixDigitHex,
  normalizeHexColor,
  readColorText,
  readHexColorText,
  rgbStringToHex,
} from './colorText';

describe('normalizeHexColor', () => {
  it('reads six and three digit hex with or without the #', () => {
    expect(normalizeHexColor('#fffc41')).toBe('#fffc41');
    expect(normalizeHexColor('FFFC41')).toBe('#fffc41');
    expect(normalizeHexColor(' #AbC ')).toBe('#aabbcc');
    expect(normalizeHexColor('abc')).toBe('#aabbcc');
  });

  it('refuses anything else', () => {
    expect(normalizeHexColor('')).toBeNull();
    expect(normalizeHexColor('#ff')).toBeNull();
    expect(normalizeHexColor('#fffc4')).toBeNull();
    expect(normalizeHexColor('#fffc411')).toBeNull();
    expect(normalizeHexColor('red')).toBeNull();
    expect(normalizeHexColor('rgb(1, 2, 3)')).toBeNull();
  });
});

describe('readColorText', () => {
  it('reads an empty field as the theme default', () => {
    expect(readColorText('', false)).toEqual({ kind: 'default' });
    expect(readColorText('   ', true)).toEqual({ kind: 'default' });
  });

  it('saves six digits while you type', () => {
    expect(readColorText('#fffc41', false)).toEqual({ kind: 'color', hex: '#fffc41' });
    expect(readColorText('88C0D0', false)).toEqual({ kind: 'color', hex: '#88c0d0' });
  });

  it('holds three digits until you finish', () => {
    expect(readColorText('#fff', false)).toEqual({ kind: 'draft' });
    expect(readColorText('#fff', true)).toEqual({ kind: 'color', hex: '#ffffff' });
  });

  it('keeps a half typed color as a draft', () => {
    expect(readColorText('#fffc4', false)).toEqual({ kind: 'draft' });
    expect(readColorText('#fffc4', true)).toEqual({ kind: 'draft' });
    expect(readColorText('yellow', true)).toEqual({ kind: 'draft' });
  });
});

describe('readHexColorText', () => {
  // The sent command color and the divider color. The terminal reads
  // only hex there, so the field saves #rrggbb or nothing.
  it('saves hex the way readColorText does', () => {
    expect(readHexColorText('', true)).toEqual({ kind: 'default' });
    expect(readHexColorText('#FFFC41', false)).toEqual({ kind: 'color', hex: '#fffc41' });
    expect(readHexColorText('88c0d0', false)).toEqual({ kind: 'color', hex: '#88c0d0' });
    expect(readHexColorText('#fff', true)).toEqual({ kind: 'color', hex: '#ffffff' });
  });

  it('waits on a hex you are still typing', () => {
    expect(readHexColorText('#', false)).toEqual({ kind: 'draft' });
    expect(readHexColorText('#fff', false)).toEqual({ kind: 'draft' });
    expect(readHexColorText('#fffc4', false)).toEqual({ kind: 'draft' });
  });

  it('refuses a color name, rgb(), and a hex with alpha', () => {
    for (const text of ['red', 'rebeccapurple', 'rgb(1, 2, 3)', 'hsl(0 0% 50%)', '#ffffff80']) {
      expect(readHexColorText(text, false)).toEqual({ kind: 'invalid' });
      expect(readHexColorText(text, true)).toEqual({ kind: 'invalid' });
    }
  });

  it('refuses a partial hex once you leave the field', () => {
    expect(readHexColorText('#', true)).toEqual({ kind: 'invalid' });
    expect(readHexColorText('#ff', true)).toEqual({ kind: 'invalid' });
    expect(readHexColorText('#fffc4', true)).toEqual({ kind: 'invalid' });
    expect(readHexColorText('#0008', true)).toEqual({ kind: 'invalid' });
  });

  it('never hands the terminal a color it cannot read', () => {
    const texts = ['red', '#fff', '#fffc41', 'rgb(1,2,3)', '#12345678', 'fffc41', ' #ABCDEF '];
    for (const text of texts) {
      for (const final of [false, true]) {
        const read = readHexColorText(text, final);
        if (read.kind === 'color') expect(read.hex).toMatch(/^#[0-9a-f]{6}$/);
      }
    }
  });
});

describe('isSixDigitHex', () => {
  it('reads what the echo and the divider can draw', () => {
    expect(isSixDigitHex('#88c0d0')).toBe(true);
    expect(isSixDigitHex('88C0D0')).toBe(true);
    expect(isSixDigitHex('#fff')).toBe(false);
    expect(isSixDigitHex('red')).toBe(false);
    expect(isSixDigitHex('#88c0d080')).toBe(false);
  });
});

describe('colorInputValue', () => {
  it('gives the picker a hex it can hold, or null', () => {
    expect(colorInputValue('#FFF')).toBe('#ffffff');
    expect(colorInputValue('#fffc41')).toBe('#fffc41');
    expect(colorInputValue(null)).toBeNull();
    expect(colorInputValue('rgb(255, 0, 0)')).toBeNull();
  });
});

describe('rgbStringToHex', () => {
  it('reads a computed color', () => {
    expect(rgbStringToHex('rgb(123, 130, 148)')).toBe('#7b8294');
    expect(rgbStringToHex('rgba(0, 0, 0, 0.5)')).toBe('#000000');
    expect(rgbStringToHex('#434C5E')).toBe('#434c5e');
    expect(rgbStringToHex('transparent')).toBeNull();
  });
});
