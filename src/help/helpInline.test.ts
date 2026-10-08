import { describe, expect, it } from 'vitest';
import { HELP_TOPICS, parseHelpBody } from './helpContent';
import { classifyInline, inlinePieces, keyGlyph, keyParts } from './helpInline';

// Help draws a backticked span three ways (G5): a label you see in
// Vosh in SF 600, a key as keycaps, and MUD text or a code as a mono
// chip.

describe('a backticked span in help', () => {
  it('is a label when it names something you see in Vosh', () => {
    for (const span of [
      'Lifted',
      'In the text',
      'Edit as text',
      'Insert value…',
      'Where your prompt shows',
      'Show here instead',
      'Down past 0',
    ]) {
      expect(classifyInline(span), span).toBe('label');
    }
  });

  it('is a label when it is a line Vosh shows you, even one that names you', () => {
    // The Updates section of Settings, which the game never says.
    for (const span of [
      'Checking for updates…',
      'Vosh is up to date.',
      'You have Vosh <version>.',
      'Vosh <version> is ready.',
    ]) {
      expect(classifyInline(span), span).toBe('label');
    }
    // The game speaks to you, and never names Vosh.
    expect(classifyInline('You tell your group')).toBe('code');
    expect(classifyInline('You are hungry.')).toBe('code');
    expect(classifyInline('<file>.bak.<timestamp>')).toBe('code');
  });

  it('is code when it is MUD text, a command, a code or a file', () => {
    for (const span of [
      '#prompt show',
      '[prompt]',
      'text',
      'Char.Combat',
      '31s',
      '%hp',
      'catalog.toml',
      'a Blackwatch villager',
      'HH:MM',
      'TLS',
      'You tell your group',
    ]) {
      expect(classifyInline(span), span).toBe('code');
    }
  });

  it('is a key when it names one you press', () => {
    for (const span of [
      'Enter',
      'Shift+Enter',
      'Esc',
      'Tab',
      'Shift+Tab',
      'PageUp',
      'Cmd+F',
      'Ctrl+K',
      'Fn+Up',
      'F1',
      'Ctrl+Alt+Numpad7',
      'Cmd+,',
      'Cmd+/',
      'Cmd+\\',
      'Cmd+Shift+L',
      'Shift',
      'ArrowUp',
      '⌘K',
    ]) {
      expect(classifyInline(span), span).toBe('key');
    }
    expect(keyParts('Shift+Enter')).toEqual(['Shift', 'Enter']);
    expect(keyParts('Cmd+Shift+L')).toEqual(['Cmd', 'Shift', 'L']);
    expect(keyParts('Cmd+\\')).toEqual(['Cmd', '\\']);
    // A letter or a sign alone is no key, it is what you type.
    expect(classifyInline(',')).toBe('code');
    expect(classifyInline('a')).toBe('code');
  });

  it('keeps Up and Down labels unless a modifier comes first', () => {
    // Tick counts offers Up and Down, so the arrows go by ArrowUp.
    expect(classifyInline('Up')).toBe('label');
    expect(classifyInline('Down')).toBe('label');
    expect(keyGlyph('ArrowUp')).toBe('↑');
    expect(keyGlyph('Up')).toBe('↑');
  });

  it('splits a line at its backticks', () => {
    expect(inlinePieces('Choose `Edit as text` and press `Enter` to type `#help`.')).toEqual([
      { kind: 'text', text: 'Choose ' },
      { kind: 'label', text: 'Edit as text' },
      { kind: 'text', text: ' and press ' },
      { kind: 'key', text: 'Enter' },
      { kind: 'text', text: ' to type ' },
      { kind: 'code', text: '#help' },
      { kind: 'text', text: '.' },
    ]);
  });

  it('reads every span in the help as one of the three', () => {
    // Every backtick opens a span that closes on the same line.
    for (const topic of HELP_TOPICS) {
      for (const block of parseHelpBody(topic.body)) {
        const lines =
          block.kind === 'paragraph'
            ? [block.text]
            : block.kind === 'list'
              ? block.items
              : block.kind === 'table'
                ? [...block.head, ...block.rows.flat()]
                : [block.label];
        for (const line of lines) {
          expect(line.split('`').length % 2, `${topic.number} ${line}`).toBe(1);
          for (const piece of inlinePieces(line)) {
            expect(['text', 'code', 'label', 'key']).toContain(piece.kind);
          }
        }
      }
    }
  });
});
