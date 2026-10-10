import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { DEFAULT_VITALS_OPTIONS } from '../ipc/uiConfigVitals';
import type { CombatOpponent } from '../stores/gmcp/combatStore';
import type { Vitals } from '../stores/gmcp/vitalsStore';
import { vitalRows } from '../panel/vitalsView';
import frameCss from '../styles/frame.css?raw';
import { StatusBar, type BarProps } from './StatusBar';
import { BAR_GIVE_WAY, partsGone, tickShare } from './statusBarFit';

// Tolliver at 186 of 412 and low, against a Blackwatch guard at 63.
const HURT: Vitals = {
  hp: 186,
  maxhp: 412,
  mana: 240,
  maxmana: 300,
  move: 180,
  maxmove: 180,
  low: { hp: true, mana: false, move: false },
  hidden: false,
};
const GUARD: CombatOpponent = {
  name: 'a Blackwatch guard',
  hp_pct: 63,
  condition: null,
  hidden: false,
  tank: null,
};
const ROWS = vitalRows(HURT, ['hp', 'mana', 'move'], DEFAULT_VITALS_OPTIONS);

function props(over: Partial<BarProps> = {}): BarProps {
  return {
    style: 'meters',
    connected: true,
    character: 'Tolliver',
    room: null,
    items: { text: null, rows: ROWS, foe: GUARD, target: null },
    inks: { hp: '#ea8f80', mana: '#8fb6e8', move: '#8fdaa8' },
    tick: { secs: 12, warn: false, interval: 30, count: 'up' },
    time: { text: '20:42', tint: null, daytime: false, hour: 20 },
    moons: null,
    roundTrip: 84,
    ...over,
  };
}

const draw = (over: Partial<BarProps> = {}) => renderToStaticMarkup(<StatusBar {...props(over)} />);

describe('StatusBar', () => {
  for (const style of ['meters', 'strip', 'dashboard'] as const) {
    describe(style, () => {
      it('reads each vital with its label and value, then your opponent', () => {
        const html = draw({ style });
        for (const text of ['Health', '186 / 412', 'Mana', '240 / 300', 'Moves', '180 / 180']) {
          expect(html).toContain(text);
        }
        expect(html).toContain('a Blackwatch guard');
        expect(html).toMatch(/is-warn">63%/);
        expect(html.indexOf('Health')).toBeLessThan(html.indexOf('a Blackwatch guard'));
      });

      it('turns low Health the danger tone and colors each vital', () => {
        const html = draw({ style });
        expect(html).toMatch(/is-low">186 \/ 412/);
        expect(html).toContain('--bar-ink:#ea8f80');
        expect(html).toContain('--bar-ink:#8fb6e8');
      });

      it('shows the room in place of your vitals while the panel holds them', () => {
        const html = draw({
          style,
          room: { name: '[Room name]', area: '[Area]' },
          items: { text: null, rows: [], foe: null, target: null },
        });
        expect(html).toContain('[Room name]');
        expect(html).toContain('[Area]');
        expect(html).not.toContain('Health');
      });

      it('reads the tick, the time and the round trip', () => {
        const html = draw({ style });
        expect(html).toContain('12s');
        expect(html).toContain('20:42');
        expect(html).toContain('84ms');
      });

      it('says Not connected while you are not', () => {
        expect(draw({ style, connected: false })).toContain('Not connected');
      });

      it('draws your vitals text where it is handed in', () => {
        const html = draw({
          style,
          items: {
            text: <span className="shell-status-text">text</span>,
            rows: [],
            foe: null,
            target: null,
          },
        });
        expect(html).toContain('class="shell-status-text"');
      });
    });
  }

  it('counts down as Next tick and turns the warn tone', () => {
    const html = draw({ tick: { secs: 2, warn: true, interval: 30, count: 'down' } });
    expect(html).toContain('Next tick');
    expect(html).toContain('shell-bar-tick is-warn');
  });

  it('fills each Meters zone as wide as its value', () => {
    const html = draw();
    expect(html).toMatch(/width:45\.\d+%/);
    expect(html).toContain('width:80%');
    expect(html).toContain('width:63%');
    expect(html).toMatch(/width:40%/);
  });

  it('shows the round trip as signal bars on Strip, fewer as it slows', () => {
    const lit = (ms: number) =>
      (draw({ style: 'strip', roundTrip: ms }).match(/class="is-lit"/g) ?? []).length;
    expect(lit(84)).toBe(3);
    expect(lit(420)).toBe(2);
    expect(lit(1500)).toBe(1);
  });
});

describe('the give way order', () => {
  it('lets each style go in its own order, ending with the time or a name', () => {
    expect(BAR_GIVE_WAY.meters[0]).toBe('moons');
    expect(BAR_GIVE_WAY.strip[0]).toBe('name');
    expect([...partsGone('strip', 2)]).toEqual(['name', 'area']);
    expect(partsGone('dashboard', 0).size).toBe(0);
  });

  it('fills the tick share up or down against the interval', () => {
    expect(tickShare(15, 30, 'up')).toBe(0.5);
    expect(tickShare(10, 30, 'down')).toBeCloseTo(2 / 3);
    expect(tickShare(-4, 30, 'down_past_zero')).toBe(1);
    expect(tickShare(15, null, 'up')).toBeNull();
  });
});

describe('the status bar in frame.css', () => {
  it('sets each style its height, scaled by your panel size', () => {
    expect(frameCss).toMatch(
      /\[data-status-style='meters'\] \{\n {2}--status-line: round\(32px \* var\(--panel-text-px, 12\) \/ 12, 1px\);/,
    );
    expect(frameCss).toMatch(
      /\[data-status-style='dashboard'\] \{\n {2}--status-line: round\(44px/,
    );
  });
});
