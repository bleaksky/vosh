import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { DEFAULT_VITALS_OPTIONS, type VitalsOptions } from '../ipc/uiConfig';
import type { CombatOpponent } from '../stores/gmcp/combatStore';
import type { Vitals } from '../stores/gmcp/vitalsStore';
import type { BandEnv } from '../terminal/bandCells';
import { parseSgrCells } from '../terminal/sgrCells';
import frameCss from '../styles/frame.css?raw';
import { StatusVitals, type LineText, type StatusVitalsProps } from './StatusLine';
import { FIT_ALL, type StatusLineFit } from './statusLineFit';

// The stores behind StatusLine reach the Tauri bridge. StatusVitals,
// under test, draws from plain values and never calls it.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

// Board 4 of the Vitals Styles review: Tolliver at 765 of 1020 with a
// Blackwatch guard at 54, and low at 159 on a walk.
const FULL: Vitals = {
  hp: 765,
  maxhp: 1020,
  mana: 800,
  maxmana: 800,
  move: 930,
  maxmove: 930,
  low: { hp: false, mana: false, move: false },
  hidden: false,
};
const LOW: Vitals = {
  hp: 159,
  maxhp: 1020,
  mana: 310,
  maxmana: 800,
  move: 489,
  maxmove: 930,
  low: { hp: true, mana: false, move: false },
  hidden: false,
};
// Lamented tears: Char.Vitals all zeros with the hidden flag.
const HIDDEN: Vitals = {
  hp: 0,
  maxhp: 0,
  mana: 0,
  maxmana: 0,
  move: 0,
  maxmove: 0,
  low: { hp: false, mana: false, move: false },
  hidden: true,
};
const GUARD: CombatOpponent = {
  name: 'a Blackwatch guard',
  hp_pct: 54,
  condition: null,
  hidden: false,
  tank: null,
};

function draw(props: Partial<StatusVitalsProps> = {}, options: Partial<VitalsOptions> = {}) {
  return renderToStaticMarkup(
    <StatusVitals
      showVitals
      vitals={FULL}
      target={null}
      combat={GUARD}
      {...props}
      options={{ ...DEFAULT_VITALS_OPTIONS, ...options }}
    />,
  );
}

/** Each label, name and value on the line with its tone, in order. */
function items(html: string): string[] {
  return [
    ...html.matchAll(
      /<span class="shell-status-(?:value( is-(?:low|warn|hidden))?|name)">([^<]*)<\/span>|<span class="shell-status-vital">(Health|Mana|Moves)(?=<)|>(Target)(?=<)/g,
    ),
  ].map((m) => m[3] ?? m[4] ?? (m[1] ? `${m[2]} ${m[1].trim()}` : m[2]));
}

describe('StatusVitals', () => {
  it('reads every vital, then your opponent with its health in warn', () => {
    expect(items(draw())).toEqual([
      'Health',
      '765 / 1020',
      'Mana',
      '800 / 800',
      'Moves',
      '930 / 930',
      'a Blackwatch guard',
      '54% is-warn',
    ]);
  });

  it('turns low Health danger out of a fight', () => {
    expect(items(draw({ vitals: LOW, combat: null }))).toEqual([
      'Health',
      '159 / 1020 is-low',
      'Mana',
      '310 / 800',
      'Moves',
      '489 / 930',
    ]);
  });

  it('joins a target on the same mob to your opponent', () => {
    for (const target of ['a Blackwatch guard', 'A BLACKWATCH GUARD ']) {
      expect(draw({ target })).toBe(draw());
    }
  });

  it('keeps a target on another mob as its own item after your opponent', () => {
    expect(items(draw({ target: 'Orla' })).slice(6)).toEqual([
      'a Blackwatch guard',
      '54% is-warn',
      'Target',
      'Orla',
    ]);
    expect(items(draw({ target: 'Orla', combat: null })).slice(6)).toEqual(['Target', 'Orla']);
  });

  it('follows your order and the vitals you turned off', () => {
    const html = draw(
      { vitals: LOW, combat: null },
      { order: ['move', 'hp', 'mana'], off: ['mana'], values: 'current' },
    );
    expect(items(html)).toEqual(['Moves', '489', 'Health', '159 is-low']);
  });

  it('drops your opponent you turned off and keeps your target by name', () => {
    expect(items(draw({ target: 'a Blackwatch guard' }, { off: ['opponent'] })).slice(6)).toEqual([
      'Target',
      'a Blackwatch guard',
    ]);
  });

  it('keeps a vital with no max', () => {
    const noMana = { ...FULL, mana: 0, maxmana: 0 };
    expect(items(draw({ vitals: noMana, combat: null })).slice(2, 4)).toEqual(['Mana', '0 / 0']);
  });

  it('follows Values and Warn before you run low', () => {
    expect(items(draw({ vitals: LOW, combat: null }, { values: 'percent' }))).toContain(
      '16% is-low',
    );
    expect(items(draw({ vitals: LOW, combat: null }, { warn_thirds: true }))).toEqual([
      'Health',
      '159 / 1020 is-low',
      'Mana',
      '310 / 800 is-warn',
      'Moves',
      '489 / 930 is-warn',
    ]);
  });

  it('draws one quiet form whatever the style, meter, colors or pinned switch', () => {
    const plain = draw();
    for (const style of ['rows', 'line', 'ledger', 'gauges', 'pips'] as const) {
      expect(draw({}, { style })).toBe(plain);
    }
    for (const meter of ['line', 'bar', 'none'] as const) expect(draw({}, { meter })).toBe(plain);
    expect(draw({}, { colors: { hp: 1, mana: 4 } })).toBe(plain);
    expect(draw({}, { hide_when_pinned: false })).toBe(plain);
    expect(draw({}, { opponent: 'bottom' })).toBe(plain);
    expect(plain).not.toContain('meter');
    expect(plain).not.toContain('style=');
  });

  it('shows ? for each hidden vital in the quiet tone and never warns', () => {
    for (const warn_thirds of [false, true]) {
      expect(items(draw({ vitals: HIDDEN, combat: null }, { warn_thirds }))).toEqual([
        'Health',
        '? / ? is-hidden',
        'Mana',
        '? / ? is-hidden',
        'Moves',
        '? / ? is-hidden',
      ]);
    }
  });

  it('reads the health the game withholds as a quiet ?, and a condition in warn', () => {
    const withheld = { ...GUARD, hp_pct: null, hidden: true };
    expect(items(draw({ combat: withheld })).slice(6)).toEqual([
      'a Blackwatch guard',
      '? is-hidden',
    ]);
    const condition = { ...GUARD, hp_pct: null, condition: 'quite a few wounds' };
    expect(items(draw({ combat: condition })).slice(7)).toEqual(['quite a few wounds is-warn']);
  });

  it('keeps your target by name alone while the panel draws your vitals', () => {
    expect(items(draw({ showVitals: false, target: 'a Blackwatch guard' }))).toEqual([
      'Target',
      'a Blackwatch guard',
    ]);
    expect(draw({ showVitals: false })).toBe('');
  });

  describe('as the line gives way', () => {
    const at = (fit: Partial<StatusLineFit>, props: Partial<StatusVitalsProps> = {}) =>
      renderToStaticMarkup(
        <StatusVitals
          showVitals
          vitals={LOW}
          target="Orla"
          combat={GUARD}
          options={DEFAULT_VITALS_OPTIONS}
          {...props}
          fit={{ ...FIT_ALL, ...fit }}
        />,
      );

    it('hides the name and drops a Target item on another mob', () => {
      const html = at({ names: false });
      expect(html).toContain(
        '<span class="shell-status-foe"><span class="shell-sr">a Blackwatch guard</span><span class="shell-status-value is-warn is-bare">54%</span>',
      );
      expect(html).not.toContain('Target');
    });

    it('keeps each label for a screen reader once it goes', () => {
      expect(at({ labels: false })).toContain(
        '<span class="shell-status-vital"><span class="shell-sr">Health</span><span class="shell-status-value is-low is-bare">159 / 1020</span></span>',
      );
    });

    it('falls back to Current, and to ? while the game hides your vitals', () => {
      expect(at({ current: true })).toContain('>159</span>');
      expect(at({ current: true }, { vitals: HIDDEN })).toContain(
        '<span class="shell-status-value is-hidden">?</span>',
      );
    });

    it('starts a bare value at the item edge', () => {
      expect(frameCss).toMatch(/\.shell-status-value\.is-bare \{\s*margin-left: 0;/);
    });
  });

  describe('in the Text style', () => {
    const env: BandEnv = {
      palette: Array.from({ length: 16 }, (_, i) => `#${String(i).padStart(2, '0')}0000`),
      fg: '#d0d0d0',
      bg: '#101218',
      selection: '#333333',
      selectionText: '#ffffff',
      renderer: 'xterm',
      brightBold: false,
    };
    const pieces = parseSgrCells(
      'a Blackwatch guard\r\n\x1b[33m54%\x1b[39m\r\n765\x1b[90m/1020hp\x1b[39m',
    );
    const text: LineText = { pieces, fight: true, env };
    const drawText = (props: Partial<StatusVitalsProps> = {}) =>
      draw({ text, ...props }, { style: 'text' });

    it('writes your text in pieces in place of the quiet form', () => {
      const html = drawText();
      expect(html).toContain('<span class="shell-status-text" style="color:#d0d0d0">');
      expect(html.match(/class="shell-status-text-piece"/g)).toHaveLength(3);
      expect(html).toContain('<span style="color:#030000">54%</span>');
      expect(html).not.toContain('Health');
      expect(html).not.toContain('shell-status-name');
    });

    it('joins a target on the mob your text fights, and keeps another', () => {
      expect(drawText({ target: 'a Blackwatch guard' })).toBe(drawText());
      expect(drawText({ target: 'Orla' })).toContain('Target<span class="shell-status-value">Orla');
      const calm = { ...text, fight: false };
      expect(drawText({ text: calm, target: 'a Blackwatch guard' })).toContain('Target<span');
    });

    it('draws nothing of its own before the text comes', () => {
      expect(drawText({ text: null })).toBe('');
    });

    it('leaves your vitals to the panel while the line does not carry them', () => {
      expect(drawText({ showVitals: false })).toBe('');
    });
  });
});

describe('the status line in frame.css', () => {
  const rule = (selector: string) => {
    const at = frameCss.indexOf(`\n${selector} {`);
    expect(at, selector).toBeGreaterThanOrEqual(0);
    return frameCss.slice(at, frameCss.indexOf('}', at));
  };

  it('keeps every item whole but your opponent, your target and your text', () => {
    expect(rule('.shell-statusline > *')).toContain('flex: none;');
    for (const item of ['foe', 'target', 'text']) {
      const shrinks = rule(`.shell-statusline > .shell-status-${item}`);
      expect(shrinks).toContain('flex: 0 1 auto;');
      expect(shrinks).toContain('min-width: 0;');
      expect(shrinks).toContain('overflow: hidden;');
    }
  });

  it('ends the name and the text in an ellipsis and keeps the health whole', () => {
    expect(rule('.shell-status-name')).toContain('text-overflow: ellipsis;');
    expect(rule('.shell-statusline > .shell-status-text')).toContain('text-overflow: ellipsis;');
    expect(rule('.shell-status-foe > .shell-status-value')).toContain('flex: none;');
  });

  it('writes your text in the terminal face, 20 px between pieces', () => {
    expect(rule('.shell-statusline > .shell-status-text')).toContain(
      'font-family: var(--font-panel-game);',
    );
    expect(rule('.shell-status-text-piece + .shell-status-text-piece')).toContain(
      'margin-left: 20px;',
    );
  });

  it('sets the warn and hidden tones', () => {
    expect(rule('.shell-statusline .is-warn')).toContain('color: var(--warn)');
    expect(rule('.shell-status-value.is-hidden')).toContain('color: var(--tertiary)');
  });
});
