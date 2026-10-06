import { describe, expect, it } from 'vitest';
import { shortcutKey, shortcutKeys, shortcutLabel } from './shortcuts';

describe('shortcutKeys', () => {
  it('uses the Apple glyphs and modifier order on macOS', () => {
    expect(shortcutKeys('Mod+Shift+L', true)).toEqual(['⇧', '⌘', 'L']);
    expect(shortcutKeys('Mod+F', true)).toEqual(['⌘', 'F']);
    expect(shortcutKeys('Ctrl+Alt+Shift+Mod+K', true)).toEqual(['⌃', '⌥', '⇧', '⌘', 'K']);
  });

  it('spells the modifiers out on Windows and Linux, with Mod as Ctrl', () => {
    expect(shortcutKeys('Mod+Shift+L', false)).toEqual(['Ctrl', 'Shift', 'L']);
    expect(shortcutKeys('Mod+F', false)).toEqual(['Ctrl', 'F']);
    expect(shortcutKeys('Mod+Ctrl+X', false)).toEqual(['Ctrl', 'X']);
  });

  it('keeps punctuation keys and names the special ones', () => {
    expect(shortcutKeys('Mod+,', true)).toEqual(['⌘', ',']);
    expect(shortcutKeys('Mod+/', false)).toEqual(['Ctrl', '/']);
    expect(shortcutKeys('Mod++', true)).toEqual(['⌘', '+']);
    expect(shortcutKeys('Shift+Enter', true)).toEqual(['⇧', '↩']);
    expect(shortcutKeys('Shift+Enter', false)).toEqual(['Shift', 'Enter']);
    expect(shortcutKeys('Escape', false)).toEqual(['Esc']);
  });
});

describe('shortcutLabel', () => {
  it('runs the glyphs together on macOS and joins with plus elsewhere', () => {
    expect(shortcutLabel('Mod+C', true)).toBe('⌘C');
    expect(shortcutLabel('Mod+Shift+L', true)).toBe('⇧⌘L');
    expect(shortcutLabel('Mod+C', false)).toBe('Ctrl+C');
    expect(shortcutLabel('Mod+Shift+L', false)).toBe('Ctrl+Shift+L');
  });
});

describe('shortcutKey', () => {
  it('matches Latin layouts on the character typed', () => {
    expect(shortcutKey({ key: 'R', code: 'KeyR' })).toBe('r');
    // Dvorak types k from the physical V key.
    expect(shortcutKey({ key: 'k', code: 'KeyV' })).toBe('k');
    expect(shortcutKey({ key: ',', code: 'KeyW' })).toBe(',');
  });

  it('falls back to the physical key on a non-Latin layout', () => {
    expect(shortcutKey({ key: 'к', code: 'KeyR' })).toBe('r');
    expect(shortcutKey({ key: 'Л', code: 'KeyK' })).toBe('k');
    expect(shortcutKey({ key: 'б', code: 'Comma' })).toBe(',');
    expect(shortcutKey({ key: '.', code: 'Slash' })).toBe('.');
    expect(shortcutKey({ key: 'ё', code: 'Backquote' })).toBe('ё');
  });

  it('leaves named keys alone', () => {
    expect(shortcutKey({ key: 'Escape', code: 'Escape' })).toBe('escape');
  });
});
