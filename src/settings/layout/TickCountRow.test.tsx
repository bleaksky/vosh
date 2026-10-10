import type { ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { normalizeUiConfig, type TickCount, type UiConfig } from '../../ipc/uiConfig';
import { SETTINGS_ROWS } from '../settingsSearch';
import { StatusLineSection } from './LayoutPage';
import type { SegmentedProps } from '../../ui';
import { TickCountField } from './TickCountRow';

// The rows' save hook and LayoutPage's stores reach the Tauri bridge.
// The rows under test draw from the config they are handed.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

const config = (patch: Partial<UiConfig> = {}): UiConfig => ({
  ...normalizeUiConfig({
    theme: 'nord',
    auto_update: false,
    font_family: 'Menlo',
    font_size: 14,
    tracked_affects: [],
    enabled_presets: [],
  }),
  ...patch,
});

function draw(cfg: UiConfig | null): string {
  return renderToStaticMarkup(<TickCountField config={cfg} update={() => undefined} />);
}

/** The segment labels, in order. */
const segments = (html: string) =>
  [...html.matchAll(/class="st-seg-item"[^>]*>([^<]*)</g)].map((m) => m[1]);
/** The pressed segment labels. */
const pressed = (html: string) =>
  [...html.matchAll(/aria-pressed="true"[^>]*>([^<]*)</g)].map((m) => m[1]);
/** The row labels, in order. */
const labels = (html: string) =>
  [...html.matchAll(/class="st-row-label"[^>]*>([^<]*)</g)].map((m) => m[1]);

describe('TickCountField', () => {
  it('offers Up, Down, and Down past 0 under Tick counts', () => {
    const html = draw(config());
    expect(labels(html)).toEqual(['Tick counts']);
    expect(segments(html)).toEqual(['Up', 'Down', 'Down past 0']);
    expect(html).toContain(
      'Up shows the seconds since the last tick and Down the seconds left until the next. ' +
        'Down waits at 0 when the game is late, and Down past 0 keeps counting below zero ' +
        'until the tick lands.',
    );
    expect(html).toContain('data-st-anchor="tick-counts"');
  });

  it('presses Up by default and the saved count after', () => {
    expect(pressed(draw(config()))).toEqual(['Up']);
    expect(pressed(draw(config({ tick_count: 'down' })))).toEqual(['Down']);
    expect(pressed(draw(config({ tick_count: 'down_past_zero' })))).toEqual(['Down past 0']);
  });

  it('waits for the config before you can press anything', () => {
    const html = draw(null);
    expect(pressed(html)).toEqual([]);
    expect(html.match(/<button[^>]*disabled=""/g)).toHaveLength(3);
  });

  it('saves the count you press', () => {
    const update = vi.fn();
    const row = TickCountField({ config: config(), update }) as ReactElement<{
      children: ReactElement<SegmentedProps<TickCount>>;
    }>;
    const segmented = row.props.children;
    for (const count of ['down', 'down_past_zero', 'up'] as const) {
      segmented.props.onChange(count);
      expect(update).toHaveBeenLastCalledWith({ tick_count: count });
    }
    expect(update).toHaveBeenCalledTimes(3);
  });

  it('is found under Layout, Status line', () => {
    const row = SETTINGS_ROWS.find((r) => r.label === 'Tick counts');
    expect(row?.target).toEqual({ group: 'layout', section: 'status', anchor: 'tick-counts' });
  });
});

describe('StatusLineSection', () => {
  it('puts Game time under Tick and time, and Tick counts under them', () => {
    const html = renderToStaticMarkup(
      <StatusLineSection
        config={config({ chip_style: 'icon_value', game_time: '12h', tick_count: 'down' })}
        setConfig={() => undefined}
        onError={() => undefined}
      />,
    );
    expect(labels(html)).toEqual(['Style', 'Tick and time', 'Game time', 'Tick counts']);
    expect(pressed(html)).toEqual(['Icon', '12 hour', 'Down']);
    const anchors = SETTINGS_ROWS.filter(
      (r) => r.target.group === 'layout' && r.target.section === 'status',
    ).map((r) => r.target.anchor);
    expect(anchors).toEqual(['tick-time', 'game-time', 'tick-counts', 'status-style']);
    for (const anchor of anchors) expect(html).toContain(`data-st-anchor="${anchor}"`);
  });
});
