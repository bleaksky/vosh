import { StreamLanguage } from '@codemirror/language';
import { lua } from '@codemirror/legacy-modes/mode/lua';
import { highlightTree } from '@lezer/highlight';
import { describe, expect, it } from 'vitest';
import { codeEditorAttributes, codeHighlightStyle } from './codeEditor';

/** The color each highlighted piece of `code` gets, by its text. */
function colors(code: string): Map<string, string> {
  const rules = codeHighlightStyle.module?.getRules() ?? '';
  const colorOf = (cls: string): string => {
    const rule = new RegExp(`\\.${cls}\\s*\\{([^}]*)\\}`).exec(rules);
    return /color:\s*([^;]+)/.exec(rule?.[1] ?? '')?.[1].trim() ?? '';
  };
  const tree = StreamLanguage.define(lua).parser.parse(code);
  const out = new Map<string, string>();
  highlightTree(tree, codeHighlightStyle, (from, to, classes) => {
    out.set(code.slice(from, to), colorOf(classes.split(' ')[0]));
  });
  return out;
}

describe('codeHighlightStyle', () => {
  it('colors Lua from the theme tokens', () => {
    const seen = colors('local hp = 42 -- low\nif hp then mud.send("flee") end');
    expect(seen.get('local')).toBe('var(--accent)');
    expect(seen.get('if')).toBe('var(--accent)');
    expect(seen.get('42')).toBe('var(--warn)');
    expect(seen.get('"flee"')).toBe('var(--success)');
    expect(seen.get('-- low')).toBe('var(--tertiary)');
  });

  it('uses no fixed colors, so every theme keeps its contrast', () => {
    const rules = codeHighlightStyle.module?.getRules() ?? '';
    expect(rules).toContain('var(--accent)');
    expect(rules).not.toMatch(/#[0-9a-f]{3,8}\b/i);
  });
});

describe('codeEditorAttributes', () => {
  it('names the editor by its visible label and describes it', () => {
    expect(
      codeEditorAttributes({
        ariaLabel: 'Lua script',
        ariaLabelledBy: 'row-label',
        ariaDescribedBy: 'row-desc',
      }),
    ).toEqual({ 'aria-labelledby': 'row-label', 'aria-describedby': 'row-desc' });
  });

  it('falls back to a plain name', () => {
    expect(codeEditorAttributes({ ariaLabel: 'send template' })).toEqual({
      'aria-label': 'send template',
    });
  });

  it('adds nothing it was not given', () => {
    expect(codeEditorAttributes({})).toEqual({});
  });
});
