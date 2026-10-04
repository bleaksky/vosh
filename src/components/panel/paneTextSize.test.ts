import { afterEach, describe, expect, it, vi } from 'vitest';
import panelCss from '../../styles/panel.css?raw';
import frameCss from '../../styles/frame.css?raw';
import tokensCss from '../../styles/tokens.css?raw';
import { liveChipMeasure } from './chipMeasure';
import { PANE_TEXT_BASE, PANE_TEXT_PX, paneText, paneTextSize, textPx } from './paneTextSize';

// Every pane and the status line draw at your panel size, and every
// length that sits with the text scales from the 12 px the panes were
// drawn at. These tests hold panel.css, frame.css and paneTextSize.ts
// to the same numbers, and hold every scaled length at 12 px to the
// number the sheets drew before the panel had a size of its own.

/** Every rule in a sheet, its selector on one line, comments out. */
function rulesOf(css: string): { selector: string; body: string }[] {
  return [...css.replace(/\/\*[\s\S]*?\*\//g, '').matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
    selector: m[1].replace(/\s+/g, ' ').trim(),
    body: m[2],
  }));
}

const RULES = rulesOf(panelCss);
const FRAME_RULES = rulesOf(frameCss);
const TOKEN_RULES = rulesOf(tokensCss);

/** The declarations of the one rule with `selector` in `rules`. */
function declarations(selector: string, rules = RULES): Map<string, string> {
  const found = rules.filter((r) => r.selector === selector);
  expect(found, selector).toHaveLength(1);
  return new Map(
    [...found[0].body.matchAll(/([\w-]+)\s*:\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]),
  );
}

const TOKENS = declarations('.panel-host');

/** `value` with your size for --panel-text-px and every --mud-* token
 *  written out, as the webview substitutes them. The map band reads
 *  as its one row fallback. */
function resolve(value: string, size: number): string {
  let out = value;
  for (let i = 0; i < 8 && out.includes('var('); i += 1) {
    out = out
      .replace(/var\(--panel-text-px, 12\)/g, String(size))
      .replace(/var\(--band-rows, 1\)/g, '1')
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

/** A value as the webview computes it at panel size `size` px: each
 *  term that reads the size becomes px, the rest stay as written. */
function computedValue(value: string, size: number): string {
  const terms: string[] = [];
  let depth = 0;
  let term = '';
  for (const ch of value) {
    if (ch === '(') depth += 1;
    if (ch === ')') depth -= 1;
    if (ch === ' ' && depth === 0) {
      terms.push(term);
      term = '';
    } else term += ch;
  }
  terms.push(term);
  return terms
    .map((t) =>
      /var\(--(mud|panel-text-px)|calc\(|round\(/.test(t) ? `${lengthPx(t, size)}px` : t,
    )
    .join(' ');
}

/** A panel.css declaration as the webview computes it at `size` px. */
function computed(selector: string, prop: string, size: number): string {
  const value = declarations(selector).get(prop);
  expect(value, `${selector} ${prop}`).toBeDefined();
  return computedValue(value ?? '', size);
}

// Every declaration that follows your size, what panel.css drew for it
// at 12 px before, and what it draws at 16 px, four thirds of each,
// rounded, with the marks still 8 px and centered and the side insets
// as they were. --pane-header was 28px and --row 22px.
const SCALED: [string, string, string, string][] = [
  ['.pane-header', 'height', '28px', '37px'],
  ['.pane-header', 'padding', '6px 40px 0 18px', '8px 40px 0 18px'],
  ['.pane-label', 'font-size', '10px', '13px'],
  ['.pane-label', 'line-height', '12px', '16px'],
  ['.pane-meta', 'font-size', '11px', '15px'],
  ['.pane-meta', 'line-height', '15px', '20px'],
  ['.pane-more', 'top', '4px', '8px'],
  ['.pane-select', 'height', '15px', '20px'],
  ['.pane-select', 'font-size', '11px', '15px'],
  ['.pane-select', 'line-height', '15px', '20px'],
  ['.pane-row', 'height', '22px', '29px'],
  ['.pane-row', 'font-size', '12px', '16px'],
  ['.pane-row', 'line-height', '16px', '21px'],
  ['.pane-row-note', 'font-size', '11px', '15px'],
  ['.pane-marker', 'top', '7px', '11px'],
  ['.pane-empty', 'min-height', '22px', '29px'],
  ['.pane-empty', 'padding', '3px 12px 3px 18px', '4px 12px 4px 18px'],
  ['.pane-empty', 'font-size', '12px', '16px'],
  ['.pane-empty', 'line-height', '16px', '21px'],
  ['.pane-affects-grid', 'grid-auto-rows', '22px', '29px'],
  ['.pane-affect', 'height', '22px', '29px'],
  ['.pane-affect-mark', 'top', '7px', '11px'],
  ['.pane-affect-mark.is-harmful', 'top', '8px', '12px'],
  ['.pane-affect-hours', 'width', '22px', '29px'],
  ['.pane-affect-hours', 'font-size', '12px', '16px'],
  ['.pane-affect-hours', 'line-height', '16px', '21px'],
  ['.pane-affect-name', 'font-size', '12px', '16px'],
  ['.pane-affect-name', 'line-height', '16px', '21px'],
  ['.pane-affects-rule', 'margin', '4px 12px 4px 18px', '5px 12px 5px 18px'],
  ['.pane-affects-more-cell', 'height', '22px', '29px'],
  ['.pane-affects-more', 'height', '22px', '29px'],
  ['.pane-affects-more', 'font-size', '12px', '16px'],
  ['.pane-affects-more', 'line-height', '16px', '21px'],
  ['.pane-countdown-cell', 'height', '23px', '31px'],
  ['.pane-countdown-cell .pane-affect-mark', 'top', '5px', '8px'],
  ['.pane-countdown-cell .pane-affect-mark.is-harmful', 'top', '6px', '9px'],
  ['.pane-countdown-line', 'height', '17px', '22px'],
  ['.pane-countdown-line', 'font-size', '12px', '16px'],
  ['.pane-countdown-line', 'line-height', '16px', '21px'],
  ['.pane-countdown-meter', 'top', '19px', '25px'],
  ['.pane-countdown-meter', 'height', '2px', '3px'],
  ['.pane-countdown-fill', 'height', '2px', '3px'],
  ['.pane-countdown-more-cell', 'height', '23px', '31px'],
  ['.pane-countdown-more-cell .pane-affects-more', 'height', '16px', '21px'],
  ['.pane-chips-line', 'height', '20px', '27px'],
  ['.pane-chips-label', 'width', '56px', '75px'],
  ['.pane-chips-label', 'font-size', '11px', '15px'],
  ['.pane-chips-label', 'line-height', '20px', '27px'],
  ['.pane-chips-more', 'height', '20px', '27px'],
  ['.pane-chips-more .pane-affects-more', 'height', '20px', '27px'],
  ['.pane-chip', 'height', '20px', '27px'],
  ['.pane-chip', 'font-size', '12px', '16px'],
  ['.pane-chip', 'line-height', '16px', '21px'],
  ['.pane-map', 'padding-bottom', '8px', '11px'],
  ['.pane-map-empty', 'padding', '3px 4px 3px 10px', '4px 4px 4px 10px'],
  ['.pane-map-empty', 'font-size', '12px', '16px'],
  ['.pane-map-empty', 'line-height', '16px', '21px'],
  ['.pane-map-rows', 'height', '22px', '29px'],
  ['.pane-map-rows', 'min-height', '22px', '29px'],
  ['.pane-map-rows', 'margin-top', '4px', '5px'],
  ['.pane-map-where', 'font-size', '11px', '15px'],
  ['.pane-map-where', 'line-height', '14px', '19px'],
  ['.pane-member-lead', 'font-size', '11px', '15px'],
  ['.pane-member-pct', 'width', '36px', '48px'],
  ['.pane-chat-log', 'padding', '8px 12px 12px 18px', '11px 12px 16px 18px'],
  ['.pane-chat-log', 'font-size', '12px', '16px'],
  ['.pane-chat-log', 'line-height', '17px', '23px'],
  ['.pane-chat-msg + .pane-chat-msg', 'margin-top', '3px', '4px'],
  ['.panel-vitals-line', 'height', '16px', '21px'],
  ['.panel-vitals-line', 'font-size', '12px', '16px'],
  ['.panel-vitals-line', 'line-height', '16px', '21px'],
  ['.panel-vitals-empty', 'height', '16px', '21px'],
  ['.panel-vitals-empty', 'font-size', '12px', '16px'],
  ['.panel-vitals-empty', 'line-height', '16px', '21px'],
];

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
      header: 37,
      row: 29,
      affectsRow: 29,
      affectsRuleGap: 5,
      affectsHours: 29,
      countdownRow: 31,
      countdownMeterTop: 25,
      countdownMeter: 3,
      chip: 27,
      chipLineGap: 5,
      chipGroupGap: 11,
      chipsTop: 5,
      chipGutter: 75,
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

describe('the panel in panel.css', () => {
  it('sets the panel face and size once on .panel-host', () => {
    const host = declarations('.panel-host');
    expect(host.get('font-family')).toBe('var(--font-panel)');
    expect(host.get('font-size')).toBe('var(--mud-text)');
    expect(host.get('line-height')).toBe('var(--mud-line)');
    expect(TOKENS.get('--mud-text')).toBe('calc(var(--panel-text-px, 12) * 1px)');
    expect(TOKENS.get('--mud-scale')).toBe('calc(var(--panel-text-px, 12) / 12)');
  });

  it('sets every text in the panes at your panel size, and the menus at their own', () => {
    let read = 0;
    for (const { selector, body } of RULES) {
      for (const [, prop, value] of body.matchAll(/(font-size|line-height)\s*:\s*([^;]+);/g)) {
        const at = `${selector} ${prop}`;
        if (selector.includes('pane-menu')) {
          expect(value.trim(), at).toMatch(/^\d+px$/);
        } else {
          expect(value.trim(), at).toMatch(
            /^(var\(--mud-[\w-]+\)|round\(\d+px \* var\(--mud-scale\), 1px\))$/,
          );
          read += 1;
        }
      }
    }
    expect(read).toBeGreaterThanOrEqual(30);
  });

  it('scales each length from its base in paneTextSize.ts', () => {
    const scaled = [...TOKENS].filter(
      ([name]) => name.startsWith('--mud-') && name !== '--mud-scale' && name !== '--mud-text',
    );
    expect(scaled.length).toBeGreaterThanOrEqual(13);
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
      [...r.body.matchAll(/([\w-]+)\s*:\s*([^;]*var\(--(?:mud-|panel-text-px)[^;]*);/g)].map(
        (m) => `${r.selector} ${m[1]}`,
      ),
    );
    expect(reading.sort()).toEqual(SCALED.map(([s, p]) => `${s} ${p}`).sort());
  });

  it('draws every length at 12 px exactly as before', () => {
    for (const [selector, prop, before] of SCALED) {
      expect(computed(selector, prop, 12), `${selector} ${prop}`).toBe(before);
    }
  });

  it('draws them at 16 px four thirds as large, the marks centered', () => {
    for (const [selector, prop, , at16] of SCALED) {
      expect(computed(selector, prop, 16), `${selector} ${prop}`).toBe(at16);
    }
  });

  it('keeps the marks, the meters, and the side insets at their px', () => {
    expect(declarations('.pane-affect-mark').get('width')).toBe('8px');
    expect(declarations('.pane-marker').get('width')).toBe('8px');
    expect(declarations('.pane-member-meter').get('width')).toBe('48px');
    expect(declarations('.pane-member-meter').get('height')).toBe('3px');
    expect(declarations('.pane-more').get('height')).toBe('20px');
    expect(declarations('.pane-chips-line').get('left')).toBe('18px');
    expect(declarations('.pane-map-box').get('min-height')).toBe('96px');
  });

  it('hangs wrapped chat lines two cells of the log face in', () => {
    const msg = declarations('.pane-chat-msg');
    expect(msg.get('padding')).toBe('0 0 0 2ch');
    expect(msg.get('text-indent')).toBe('-2ch');
  });
});

describe('the status line in frame.css', () => {
  const line = declarations('.shell-statusline', FRAME_RULES);
  const row = declarations(':root', TOKEN_RULES.slice(0, 1)).get('--status-line') ?? '';

  it('draws at your panel size in the panel face', () => {
    expect(line.get('font-family')).toBe('var(--font-panel)');
    expect(line.get('height')).toBe('var(--status-line)');
    // The shell's last row is the status line's.
    expect(frameCss).toMatch(/grid-template-rows: [^;]* var\(--status-line\);/);
  });

  it('draws 12 px text on 16 px lines in a 28 px row at 12 px, as before', () => {
    expect(computedValue(line.get('font-size') ?? '', 12)).toBe('12px');
    expect(computedValue(line.get('line-height') ?? '', 12)).toBe('16px');
    expect(computedValue(row, 12)).toBe('28px');
  });

  it('scales its text, its line, and its row with your size', () => {
    expect(computedValue(line.get('font-size') ?? '', 14)).toBe('14px');
    expect(computedValue(line.get('line-height') ?? '', 14)).toBe('19px');
    expect(computedValue(row, 14)).toBe('33px');
    expect(computedValue(line.get('font-size') ?? '', 16)).toBe('16px');
    expect(computedValue(line.get('line-height') ?? '', 16)).toBe('21px');
    expect(computedValue(row, 16)).toBe('37px');
    // The gap between items and the side insets keep their px.
    expect(line.get('gap')).toBe('20px');
    expect(line.get('padding')).toBe('0 16px');
  });
});

describe('liveChipMeasure', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('measures every chip text in the panel face at your panel size', () => {
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
    const at12 = liveChipMeasure();
    at12.mono('sanctuary');
    at12.label('Recast');
    at12.count('3 more');
    expect(fonts).toEqual([
      '16px Menlo, monospace',
      '700 16px Menlo, monospace',
      '600 15px Menlo, monospace',
      '16px Menlo, monospace',
      '12px Menlo, monospace',
      '600 11px Menlo, monospace',
      '12px Menlo, monospace',
    ]);
  });
});
