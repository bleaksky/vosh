import { afterEach, describe, expect, it, vi } from 'vitest';
import panelCss from '../../styles/panel.css?raw';
import { liveChipMeasure } from './chipMeasure';
import { PANE_TEXT_BASE, PANE_TEXT_PX, paneText, paneTextSize, textPx } from './paneTextSize';

// The game text in the panes follows your terminal size, and every
// length that sits with it scales from the 12 px the panes were drawn
// at. These tests hold panel.css and paneTextSize.ts to the same
// numbers, and hold every scaled length at 12 px to the number the
// sheet drew before the panes followed your size.

/** Every rule in panel.css, its selector on one line, comments out. */
const RULES: { selector: string; body: string }[] = [
  ...panelCss.replace(/\/\*[\s\S]*?\*\//g, '').matchAll(/([^{}]+)\{([^{}]*)\}/g),
].map((m) => ({ selector: m[1].replace(/\s+/g, ' ').trim(), body: m[2] }));

/** The declarations of the one rule with `selector`. */
function declarations(selector: string): Map<string, string> {
  const found = RULES.filter((r) => r.selector === selector);
  expect(found, selector).toHaveLength(1);
  return new Map(
    [...found[0].body.matchAll(/([\w-]+)\s*:\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]),
  );
}

const TOKENS = declarations('.panel-host');

/** `value` with your size for --font-mud-px and every --mud-* token
 *  written out, as the webview substitutes them. */
function resolve(value: string, size: number): string {
  let out = value;
  for (let i = 0; i < 8 && out.includes('var('); i += 1) {
    out = out
      .replace(/var\(--font-mud-px, 12\)/g, String(size))
      .replace(/var\((--mud-[\w-]+)\)/g, (_, name: string) => {
        const token = TOKENS.get(name);
        expect(token, name).toBeDefined();
        return `(${token})`;
      });
  }
  return out;
}

/** One length, computed in px the way CSS does: calc arithmetic, and
 *  round() to the nearest step with a tie going up. */
function lengthPx(term: string, size: number): number {
  const expr = resolve(term, size)
    .replace(/px/g, '')
    .replace(/calc\(/g, '(')
    .replace(/round\(/g, 'R(');
  expect(expr, term).toMatch(/^[\d\s.+\-*/(),R]+$/);
  const round = (x: number, step: number) => Math.round(x / step) * step;
  return new Function('R', `return ${expr};`)(round) as number;
}

/** A declaration's value as the webview computes it at text `size` px:
 *  each term that reads the size becomes px, the rest stay as written. */
function computed(selector: string, prop: string, size: number): string {
  const value = declarations(selector).get(prop);
  expect(value, `${selector} ${prop}`).toBeDefined();
  const terms: string[] = [];
  let depth = 0;
  let term = '';
  for (const ch of value ?? '') {
    if (ch === '(') depth += 1;
    if (ch === ')') depth -= 1;
    if (ch === ' ' && depth === 0) {
      terms.push(term);
      term = '';
    } else term += ch;
  }
  terms.push(term);
  return terms
    .map((t) => (/var\(--(mud|font-mud-px)|calc\(|round\(/.test(t) ? `${lengthPx(t, size)}px` : t))
    .join(' ');
}

// Every declaration that follows your size, and what panel.css drew
// for it before at 12 px. --row was 22px.
const AT_12: [string, string, string][] = [
  ['.pane-affects-grid', 'grid-auto-rows', '22px'],
  ['.pane-affect', 'height', '22px'],
  ['.pane-affect-mark', 'top', '7px'],
  ['.pane-affect-mark.is-harmful', 'top', '8px'],
  ['.pane-affect-hours', 'width', '22px'],
  ['.pane-affect-hours', 'font-size', '12px'],
  ['.pane-affect-hours', 'line-height', '16px'],
  ['.pane-affect-name', 'font-size', '12px'],
  ['.pane-affect-name', 'line-height', '16px'],
  ['.pane-affects-rule', 'margin', '4px 12px 4px 18px'],
  ['.pane-affects-more-cell', 'height', '22px'],
  ['.pane-affects-more', 'height', '22px'],
  ['.pane-countdown-cell', 'height', '23px'],
  ['.pane-countdown-cell .pane-affect-mark', 'top', '5px'],
  ['.pane-countdown-cell .pane-affect-mark.is-harmful', 'top', '6px'],
  ['.pane-countdown-line', 'height', '17px'],
  ['.pane-countdown-line', 'font-size', '12px'],
  ['.pane-countdown-line', 'line-height', '16px'],
  ['.pane-countdown-meter', 'top', '19px'],
  ['.pane-countdown-meter', 'height', '2px'],
  ['.pane-countdown-fill', 'height', '2px'],
  ['.pane-countdown-more-cell', 'height', '23px'],
  ['.pane-countdown-more-cell .pane-affects-more', 'height', '16px'],
  ['.pane-chips-line', 'height', '20px'],
  ['.pane-chips-label', 'line-height', '20px'],
  ['.pane-chips-more', 'height', '20px'],
  ['.pane-chips-more .pane-affects-more', 'height', '20px'],
  ['.pane-chip', 'height', '20px'],
  ['.pane-chip', 'font-size', '12px'],
  ['.pane-chip', 'line-height', '16px'],
  ['.pane-chat-log', 'font-size', '12px'],
  ['.pane-chat-log', 'line-height', '17px'],
  ['.pane-chat-msg + .pane-chat-msg', 'margin-top', '3px'],
];

// The same at 16 px: four thirds of each, rounded, with the marks
// still 8 px and centered.
const AT_16: Record<string, string> = {
  '.pane-affects-grid grid-auto-rows': '29px',
  '.pane-affect height': '29px',
  '.pane-affect-mark top': '11px',
  '.pane-affect-mark.is-harmful top': '12px',
  '.pane-affect-hours width': '29px',
  '.pane-affect-hours font-size': '16px',
  '.pane-affect-hours line-height': '21px',
  '.pane-affect-name font-size': '16px',
  '.pane-affect-name line-height': '21px',
  '.pane-affects-rule margin': '5px 12px 5px 18px',
  '.pane-affects-more-cell height': '29px',
  '.pane-affects-more height': '29px',
  '.pane-countdown-cell height': '31px',
  '.pane-countdown-cell .pane-affect-mark top': '8px',
  '.pane-countdown-cell .pane-affect-mark.is-harmful top': '9px',
  '.pane-countdown-line height': '22px',
  '.pane-countdown-line font-size': '16px',
  '.pane-countdown-line line-height': '21px',
  '.pane-countdown-meter top': '25px',
  '.pane-countdown-meter height': '3px',
  '.pane-countdown-fill height': '3px',
  '.pane-countdown-more-cell height': '31px',
  '.pane-countdown-more-cell .pane-affects-more height': '21px',
  '.pane-chips-line height': '27px',
  '.pane-chips-label line-height': '27px',
  '.pane-chips-more height': '27px',
  '.pane-chips-more .pane-affects-more height': '27px',
  '.pane-chip height': '27px',
  '.pane-chip font-size': '16px',
  '.pane-chip line-height': '21px',
  '.pane-chat-log font-size': '16px',
  '.pane-chat-log line-height': '23px',
  '.pane-chat-msg + .pane-chat-msg margin-top': '4px',
};

const kebab = (key: string) => key.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);

describe('paneText', () => {
  it('draws every length at its base at 12 px', () => {
    expect(PANE_TEXT_PX).toBe(12);
    expect(paneText()).toEqual({ size: 12, ...PANE_TEXT_BASE });
    expect(paneText(12)).toEqual({ size: 12, ...PANE_TEXT_BASE });
  });

  it('scales every length by your size over 12, in whole px', () => {
    expect(paneText(16)).toEqual({
      size: 16,
      line: 21,
      affectsRow: 29,
      affectsRuleGap: 5,
      affectsHours: 29,
      countdownRow: 31,
      countdownMeterTop: 25,
      countdownMeter: 3,
      chip: 27,
      chipLineGap: 5,
      chipGroupGap: 11,
      twoColumns: 480,
      chatLine: 23,
      chatGap: 4,
      chatSticky: 32,
    });
    // A tie rounds up, as CSS round() does.
    expect(textPx(3, 14)).toBe(4);
    expect(textPx(22, 15)).toBe(28);
  });

  it('draws at 12 px for a size that is not one', () => {
    for (const bad of [0, -4, Number.NaN, Number.POSITIVE_INFINITY, null, undefined]) {
      expect(paneTextSize(bad)).toBe(12);
      expect(paneText(bad as number).size).toBe(12);
    }
    expect(paneTextSize(18)).toBe(18);
  });
});

describe('the game text in panel.css', () => {
  it('sets every game text rule at your terminal size in the panel face', () => {
    const mono = RULES.filter((r) => /font-size:\s*var\(--mud-text\)/.test(r.body));
    expect(mono.map((r) => r.selector)).toEqual([
      '.pane-affect-hours',
      '.pane-affect-name',
      '.pane-countdown-line',
      '.pane-chip',
      '.pane-chat-log',
    ]);
    for (const { selector } of mono) {
      const decls = declarations(selector);
      expect(decls.get('font-family'), selector).toBeUndefined();
      expect(decls.get('line-height'), selector).toMatch(/^var\(--mud-(line|chat-line)\)$/);
    }
    expect(declarations('.panel-host').get('font-family')).toBe('var(--font-panel)');
    expect(TOKENS.get('--mud-text')).toBe('calc(var(--font-mud-px, 12) * 1px)');
    expect(TOKENS.get('--mud-scale')).toBe('calc(var(--font-mud-px, 12) / 12)');
  });

  it('scales each length from its base in paneTextSize.ts', () => {
    const scaled = [...TOKENS].filter(
      ([name]) => name.startsWith('--mud-') && name !== '--mud-scale' && name !== '--mud-text',
    );
    expect(scaled.length).toBeGreaterThanOrEqual(10);
    for (const [name, value] of scaled) {
      const m = /^round\((\d+)px \* var\(--mud-scale\), 1px\)$/.exec(value);
      expect(m, `${name}: ${value}`).not.toBeNull();
      const key = Object.keys(PANE_TEXT_BASE).find((k) => `--mud-${kebab(k)}` === name);
      expect(key, name).toBeDefined();
      expect(Number(m?.[1]), name).toBe(PANE_TEXT_BASE[key as keyof typeof PANE_TEXT_BASE]);
      // The sheet and the geometry round alike at every size you can set.
      for (let size = 6; size <= 64; size += 1) {
        expect(lengthPx(value, size), `${name} at ${size}`).toBe(
          paneText(size)[key as keyof typeof PANE_TEXT_BASE],
        );
      }
    }
  });

  it('checks every declaration that reads your size', () => {
    const reading = RULES.filter((r) => r.selector !== '.panel-host').flatMap((r) =>
      [...r.body.matchAll(/([\w-]+)\s*:\s*([^;]*var\(--(?:mud-|font-mud-px)[^;]*);/g)].map(
        (m) => `${r.selector} ${m[1]}`,
      ),
    );
    expect(reading.sort()).toEqual(AT_12.map(([s, p]) => `${s} ${p}`).sort());
  });

  it('draws every length at 12 px exactly as before', () => {
    for (const [selector, prop, before] of AT_12) {
      expect(computed(selector, prop, 12), `${selector} ${prop}`).toBe(before);
    }
  });

  it('draws them at 16 px four thirds as large, the marks centered', () => {
    for (const [selector, prop] of AT_12) {
      const key = `${selector} ${prop}`;
      expect(computed(selector, prop, 16), key).toBe(AT_16[key]);
    }
  });

  it('hangs wrapped chat lines two cells of the log face in', () => {
    const msg = declarations('.pane-chat-msg');
    expect(msg.get('padding')).toBe('0 0 0 2ch');
    expect(msg.get('text-indent')).toBe('-2ch');
  });
});

describe('liveChipMeasure', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('measures every chip text in the panel face, names and hours at your size', () => {
    const fonts: string[] = [];
    const ctx = {
      font: '',
      measureText(text: string) {
        fonts.push(this.font);
        return { width: text.length };
      },
    };
    // Just what the measure reads of a page: the faces on the root and
    // a canvas to measure in.
    vi.stubGlobal('document', {
      documentElement: {},
      createElement: () => ({ getContext: () => ctx }),
    });
    vi.stubGlobal('getComputedStyle', () => ({
      getPropertyValue: (name: string) => (name === '--font-panel' ? 'Menlo, monospace' : ''),
    }));
    const at16 = liveChipMeasure(16);
    at16.mono('sanctuary');
    at16.hours?.('12');
    at16.label('Recast');
    at16.count('3 more');
    liveChipMeasure().mono('sanctuary');
    expect(fonts).toEqual([
      '16px Menlo, monospace',
      '700 16px Menlo, monospace',
      '600 11px Menlo, monospace',
      '12px Menlo, monospace',
      '12px Menlo, monospace',
    ]);
  });
});
