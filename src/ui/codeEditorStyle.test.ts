import { StreamLanguage } from '@codemirror/language';
import { lua } from '@codemirror/legacy-modes/mode/lua';
import { diagnosticCount, forEachDiagnostic, type Diagnostic } from '@codemirror/lint';
import { highlightTree } from '@lezer/highlight';
import { EditorState } from '@uiw/react-codemirror';
import { describe, expect, it } from 'vitest';
import {
  codeEditorAttributes,
  codeHighlightStyle,
  markDiagnostics,
  marksUpdate,
} from './codeEditorStyle';

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

// wait_full, the sample the Scripts page shows.
const WAIT_FULL = [
  '-- wait_full',
  '-- Stand up once your hit points are full.',
  '',
  'mud.on_gmcp("Char.Vitals", function(data)',
  '  while data.hp < data.maxhp do',
  '    -- data never changes inside this loop, so it never ends',
  '  end',
  '  mud.send("stand")',
  'end)',
  '',
].join('\n');

const STOP = 'Vosh stopped wait_full at main.lua line 5 after 100 ms.';

/** Each diagnostic the state holds, with the text it covers. */
function marked(state: EditorState) {
  const out: { text: string; severity: string; message: string }[] = [];
  forEachDiagnostic(state, (d: Diagnostic, from: number, to: number) => {
    out.push({ text: state.sliceDoc(from, to), severity: d.severity, message: d.message });
  });
  return out;
}

describe('markDiagnostics', () => {
  it('covers each marked line from end to end as an error', () => {
    const doc = EditorState.create({ doc: WAIT_FULL }).doc;
    expect(markDiagnostics(doc, [{ line: 5, message: STOP }])).toEqual([
      { from: 99, to: 130, severity: 'error', message: STOP },
    ]);
    expect(WAIT_FULL.slice(99, 130)).toBe('  while data.hp < data.maxhp do');
  });

  it('marks nothing for a line the text does not have', () => {
    const doc = EditorState.create({ doc: WAIT_FULL }).doc;
    expect(markDiagnostics(doc, [{ line: 0, message: STOP }])).toEqual([]);
    expect(markDiagnostics(doc, [{ line: 11, message: STOP }])).toEqual([]);
  });
});

describe('marksUpdate', () => {
  it('puts the marks on through lint diagnostics', () => {
    const state = EditorState.create({ doc: WAIT_FULL });
    const update = marksUpdate(state, [{ line: 5, message: STOP }]);
    expect(update).not.toBeNull();
    const after = state.update(update ?? {}).state;
    expect(marked(after)).toEqual([
      { text: '  while data.hp < data.maxhp do', severity: 'error', message: STOP },
    ]);
  });

  it('takes the marks off once the page has none', () => {
    const state = EditorState.create({ doc: WAIT_FULL });
    const on = state.update(marksUpdate(state, [{ line: 5, message: STOP }]) ?? {}).state;
    const off = on.update(marksUpdate(on, []) ?? {}).state;
    expect(diagnosticCount(off)).toBe(0);
  });

  it('leaves an editor that never had a mark as it is', () => {
    expect(marksUpdate(EditorState.create({ doc: WAIT_FULL }), [])).toBeNull();
  });
});
