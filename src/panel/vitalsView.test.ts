import { describe, expect, it } from 'vitest';
import {
  formatVital,
  hiddenVital,
  meterFill,
  opponentHealth,
  panelVitals,
  sameMob,
  shownVitals,
  thirdsTone,
  vitalsFooterHeight,
  vitalsGeometry,
  vitalInks,
  vitalsOn,
  vitalTone,
  widestVital,
} from './vitalsView';
import type { Vitals } from '../stores/gmcp/vitalsStore';
import { contrast, parseHex } from '../theme/color';
import { findTheme, themeTokens } from '../theme/themes';

// The VitalsOptions board's fight: Health 186 / 1020, Mana 344 / 800,
// Moves 870 / 930, and Blackwatch Guard at 38%.

describe('formatVital', () => {
  it('reads current and max by default', () => {
    expect(formatVital('current-max', 186, 1020)).toBe('186 / 1020');
    expect(formatVital('current-max', 1020, 1020)).toBe('1020 / 1020');
  });

  it('drops the max for Current', () => {
    expect(formatVital('current', 186, 1020)).toBe('186');
    expect(formatVital('current', 344, 800)).toBe('344');
  });

  it('reads whole percent for Percent', () => {
    expect(formatVital('percent', 186, 1020)).toBe('18%');
    expect(formatVital('percent', 344, 800)).toBe('43%');
    expect(formatVital('percent', 870, 930)).toBe('94%');
    expect(formatVital('percent', 1020, 1020)).toBe('100%');
  });

  it('shows the value alone for a vital with no max under Percent', () => {
    expect(formatVital('percent', 12, 0)).toBe('12');
  });
});

describe('widestVital', () => {
  it('reads the vital at full in each form', () => {
    expect(widestVital('current-max', 1020)).toBe('1020 / 1020');
    expect(widestVital('current', 1020)).toBe('1020');
    expect(widestVital('percent', 1020)).toBe('100%');
  });

  it('reads the hidden form while the game hides your vitals', () => {
    expect(widestVital('current-max', 0, true)).toBe('? / ?');
    expect(widestVital('percent', 0, true)).toBe('?%');
  });
});

describe('hiddenVital', () => {
  it('writes ? for each number the form would show', () => {
    expect(hiddenVital('current-max')).toBe('? / ?');
    expect(hiddenVital('current')).toBe('?');
    expect(hiddenVital('percent')).toBe('?%');
  });
});

describe('thirdsTone', () => {
  it('matches the Group pane', () => {
    expect(thirdsTone(100)).toBe('quiet');
    expect(thirdsTone(67)).toBe('quiet');
    expect(thirdsTone(66)).toBe('warn');
    expect(thirdsTone(34)).toBe('warn');
    expect(thirdsTone(33)).toBe('danger');
    expect(thirdsTone(0)).toBe('danger');
  });
});

describe('vitalTone', () => {
  it('keeps the low latch while Warn before you run low is off', () => {
    expect(vitalTone(186, 1020, true, false)).toBe('danger');
    expect(vitalTone(344, 800, false, false)).toBe('quiet');
    // Climbing back past 20 percent stays danger until the latch lets go.
    expect(vitalTone(230, 1020, true, false)).toBe('danger');
  });

  it('follows the thirds while it is on', () => {
    expect(vitalTone(186, 1020, true, true)).toBe('danger');
    expect(vitalTone(344, 800, false, true)).toBe('warn');
    expect(vitalTone(870, 930, false, true)).toBe('quiet');
    // 665 of 1000 reads 67 percent, the first quiet one.
    expect(vitalTone(665, 1000, false, true)).toBe('quiet');
    expect(vitalTone(664, 1000, false, true)).toBe('warn');
  });

  it('keeps a vital with no max quiet', () => {
    expect(vitalTone(0, 0, false, true)).toBe('quiet');
    expect(vitalTone(0, 0, false, false)).toBe('quiet');
  });
});

describe('meterFill', () => {
  it('fills unrounded and clamps', () => {
    expect(meterFill(186, 1020)).toBeCloseTo(18.24, 2);
    expect(meterFill(1100, 1020)).toBe(100);
    expect(meterFill(-5, 100)).toBe(0);
    expect(meterFill(5, 0)).toBe(0);
  });
});

describe('vitalsGeometry', () => {
  it('draws the 2 px line 1 under the text on a 28 pitch', () => {
    expect(vitalsGeometry('line')).toEqual({
      row: 28,
      rowTop: 4,
      meter: 2,
      meterGap: 1,
      meterRadius: 1,
      padTop: 8,
      padBottom: 11,
    });
  });

  it('draws the 4 px bar 2 under the text and keeps the pitch', () => {
    const bar = vitalsGeometry('bar');
    expect(bar).toMatchObject({ row: 28, rowTop: 4, meter: 4, meterGap: 2, meterRadius: 2 });
    // The bar still ends inside the row.
    expect(bar.rowTop + 16 + bar.meterGap + bar.meter).toBeLessThanOrEqual(bar.row);
  });

  it('drops the meter and tightens the rows to the dense 22 pitch', () => {
    expect(vitalsGeometry('none')).toEqual({
      row: 22,
      rowTop: 3,
      meter: 0,
      meterGap: 0,
      meterRadius: 0,
      padTop: 9,
      padBottom: 13,
    });
  });

  it('keeps the text 12 above and 16 below in Rows and None', () => {
    for (const meter of ['line', 'none'] as const) {
      const g = vitalsGeometry(meter);
      expect(g.padTop + g.rowTop).toBe(12);
    }
    const none = vitalsGeometry('none');
    expect(none.row - none.rowTop - 16 + none.padBottom).toBe(16);
  });
});

describe('vitalsFooterHeight', () => {
  it('holds three rows, or one on One line', () => {
    expect(vitalsFooterHeight(vitalsGeometry('line'), 3)).toBe(104);
    expect(vitalsFooterHeight(vitalsGeometry('bar'), 3)).toBe(104);
    expect(vitalsFooterHeight(vitalsGeometry('none'), 3)).toBe(89);
    expect(vitalsFooterHeight(vitalsGeometry('line'), 1)).toBe(48);
    expect(vitalsFooterHeight(vitalsGeometry('none'), 1)).toBe(45);
  });
});

describe('sameMob', () => {
  it('compares the names without case or the spaces round them', () => {
    expect(sameMob('Blackwatch Guard', 'blackwatch guard')).toBe(true);
    expect(sameMob(' BLACKWATCH GUARD ', 'Blackwatch Guard')).toBe(true);
    expect(sameMob('guard', 'Blackwatch Guard')).toBe(false);
  });
});

describe('panelVitals', () => {
  const pinned = { show: 'pinned', capture: true, promptsOff: false } as const;
  const at = (hide: boolean, place: 'panel' | 'status' = 'panel') => ({
    place,
    hide_when_pinned: hide,
  });

  it('keeps the opponent alone only while your prompt is pinned and the switch is on', () => {
    expect(panelVitals(pinned, at(true))).toBe('opponent');
    expect(panelVitals(pinned, at(false))).toBe('vitals');
    expect(panelVitals({ ...pinned, show: 'text' }, at(true))).toBe('vitals');
    expect(panelVitals({ ...pinned, show: 'lifted' }, at(true))).toBe('vitals');
    expect(panelVitals(null, at(true))).toBe('vitals');
  });

  it('keeps the footer while the pinned band has no prompt to show', () => {
    // No capture: the band never draws.
    expect(panelVitals({ ...pinned, capture: false }, at(true))).toBe('vitals');
    // Prompts off in the game: the band only says so, with no vitals.
    expect(panelVitals({ ...pinned, promptsOff: true }, at(true))).toBe('vitals');
  });

  it('draws nothing with your vitals in the status line', () => {
    expect(panelVitals(null, at(true, 'status'))).toBeNull();
    expect(panelVitals(pinned, at(true, 'status'))).toBeNull();
    expect(panelVitals(pinned, at(false, 'status'))).toBeNull();
  });
});

describe('shownVitals', () => {
  const vitals: Vitals = {
    hp: 186,
    maxhp: 1020,
    mana: 0,
    maxmana: 0,
    move: 870,
    maxmove: 930,
    low: { hp: true, mana: false, move: false },
    hidden: false,
  };

  it('keeps your order without the vitals you turned off', () => {
    expect(vitalsOn(['move', 'hp', 'mana'], ['mana', 'opponent'])).toEqual(['move', 'hp']);
    expect(vitalsOn(['hp', 'mana', 'move'], [])).toEqual(['hp', 'mana', 'move']);
  });

  it('drops Mana and Moves while the game sends no max for them', () => {
    expect(shownVitals(vitals, ['move', 'mana', 'hp'])).toEqual(['move', 'hp']);
    expect(shownVitals({ ...vitals, maxhp: 0 }, ['hp'])).toEqual(['hp']);
  });

  it('keeps every vital you left on while the game hides them', () => {
    expect(shownVitals({ ...vitals, maxhp: 0, maxmove: 0, hidden: true }, ['mana', 'hp'])).toEqual([
      'mana',
      'hp',
    ]);
  });
});

describe('opponentHealth', () => {
  it('reads the percent, else the condition, else ?', () => {
    expect(opponentHealth({ hp_pct: 54, condition: 'quite a few wounds', hidden: false })).toEqual({
      value: '54%',
      pct: 54,
      hidden: false,
    });
    expect(opponentHealth({ hp_pct: null, condition: 'awful', hidden: false })).toEqual({
      value: 'awful',
      pct: null,
      hidden: false,
    });
    expect(opponentHealth({ hp_pct: null, condition: null, hidden: false })).toEqual({
      value: '?',
      pct: null,
      hidden: true,
    });
    expect(opponentHealth({ hp_pct: 54, condition: 'awful', hidden: true }).value).toBe('?');
  });
});

describe('vitalInks', () => {
  const kanso = findTheme('kanso-zen');
  const rubric = findTheme('rubric');

  it('colors only the vitals you picked a slot for', () => {
    const inks = vitalInks({ mana: 12 }, kanso.xterm, themeTokens(kanso));
    expect(Object.keys(inks)).toEqual(['mana']);
  });

  it('keeps a slot that already reads at 3:1 on the panel', () => {
    // Bright blue on Kanso Zen, #8cc2d8 on the board.
    const ground = themeTokens(kanso);
    expect(vitalInks({ mana: 12 }, kanso.xterm, ground).mana).toBe(kanso.xterm.brightBlue);
  });

  it('lifts every slot to 3:1 on the panel, dark and light', () => {
    for (const theme of [kanso, rubric]) {
      const ground = themeTokens(theme);
      const panel = parseHex(ground.panel)!;
      for (let slot = 0; slot < 16; slot++) {
        const ink = parseHex(vitalInks({ hp: slot }, theme.xterm, ground).hp!)!;
        expect(contrast(ink, panel), `${theme.id} ${slot}`).toBeGreaterThanOrEqual(2.99);
      }
    }
  });
});
