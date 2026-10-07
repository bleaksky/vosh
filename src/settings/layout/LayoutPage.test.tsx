import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { normalizeUiConfig, type UiConfig } from '../../ipc/uiConfig';
import { SETTINGS_ROWS } from '../settingsSearch';
import { AffectsSection, VitalsSection } from './LayoutPage';

// LayoutPage's stores reach the Tauri bridge. VitalsSection, under
// test, draws from the config it is handed and never calls it.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));
// The gallery reads the window's stores and has tests of its own
// (VitalsGallery.test.tsx). Here it stands in as its anchor.
vi.mock('./VitalsGallery', () => ({
  VitalsGallery: () => <fieldset data-st-anchor="style" />,
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

/** Whether each switch is on, in order. */
const switches = (html: string) =>
  [...html.matchAll(/<input[^>]*role="switch"[^>]*>/g)].map((m) => m[0].includes('checked=""'));

describe('VitalsSection', () => {
  it('draws the gallery, then Show your vitals in and the pinned switch', () => {
    const html = draw();
    expect(html.indexOf('data-st-anchor="style"')).toBeLessThan(html.indexOf('st-row-label'));
    const labels = [...html.matchAll(/class="st-row-label"[^>]*>([^<]*)</g)].map((m) => m[1]);
    expect(labels).toEqual(['Show your vitals in', 'Hide vitals while your prompt is pinned']);
    expect(html).not.toContain('Density');
    expect(html).toContain(
      'Status line moves them under the terminal in the line&#x27;s quiet form, and the panes take the footer&#x27;s room.',
    );
    expect(html).toContain(
      'While your prompt is pinned, the panes take their room, and your opponent keeps its row in a fight. Turn it off if your prompt leaves your vitals out.',
    );
    // Values, Meter and the warning sit under Customize vitals.
    expect(html).not.toContain('Current drops the maximum.');
  });

  it('says under Text that only the rows reading your fight stay while your prompt is pinned', () => {
    const html = draw({ vitals_style: 'text' });
    expect(html).toContain(
      'While your prompt is pinned, only the rows of your text that read your fight stay, and the panes take the rest. Turn it off if your prompt leaves your vitals out.',
    );
  });

  it('presses the defaults, the panel you had before these rows', () => {
    const html = draw();
    expect(pressed(html)).toEqual(['Panel']);
    // Hiding the vitals under a pinned prompt starts on, as the
    // recommended choice you can turn off.
    expect(switches(html)).toEqual([true]);
  });

  it('shows the saved choices', () => {
    const html = draw({ vitals_place: 'status', vitals_hide_when_pinned: false });
    expect(pressed(html)).toEqual(['Status line']);
    expect(switches(html)).toEqual([false]);
  });

  it('saves the place and the pinned switch alone', () => {
    const update = vi.fn();
    const element = VitalsSection({ config: config(), update });
    // The gallery comes first, then Show your vitals in and the switch.
    const rows = (element.props as { children: { props: { children: unknown } }[] }).children;
    const place = rows[1].props.children as { props: { onChange: (place: string) => void } };
    place.props.onChange('status');
    expect(update).toHaveBeenLastCalledWith({ vitals_place: 'status' });
    const toggle = rows[2].props.children as { props: { onChange: (on: boolean) => void } };
    toggle.props.onChange(false);
    expect(update).toHaveBeenLastCalledWith({ vitals_hide_when_pinned: false });
  });

  it('renders every search anchor Layout, Vitals lists', () => {
    const html = draw();
    const anchors = SETTINGS_ROWS.filter(
      (r) => r.target.group === 'layout' && r.target.section === 'vitals',
    ).map((r) => r.target.anchor);
    expect(anchors).toEqual(['style', 'place', 'hide-pinned']);
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

/** Each option of the Style select as `label selected`, in order. */
const styles = (html: string) =>
  [...html.matchAll(/<option value="([^"]*)"( selected="")?>([^<]*)<\/option>/g)].map(
    ([, , selected, label]) => `${label}${selected ? ' selected' : ''}`,
  );

describe('AffectsSection', () => {
  it('draws Style and Marker, Timers first and the dot by default', () => {
    const html = drawAffects();
    const labels = [...html.matchAll(/class="st-row-label"[^>]*>([^<]*)</g)].map((m) => m[1]);
    expect(labels).toEqual([
      'Style',
      'Marker',
      'Tint what to recast',
      'Running out at',
      'Almost gone at',
    ]);
    expect(html).toMatch(/<h2[^>]*>Affects<\/h2>/);
    expect(html).toContain(
      'Timers first keeps your slots, Countdown sorts by hours left, Grouped chips puts what to recast first, and Draining chips colors only the hours a chip has left.',
    );
    expect(html).toContain(
      'It sits beside each affect you track, and its color shows whether the affect is up, running out, or missing.',
    );
    // Four styles do not fit as segments beside the words, so they sit
    // in a select.
    expect(styles(html)).toEqual([
      'Timers first selected',
      'Countdown',
      'Grouped chips',
      'Draining chips',
    ]);
    expect(segments(html)).toEqual(['Dot pressed', 'Square', 'Plus and minus', 'None']);
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
    // Draining chips the same way.
    const drain = drawAffects({ affects_style: 'chips_drain', affects_tint: true });
    expect(drain).toContain('Draining chips always mark what to recast.');
    expect(drain).toMatch(/<input[^>]*disabled=""[^>]*role="switch"/);
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
    expect(styles(html)).toContain('Grouped chips selected');
    expect(segments(html)).toEqual([
      'Dot disabled',
      'Square pressed disabled',
      'Plus and minus disabled',
      'None disabled',
    ]);
    expect(html).toContain('Grouped chips show the state on each chip, so they draw no marker.');
    const drain = drawAffects({ affects_style: 'chips_drain', affects_marker: 'square' });
    expect(styles(drain)).toContain('Draining chips selected');
    expect(segments(drain)).toEqual([
      'Dot disabled',
      'Square pressed disabled',
      'Plus and minus disabled',
      'None disabled',
    ]);
    expect(drain).toContain('Draining chips show the state on each chip, so they draw no marker.');
  });

  it('shows the saved style and marker', () => {
    const html = drawAffects({ affects_style: 'countdown', affects_marker: 'none' });
    expect(styles(html)).toContain('Countdown selected');
    expect(segments(html)).toContain('None pressed');
  });

  /** Each number field as its value and the unit a screen reader
   *  hears, in order. */
  const hours = (html: string) =>
    [
      ...html.matchAll(
        /<span class="st-number"[^>]*><input[^>]*value="([^"]*)"[^>]*>.*?<span id="[^"]*" hidden="">([^<]*)<\/span><\/span>/g,
      ),
    ].map(([, value, unit]) => `${value} ${unit}`);

  it('shows when affects warn and turn red, two and one hours by default', () => {
    const html = drawAffects();
    expect(hours(html)).toEqual(['2 hours', '1 hours']);
    expect(html).toContain(
      'With this many hours or fewer an affect&#x27;s hours turn yellow, and one you track counts as running out.',
    );
    expect(html).toContain(
      'With this many hours or fewer the hours turn bold red. The game&#x27;s own affects bar turns red at 1.',
    );
    expect(
      hours(drawAffects({ affects_running_out_hours: 5, affects_almost_gone_hours: 2 })),
    ).toEqual(['5 hours', '2 hours']);
    // The hours apply to every style, so the rows never go quiet.
    const chips = drawAffects({ affects_style: 'chips_drain' });
    const rowsFrom = chips.slice(chips.indexOf('data-st-anchor="affects-running-out"'));
    expect(rowsFrom).toContain('data-st-anchor="affects-almost-gone"');
    expect(rowsFrom).not.toContain('disabled');
    expect(hours(chips)).toEqual(['2 hours', '1 hours']);
  });

  it('keeps almost gone at or under running out, running out winning as the backend does', () => {
    type Field = { props: { min: number; max: number; onChange: (n: number) => void } };
    const fields = (runningOut: number, almostGone: number) => {
      const update = vi.fn();
      const element = AffectsSection({
        config: config({
          affects_running_out_hours: runningOut,
          affects_almost_gone_hours: almostGone,
        }),
        update,
      });
      const rows = (element.props as { children: { props: { children: unknown } }[] }).children;
      return {
        update,
        runningOut: rows[3].props.children as Field,
        almostGone: rows[4].props.children as Field,
      };
    };
    const { update, runningOut, almostGone } = fields(5, 2);
    // Running out takes any hours. Almost gone stops at running out.
    expect([runningOut.props.min, runningOut.props.max]).toEqual([0, 99]);
    expect([almostGone.props.min, almostGone.props.max]).toEqual([0, 5]);
    runningOut.props.onChange(7);
    expect(update).toHaveBeenLastCalledWith({ affects_running_out_hours: 7 });
    // Down to almost gone, almost gone stays.
    runningOut.props.onChange(2);
    expect(update).toHaveBeenLastCalledWith({ affects_running_out_hours: 2 });
    // Under almost gone, running out takes almost gone down with it.
    runningOut.props.onChange(1);
    expect(update).toHaveBeenLastCalledWith({
      affects_running_out_hours: 1,
      affects_almost_gone_hours: 1,
    });
    almostGone.props.onChange(0);
    expect(update).toHaveBeenLastCalledWith({ affects_almost_gone_hours: 0 });
    // At the defaults, 0 in Running out saves 0 for both, and no
    // longer snaps back up to 1.
    const defaults = fields(2, 1);
    expect(defaults.runningOut.props.min).toBe(0);
    defaults.runningOut.props.onChange(0);
    expect(defaults.update).toHaveBeenLastCalledWith({
      affects_running_out_hours: 0,
      affects_almost_gone_hours: 0,
    });
  });

  it('renders every search anchor Layout, Affects lists', () => {
    const html = drawAffects();
    const anchors = SETTINGS_ROWS.filter(
      (r) => r.target.group === 'layout' && r.target.section === 'affects',
    ).map((r) => r.target.anchor);
    expect(anchors).toEqual([
      'affects-style',
      'affects-marker',
      'affects-tint',
      'affects-running-out',
      'affects-almost-gone',
    ]);
    for (const anchor of anchors) {
      expect(html).toContain(`data-st-anchor="${anchor}"`);
    }
    expect(html).toContain('data-st-anchor="affects"');
  });
});
