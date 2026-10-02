import { describe, expect, it } from 'vitest';
import {
  AFFECTS_MARKER_LABELS,
  AFFECTS_STYLE_LABELS,
  affectsMarkerChoices,
  affectsStyleChoices,
  markerApplies,
  openPaneSubmenu,
} from './affectsDisplay';
import { DEFAULT_AFFECTS_DISPLAY } from './session';

describe('the affects display choices', () => {
  it('names the styles and markers the way Settings and the pane menu show them', () => {
    expect(AFFECTS_STYLE_LABELS).toEqual({
      timers: 'Timers first',
      countdown: 'Countdown',
      chips: 'Grouped chips',
    });
    expect(AFFECTS_MARKER_LABELS).toEqual({
      dot: 'Dot',
      square: 'Square',
      plus_minus: 'Plus and minus',
      none: 'None',
    });
  });

  it('checks the current style and marker', () => {
    const display = {
      ...DEFAULT_AFFECTS_DISPLAY,
      style: 'countdown',
      marker: 'plus_minus',
    } as const;
    expect(affectsStyleChoices(display)).toEqual([
      { value: 'timers', label: 'Timers first', checked: false },
      { value: 'countdown', label: 'Countdown', checked: true },
      { value: 'chips', label: 'Grouped chips', checked: false },
    ]);
    expect(affectsMarkerChoices(display).filter((c) => c.checked)).toEqual([
      { value: 'plus_minus', label: 'Plus and minus', checked: true },
    ]);
    expect(affectsMarkerChoices(DEFAULT_AFFECTS_DISPLAY).map((c) => c.label)).toEqual([
      'Dot',
      'Square',
      'Plus and minus',
      'None',
    ]);
  });

  it('draws a marker only in the timers and countdown styles', () => {
    expect(markerApplies({ ...DEFAULT_AFFECTS_DISPLAY, style: 'timers' })).toBe(true);
    expect(markerApplies({ ...DEFAULT_AFFECTS_DISPLAY, style: 'countdown' })).toBe(true);
    expect(markerApplies({ ...DEFAULT_AFFECTS_DISPLAY, style: 'chips' })).toBe(false);
  });
});

describe('openPaneSubmenu', () => {
  it('opens one submenu at a time, so opening one closes the others', () => {
    const show = openPaneSubmenu(null, 'show', false);
    expect(show).toEqual({ which: 'show', focus: false });
    expect(openPaneSubmenu(show, 'style', false)).toEqual({ which: 'style', focus: false });
    expect(openPaneSubmenu({ which: 'style', focus: true }, 'marker', true)).toEqual({
      which: 'marker',
      focus: true,
    });
  });

  it('opens the Channel colors submenu, and one channel list in it at a time', () => {
    expect(openPaneSubmenu(null, 'colors', true)).toEqual({ which: 'colors', focus: true });
    const say = openPaneSubmenu<string>(null, 'say', false);
    expect(openPaneSubmenu(say, 'tell', false)).toEqual({ which: 'tell', focus: false });
    expect(openPaneSubmenu(say, 'say', false)).toBe(say);
  });

  it('keeps a submenu the keyboard opened when the pointer passes over its row again', () => {
    const typed = { which: 'style', focus: true } as const;
    expect(openPaneSubmenu(typed, 'style', false)).toBe(typed);
    // Return or the right arrow on the row focuses its first choice.
    expect(openPaneSubmenu({ which: 'style', focus: false }, 'style', true)).toEqual({
      which: 'style',
      focus: true,
    });
  });
});
