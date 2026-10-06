import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { SessionRow } from '../ipc/session';
import { parseCombat } from '../stores/gmcp/combatStore';
import { parseRoomInfo } from '../stores/gmcp/roomStore';
import { nextVitals, type Vitals } from '../stores/gmcp/vitalsStore';
import type { SessionRowState } from '../stores/session/sessionRowStore';
import { aabahranPacket } from '../test/aabahranGmcp';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../test/fakeDom';
import { cardFacts, cardWords, type CardFacts } from './cardFacts';
import { SessionCard } from './SessionCard';
import type { SessionView } from './sessionLine';
import { CARD_DELAY, useHoverCard } from './useHoverCard';

// The card beside a session's row, S6 of the Sessions Sidebar review,
// boards 03 and 07. What it says comes from the session's view, with
// the room and the fight from the game's own GMCP fixtures. When it
// shows runs on fake timers against the fake DOM.

const PLAY = 'play.theforsakenlands.com';
const NOW = 10_000_000;

const row = (fields: Partial<SessionRow> = {}): SessionRow => ({
  id: 3,
  name: null,
  character: 'Orla',
  host: PLAY,
  port: 1825,
  tls: false,
  profile: 'Build',
  connected: true,
  since: null,
  selected: false,
  ...fields,
});

const QUIET: SessionRowState = {
  link: null,
  redialing: false,
  reached: false,
  playing: false,
  lines: false,
  waiting: [],
  downAt: null,
  refused: false,
  try: null,
  tries: null,
};

const view = (fields: Partial<SessionView> = {}, state: Partial<SessionRowState> = {}) => ({
  state: { ...QUIET, link: 'live' as const, playing: true, ...state },
  room: null,
  combat: null,
  vitals: null,
  now: NOW,
  ...fields,
});

const BANK = parseRoomInfo(aabahranPacket('room-info.gmcp').data);
const GUARD = parseCombat(aabahranPacket('char-combat.gmcp').data);
const vitals = (hp: number): Vitals | null =>
  nextVitals(null, { hp, maxhp: 900, mana: 760, maxmana: 820, move: 250, maxmove: 250 });

describe('what the card says', () => {
  it('reads board 03: the name, the world with the profile, and every fact', () => {
    const facts = cardFacts(
      row({ since: NOW - 72 * 60_000 }),
      [row()],
      view(
        { room: BANK, combat: GUARD, vitals: vitals(162) },
        { waiting: ['preset:alert_attacked', 'preset:alert_low_health'] },
      ),
    );
    expect(facts).toEqual({
      name: 'Orla',
      port: '1825',
      where: 'The Forsaken Lands 1825, profile Build',
      facts: [
        { label: 'Room', value: 'The Bank of Aabahran', low: false },
        { label: 'Area', value: 'Fort Blackwatch', low: false },
        { label: 'Fighting', value: 'a Blackwatch guard', low: false },
        { label: 'Health', value: '162 / 900', low: true },
        { label: 'Mana', value: '760 / 820', low: false },
        { label: 'Moves', value: '250 / 250', low: false },
        { label: 'Online', value: '1 h 12 min', low: false },
        { label: 'Waiting', value: 'A fight started, health is low', low: false },
      ],
    });
    expect(cardWords(facts)).toBe(
      'The Forsaken Lands 1825, profile Build. Room The Bank of Aabahran. Area Fort Blackwatch. ' +
        'Fighting a Blackwatch guard. Health 162 / 900. Mana 760 / 820. Moves 250 / 250. ' +
        'Online 1 h 12 min. Waiting A fight started, health is low. Double click the name to rename.',
    );
  });

  it('drops every row the session has no data for', () => {
    const facts = cardFacts(row(), [row()], view());
    expect(facts.facts).toEqual([]);
    expect(cardWords(facts)).toBe(
      'The Forsaken Lands 1825, profile Build. Double click the name to rename.',
    );
    // The game hides your vitals, and you have been online under a minute.
    const hidden: Vitals = { ...vitals(500)!, hidden: true };
    expect(
      cardFacts(row({ since: NOW - 20_000 }), [row()], view({ room: BANK, vitals: hidden })).facts,
    ).toEqual([
      { label: 'Room', value: 'The Bank of Aabahran', low: false },
      { label: 'Area', value: 'Fort Blackwatch', low: false },
      { label: 'Online', value: 'Under a minute', low: false },
    ]);
  });

  it('keeps the character of a session you named', () => {
    const named = row({ name: 'Errands', character: 'Maren', port: 1848, profile: 'Default' });
    expect(cardFacts(named, [named], view())).toMatchObject({
      name: 'Errands',
      port: null,
      where: 'Maren on The Forsaken Lands, profile Default',
    });
  });

  it('shows the world, the profile and when it last played while it is not connected', () => {
    const down = row({ connected: false });
    const facts = cardFacts(
      down,
      [down],
      view({ room: BANK, vitals: vitals(900) }, { link: 'down', downAt: NOW - 40 * 60_000 }),
    );
    expect(facts.where).toBe('The Forsaken Lands 1825, profile Build');
    expect(facts.facts).toEqual([{ label: 'Played', value: '40 min ago', low: false }]);
    // Never played in this run of Vosh.
    expect(cardFacts(down, [down], view({}, { link: null })).facts).toEqual([]);
  });

  it('names a row read by its world with the port apart', () => {
    const world = row({ character: null, profile: null });
    expect(cardFacts(world, [world], view({}, { link: 'down' }))).toMatchObject({
      name: 'The Forsaken Lands',
      port: '1825',
      where: 'The Forsaken Lands 1825',
    });
  });
});

type Handler = (e?: unknown) => void;

/** The handlers React keeps on an element. This DOM sends no events. */
function on(el: FakeElement): Record<string, Handler> {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$'));
  if (!key) throw new Error('the element has no React props');
  return (el as unknown as Record<string, Record<string, Handler>>)[key];
}

const FACTS: CardFacts = {
  name: 'Orla',
  port: '1825',
  where: 'The Forsaken Lands 1825, profile Build',
  facts: [{ label: 'Room', value: 'The Bank of Aabahran', low: false }],
};

/** Three rows that open the card as the sidebar's do, beside a side. */
function Rows({ lifted, side }: { lifted: boolean; side: HTMLElement }) {
  const card = useHoverCard(lifted);
  return (
    <ul onPointerLeave={card.leave}>
      {[1, 2, 3].map((session) => (
        <li
          key={session}
          data-session={session}
          onPointerMove={(e) => card.rest(session, e.currentTarget)}
        >
          {card.shown?.session === session && (
            <SessionCard
              facts={{ ...FACTS, name: `Row ${session}` }}
              slot={card.shown.slot}
              side={side}
            />
          )}
        </li>
      ))}
    </ul>
  );
}

describe('when the card shows', () => {
  const doc = new FakeDocument();
  const listeners = new Map<string, Handler[]>();
  const fire = (type: string) => {
    for (const fn of listeners.get(type) ?? []) fn({});
  };
  let createRoot: typeof import('react-dom/client').createRoot;
  let unmount: (() => void) | null = null;

  beforeEach(async () => {
    vi.useFakeTimers();
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    vi.stubGlobal('document', doc);
    vi.stubGlobal('window', {
      document: doc,
      innerWidth: 1280,
      innerHeight: 800,
      location: { protocol: 'about:' },
      HTMLIFrameElement: class {},
      addEventListener: (type: string, fn: Handler) =>
        void listeners.set(type, [...(listeners.get(type) ?? []), fn]),
      removeEventListener: (type: string, fn: Handler) =>
        void listeners.set(
          type,
          (listeners.get(type) ?? []).filter((had) => had !== fn),
        ),
    });
    vi.stubGlobal('navigator', { userAgent: 'Macintosh', platform: '' });
    vi.stubGlobal('Node', FakeNode);
    vi.stubGlobal('Element', FakeElement);
    vi.stubGlobal('HTMLElement', FakeElement);
    // Each slot is 46 high from 59, the sidebar ends at 221.
    const el = FakeElement.prototype as unknown as Record<string, unknown>;
    el.getBoundingClientRect = function (this: FakeElement) {
      const session = Number(this.getAttribute('data-session') ?? 0);
      return { top: 59 + (session - 1) * 46, right: 221 };
    };
    ({ createRoot } = await import('react-dom/client'));
  });

  afterEach(() => {
    act(() => unmount?.());
    unmount = null;
    listeners.clear();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  function mount() {
    const side = doc.createElement('aside');
    const container = doc.createElement('div');
    doc.body.appendChild(container);
    const root = createRoot(container as unknown as HTMLElement);
    const draw = (lifted: boolean) =>
      act(() => root.render(<Rows lifted={lifted} side={side as unknown as HTMLElement} />));
    draw(false);
    unmount = () => {
      root.unmount();
      doc.body.removeChild(container);
    };
    const slot = (session: number) =>
      findAll(container, (el) => el.getAttribute('data-session') === String(session))[0];
    const list = findAll(container, (el) => el.nodeName === 'UL')[0];
    return {
      draw,
      point: (session: number) =>
        act(() => on(slot(session)).onPointerMove({ currentTarget: slot(session) })),
      wait: (ms: number) => act(() => void vi.advanceTimersByTime(ms)),
      leave: () => act(() => on(list).onPointerLeave({})),
      fire: (type: string) => act(() => fire(type)),
      /** The card in the body, or null. */
      card: () =>
        findAll(doc.body, (el) => el.getAttribute('class') === 'shell-sessions-card')[0] ?? null,
    };
  }

  const name = (card: FakeElement | null) =>
    card &&
    findAll(card, (el) => el.getAttribute('class') === 'shell-sessions-card-name')[0].textContent;

  it('opens after 500 ms at rest, level with the row and 12 right of the line', () => {
    const m = mount();
    m.point(2);
    m.wait(CARD_DELAY - 1);
    expect(m.card()).toBeNull();
    m.wait(1);
    const card = m.card();
    expect(name(card)).toBe('Row 2');
    expect(card?.style.left).toBe('233px');
    expect(card?.style.top).toBe('105px');
    expect(card?.getAttribute('aria-hidden')).toBe('true');
  });

  it('starts the wait again while the pointer moves', () => {
    const m = mount();
    m.point(1);
    m.wait(300);
    m.point(2);
    m.wait(300);
    expect(m.card()).toBeNull();
    m.wait(200);
    expect(name(m.card())).toBe('Row 2');
  });

  it('moves at once to the next row while it stays open', () => {
    const m = mount();
    m.point(1);
    m.wait(CARD_DELAY);
    m.point(3);
    expect(name(m.card())).toBe('Row 3');
    expect(m.card()?.style.top).toBe('151px');
  });

  it('closes when the pointer leaves the rows, and waits again after', () => {
    const m = mount();
    m.point(1);
    m.wait(CARD_DELAY);
    m.leave();
    expect(m.card()).toBeNull();
    m.point(1);
    expect(m.card()).toBeNull();
    m.wait(CARD_DELAY);
    expect(name(m.card())).toBe('Row 1');
  });

  it('closes on a key and on a click, and that row stays quiet until you leave it', () => {
    for (const type of ['keydown', 'pointerdown']) {
      const m = mount();
      m.point(1);
      m.wait(CARD_DELAY);
      m.fire(type);
      expect(m.card()).toBeNull();
      m.point(1);
      m.wait(CARD_DELAY);
      expect(m.card()).toBeNull();
      // The next row opens it again.
      m.point(2);
      m.wait(CARD_DELAY);
      expect(name(m.card())).toBe('Row 2');
      act(() => unmount?.());
      unmount = null;
    }
  });

  it('closes a card that waits too, on a key', () => {
    const m = mount();
    m.point(1);
    m.wait(200);
    m.fire('keydown');
    m.wait(CARD_DELAY);
    expect(m.card()).toBeNull();
  });

  it('closes while a row is in the air, and opens on none', () => {
    const m = mount();
    m.point(1);
    m.wait(CARD_DELAY);
    m.draw(true);
    expect(m.card()).toBeNull();
    m.point(2);
    m.wait(CARD_DELAY);
    expect(m.card()).toBeNull();
    m.draw(false);
    m.point(2);
    m.wait(CARD_DELAY);
    expect(name(m.card())).toBe('Row 2');
  });

  it('never takes the focus', () => {
    const field = doc.createElement('input');
    field.focus();
    const m = mount();
    m.point(1);
    m.wait(CARD_DELAY);
    m.point(2);
    expect(m.card()).not.toBeNull();
    expect(doc.activeElement).toBe(field);
  });
});
