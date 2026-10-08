import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { isValidElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { VITALS_OPTIONS_CHANGED } from '../ipc/events';
import { DEFAULT_VITALS_OPTIONS, type VitalsOptions } from '../ipc/uiConfig';
import { openSettingsTab } from '../lib/settingsLink';
import { VitalsChoiceItems, VitalsMenu } from './VitalsMenu';
import {
  pickVitalsStyle,
  pickVitalsValues,
  vitalsStyleChoices,
  vitalsValuesChoices,
} from './vitalsPicks';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));
vi.mock('../lib/settingsLink', () => ({ openSettingsTab: vi.fn() }));
vi.mock('./paneActions', () => ({ returnToCommandLine: vi.fn() }));
// The menu draws in place, with no page to portal into, and each row
// it draws lands in `drawn` with what it does.
interface Row {
  children: ReactNode;
  disabled?: boolean;
  trailing?: ReactNode;
  onSelect?: () => void;
}
const drawn = vi.hoisted(() => ({ rows: [] as Row[] }));
vi.mock('../ui/MenuSurface', async (actual) => ({
  ...(await actual<typeof import('../ui/MenuSurface')>()),
  MenuSurface: ({ label, children }: { label: string; children: ReactNode }) => (
    <menu aria-label={label}>{children}</menu>
  ),
  MenuItem: (row: Row) => {
    drawn.rows.push(row);
    return <li>{row.children}</li>;
  },
}));
// The options the menu reads, the defaults unless a test picks others.
const shown = vi.hoisted(() => ({ options: null as VitalsOptions | null }));
vi.mock('../stores/config/vitalsOptionsStore', async () => {
  const { DEFAULT_VITALS_OPTIONS: defaults } = await import('../ipc/uiConfig');
  const get = () => shown.options ?? defaults;
  return { useVitalsOptions: get, getVitalsOptions: get };
});

beforeEach(() => {
  shown.options = null;
  drawn.rows = [];
  vi.mocked(invoke).mockClear();
  vi.mocked(emit).mockClear();
  vi.mocked(openSettingsTab).mockClear();
});

/** The rows `node` draws. */
function rowsOf(node: ReactNode): Row[] {
  drawn.rows = [];
  renderToStaticMarkup(<>{node}</>);
  return drawn.rows;
}

/** The menu's rows for `options`. */
const menuRows = (options: VitalsOptions) => {
  shown.options = options;
  return rowsOf(<VitalsMenu x={10} y={10} onClose={() => {}} />);
};

/** Each row's text, `(off)` after a disabled one. */
const labels = (rows: Row[]) =>
  rows.map((row) => `${String(row.children)}${row.disabled ? ' (off)' : ''}`);

/** A submenu's rows for `options`, picking with `pick`. */
const styleRows = (options: VitalsOptions, done = () => {}) =>
  rowsOf(
    <VitalsChoiceItems choices={vitalsStyleChoices(options)} pick={pickVitalsStyle} done={done} />,
  );

describe('vitals menu', () => {
  it('offers Style, Values and Customize vitals', () => {
    expect(labels(menuRows(DEFAULT_VITALS_OPTIONS))).toEqual([
      'Style',
      'Values',
      'Customize vitals…',
    ]);
  });

  it('opens Customize vitals in Settings', () => {
    menuRows(DEFAULT_VITALS_OPTIONS)
      .find((row) => row.children === 'Customize vitals…')
      ?.onSelect?.();
    expect(openSettingsTab).toHaveBeenCalledWith('layout:customize-vitals');
  });

  it('adds Edit your text under Text, and Values goes quiet', () => {
    const rows = menuRows({ ...DEFAULT_VITALS_OPTIONS, style: 'text' });
    expect(labels(rows)).toEqual(['Style', 'Values (off)', 'Edit your text…', 'Customize vitals…']);
    rows.find((row) => row.children === 'Edit your text…')?.onSelect?.();
    // It opens the card for your text over the terminal, not Settings.
    expect(emit).toHaveBeenCalledWith('vosh://prompt-card-open', { view: null, vitals: true });
    expect(openSettingsTab).not.toHaveBeenCalled();
  });

  it('checks your style and your Values form', () => {
    const options: VitalsOptions = {
      ...DEFAULT_VITALS_OPTIONS,
      style: 'gauges',
      values: 'percent',
    };
    const rows = styleRows(options);
    expect(labels(rows)).toEqual([
      'Rows',
      'One line',
      'Ledger',
      'Gauges',
      'Pips',
      'Bands',
      'Ladders',
      'Blocks',
      'Traces',
      'Dials',
      'Rings',
      'Vials',
      'Orbs',
      'Candles',
      'Text',
    ]);
    expect(rows.filter((row) => isValidElement(row.trailing)).map((row) => row.children)).toEqual([
      'Gauges',
    ]);
    const values = vitalsValuesChoices(options);
    expect(values.map((c) => c.label)).toEqual(['Current and max', 'Current', 'Percent']);
    expect(values.filter((c) => c.checked).map((c) => c.value)).toEqual(['percent']);
  });

  it('saves a pick alone, then sends one broadcast', async () => {
    const done = vi.fn();
    styleRows(DEFAULT_VITALS_OPTIONS, done)
      .find((row) => row.children === 'Pips')
      ?.onSelect?.();
    await vi.waitFor(() => expect(emit).toHaveBeenCalledTimes(1));
    expect(done).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith('ui_set_fields', {
      fields: [{ field: 'vitals_style', value: 'pips' }],
      profile: null,
    });
    expect(emit).toHaveBeenCalledWith(VITALS_OPTIONS_CHANGED, {
      ...DEFAULT_VITALS_OPTIONS,
      style: 'pips',
    });
  });

  it('puts Rows back in the density and clears the style', async () => {
    shown.options = { ...DEFAULT_VITALS_OPTIONS, style: 'ledger' };
    await pickVitalsStyle('rows');
    expect(invoke).toHaveBeenCalledWith('ui_set_fields', {
      fields: [
        { field: 'vitals_density', value: 'rows' },
        { field: 'vitals_style', value: null },
      ],
      profile: null,
    });
    expect(emit).toHaveBeenCalledWith(VITALS_OPTIONS_CHANGED, {
      ...DEFAULT_VITALS_OPTIONS,
      style: 'rows',
    });
  });

  it('saves a Values form the same way', async () => {
    await pickVitalsValues('current');
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith('ui_set_fields', {
      fields: [{ field: 'vitals_values', value: 'current' }],
      profile: null,
    });
    expect(emit).toHaveBeenCalledTimes(1);
    expect(emit).toHaveBeenCalledWith(VITALS_OPTIONS_CHANGED, {
      ...DEFAULT_VITALS_OPTIONS,
      values: 'current',
    });
  });
});
