import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { normalizeUiConfig, type UiConfig } from '../../../lib/session';
import { SETTINGS_ROWS } from '../../../lib/settingsSearch';
import { VitalsSection } from './LayoutGroup';

// LayoutGroup's stores reach the Tauri bridge. VitalsSection, under
// test, draws from the config it is handed and never calls it.
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

function draw(patch: Partial<UiConfig> = {}): string {
  return renderToStaticMarkup(<VitalsSection config={config(patch)} update={() => undefined} />);
}

/** The pressed segment labels, in order. */
const pressed = (html: string) =>
  [...html.matchAll(/aria-pressed="true"[^>]*>([^<]*)</g)].map((m) => m[1]);

describe('VitalsSection', () => {
  it('draws Density, Values, Meter, and the warning in the board order', () => {
    const html = draw();
    const labels = [...html.matchAll(/class="st-row-label"[^>]*>([^<]*)</g)].map((m) => m[1]);
    expect(labels).toEqual(['Density', 'Values', 'Meter', 'Warn before you run low']);
    expect(html).toContain('Current drops the maximum. Percent matches the Group pane.');
    expect(html).toContain('Bar is easier to read in a fight. None keeps only the numbers.');
    expect(html).toContain(
      'Vitals turn yellow under two thirds and red under one third, like your group&#x27;s health.',
    );
  });

  it('presses the defaults, the panel you had before these rows', () => {
    const html = draw();
    expect(pressed(html)).toEqual(['Rows', 'Current and max', 'Line']);
    expect(html).not.toMatch(/role="switch"[^>]*checked/);
  });

  it('shows the saved choices', () => {
    const html = draw({
      vitals_density: 'line',
      vitals_values: 'percent',
      vitals_meter: 'none',
      vitals_warn_thirds: true,
    });
    expect(pressed(html)).toEqual(['One line', 'Percent', 'None']);
    expect(html).toMatch(/<input[^>]*checked=""[^>]*role="switch"|role="switch"[^>]*checked/);
  });

  it('renders every search anchor Layout, Vitals lists', () => {
    const html = draw();
    const anchors = SETTINGS_ROWS.filter(
      (r) => r.target.group === 'layout' && r.target.section === 'vitals',
    ).map((r) => r.target.anchor);
    expect(anchors).toEqual(['density', 'values', 'meter', 'warn-low']);
    for (const anchor of anchors) {
      expect(html).toContain(`data-st-anchor="${anchor}"`);
    }
  });
});
