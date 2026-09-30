import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { normalizeUiConfig, type UiConfig } from '../../../lib/session';
import { SETTINGS_ROWS } from '../../../lib/settingsSearch';
import { AffectsSection, VitalsSection } from './LayoutGroup';

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

function drawAffects(patch: Partial<UiConfig> = {}): string {
  return renderToStaticMarkup(<AffectsSection config={config(patch)} update={() => undefined} />);
}

/** Each segment of each row as `name pressed disabled`, in order. */
const segments = (html: string) =>
  [...html.matchAll(/<button type="button" class="st-seg-item"([^>]*)>(.*?)<\/button>/g)].map(
    ([, attrs, inner]) => {
      const name = /aria-label="([^"]*)"/.exec(attrs)?.[1] ?? inner;
      const on = attrs.includes('aria-pressed="true"') ? ' pressed' : '';
      const off = attrs.includes('disabled') ? ' disabled' : '';
      return `${name}${on}${off}`;
    },
  );

describe('AffectsSection', () => {
  it('draws Style and Marker, Timers first and the dot by default', () => {
    const html = drawAffects();
    const labels = [...html.matchAll(/class="st-row-label"[^>]*>([^<]*)</g)].map((m) => m[1]);
    expect(labels).toEqual(['Style', 'Marker', 'Tint what to recast']);
    expect(html).toMatch(/<h2[^>]*>Affects<\/h2>/);
    expect(html).toContain(
      'Timers first keeps your slots, Countdown sorts by hours left, and Grouped chips puts what to recast first.',
    );
    expect(html).toContain(
      'It sits beside each affect you track, and its color shows whether the affect is up, running out, or missing.',
    );
    expect(segments(html)).toEqual([
      'Timers first pressed',
      'Countdown',
      'Grouped chips',
      'Dot pressed',
      'Square',
      'Plus and minus',
      'None',
    ]);
    expect(html).toContain(
      'A missing affect sits on a red wash, and one about to drop sits on yellow or red.',
    );
    expect(html).not.toMatch(/role="switch"[^>]*checked/);
  });

  it('shows the tint at its saved value, and quiet while Grouped chips are chosen', () => {
    const on = drawAffects({ affects_tint: true });
    expect(on).toMatch(/<input[^>]*checked=""[^>]*role="switch"|role="switch"[^>]*checked/);
    expect(on).not.toMatch(/role="switch"[^>]*disabled|disabled=""[^>]*role="switch"/);
    // Chips mark what to recast on their own, and the toggle keeps the
    // value your other styles use.
    const chips = drawAffects({ affects_style: 'chips', affects_tint: false });
    expect(chips).toContain('Grouped chips always mark what to recast.');
    expect(chips).toMatch(/<input disabled=""[^>]*role="switch"/);
    expect(chips).not.toMatch(/<input[^>]*checked=""[^>]*role="switch"|role="switch"[^>]*checked/);
  });

  it('draws each marker as the pane draws it, the mark you have and the one you miss', () => {
    const html = drawAffects();
    for (const marker of ['dot', 'square', 'plus_minus']) {
      expect(html).toContain(
        `<span class="st-marker" data-affects-marker="${marker}" aria-hidden="true"><span class="pane-affect-mark is-up"></span><span class="pane-affect-mark is-missing"></span></span>`,
      );
    }
    expect(html).not.toContain('data-affects-marker="none"');
  });

  it('keeps your marker but quiets the row while Grouped chips are chosen', () => {
    const html = drawAffects({ affects_style: 'chips', affects_marker: 'square' });
    expect(segments(html)).toEqual([
      'Timers first',
      'Countdown',
      'Grouped chips pressed',
      'Dot disabled',
      'Square pressed disabled',
      'Plus and minus disabled',
      'None disabled',
    ]);
    expect(html).toContain('Grouped chips show the state on each chip, so they draw no marker.');
  });

  it('shows the saved style and marker', () => {
    const html = drawAffects({ affects_style: 'countdown', affects_marker: 'none' });
    expect(segments(html)).toContain('Countdown pressed');
    expect(segments(html)).toContain('None pressed');
  });

  it('renders every search anchor Layout, Affects lists', () => {
    const html = drawAffects();
    const anchors = SETTINGS_ROWS.filter(
      (r) => r.target.group === 'layout' && r.target.section === 'affects',
    ).map((r) => r.target.anchor);
    expect(anchors).toEqual(['affects-style', 'affects-marker', 'affects-tint']);
    for (const anchor of anchors) {
      expect(html).toContain(`data-st-anchor="${anchor}"`);
    }
    expect(html).toContain('data-st-anchor="affects"');
  });
});
