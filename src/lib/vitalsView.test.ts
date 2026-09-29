import { describe, expect, it } from 'vitest';
import {
  formatVital,
  meterFill,
  targetHealthPercent,
  thirdsTone,
  vitalsFooterHeight,
  vitalsGeometry,
  vitalTone,
  widestVital,
} from './vitalsView';

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

describe('targetHealthPercent', () => {
  const guard = { name: 'Blackwatch Guard', hp_pct: 38 };

  it('shows the opponent percent when the target is the one you fight', () => {
    expect(targetHealthPercent('Blackwatch Guard', guard)).toBe(38);
  });

  it('compares the names without case', () => {
    expect(targetHealthPercent('blackwatch guard', guard)).toBe(38);
    expect(targetHealthPercent(' BLACKWATCH GUARD ', guard)).toBe(38);
  });

  it('shows nothing for another target', () => {
    expect(targetHealthPercent('guard', guard)).toBeNull();
    expect(targetHealthPercent('Orc', guard)).toBeNull();
  });

  it('shows nothing out of a fight, with no target, or with no percent', () => {
    expect(targetHealthPercent('Blackwatch Guard', null)).toBeNull();
    expect(targetHealthPercent(null, guard)).toBeNull();
    expect(targetHealthPercent('', guard)).toBeNull();
    expect(targetHealthPercent('Blackwatch Guard', { ...guard, hp_pct: null })).toBeNull();
  });
});
