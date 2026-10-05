import type { ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { normalizeUiConfig, type GameTime, type UiConfig } from '../../../ipc/uiConfig';
import { SETTINGS_ROWS, searchSettingsRows, settingsRowKey } from '../../../lib/settingsSearch';
import type { SegmentedProps } from '../ui';
import { GameTimeField } from './GameTimeRow';

// The row's save hook reaches the Tauri bridge. The row under test
// draws from the config it is handed.
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
  return renderToStaticMarkup(<GameTimeField config={cfg} update={() => undefined} />);
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

describe('GameTimeField', () => {
  it('offers 24 hour and 12 hour under Game time', () => {
    const html = draw(config());
    expect(labels(html)).toEqual(['Game time']);
    expect(segments(html)).toEqual(['24 hour', '12 hour']);
    expect(html).toContain('How the game time shows in the status line, like 18:00 or 6:00 PM.');
    expect(html).toContain('data-st-anchor="game-time"');
  });

  it('presses 24 hour by default and the saved clock after', () => {
    expect(config().game_time).toBe('24h');
    expect(pressed(draw(config()))).toEqual(['24 hour']);
    expect(pressed(draw(config({ game_time: '12h' })))).toEqual(['12 hour']);
  });

  it('reads a clock it does not know as 24 hour', () => {
    const cfg = normalizeUiConfig({ ...config(), game_time: 'sundial' } as never);
    expect(cfg.game_time).toBe('24h');
  });

  it('waits for the config before you can press anything', () => {
    const html = draw(null);
    expect(pressed(html)).toEqual([]);
    expect(html.match(/<button[^>]*disabled=""/g)).toHaveLength(2);
  });

  it('saves the clock you press', () => {
    const update = vi.fn();
    const row = GameTimeField({ config: config(), update }) as ReactElement<{
      children: ReactElement<SegmentedProps<GameTime>>;
    }>;
    const segmented = row.props.children;
    for (const clock of ['12h', '24h'] as const) {
      segmented.props.onChange(clock);
      expect(update).toHaveBeenLastCalledWith({ game_time: clock });
    }
    expect(update).toHaveBeenCalledTimes(2);
  });

  it('is found under Layout, Status line', () => {
    const row = SETTINGS_ROWS.find((r) => r.label === 'Game time');
    expect(row?.target).toEqual({ group: 'layout', section: 'status', anchor: 'game-time' });
    const mac = { pathB: false, mac: true };
    for (const query of ['12 hour', 'am pm', 'game time']) {
      const [first] = searchSettingsRows(query, mac);
      expect(first?.label, query).toBe('Game time');
      expect(settingsRowKey(first)).toBe('layout:status#game-time');
    }
  });
});
