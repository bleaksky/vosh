import { describe, expect, it } from 'vitest';
import { colorInputValue, normalizeHexColor, readColorText, rgbStringToHex } from './colorField';

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
