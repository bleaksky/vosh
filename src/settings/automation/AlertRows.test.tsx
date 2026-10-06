import { act, useState } from 'react';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { TriggerRecord } from '../../ipc/automation';
import { FakeDocument, FakeElement, findAll } from '../../test/fakeDom';

// The four rows that close a trigger card under Advanced and tune its
// alert, board 1 of the Alerts review: Sound with its play button,
// Bounce, Banner shows and the switch. This mounts the card on one
// trigger, opens Advanced, and drives the rows through the handlers
// React keeps on each element, since this DOM sends no events.

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => undefined)),
  emit: vi.fn(() => Promise.resolve()),
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
// CodeMirror needs a real DOM, and Advanced holds the Lua script row.
vi.mock('../../ui/CodeEditor', () => ({ CodeEditor: () => null }));
vi.mock('../../stores/session/promptGagStore', () => ({
  usePromptGags: () => new Set<string>(),
}));

/** Each tone the play button sounded, in order. */
const played = vi.hoisted(() => [] as string[]);
vi.mock('../../stores/session/alertTones', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../stores/session/alertTones')>()),
  playAlertTone: (tone: string) => played.push(tone),
}));

const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;
let TriggerDetail: typeof import('./TriggersEditor').TriggerDetail;

beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('document', doc);
  vi.stubGlobal('window', {
    document: doc,
    location: { protocol: 'about:' },
    HTMLIFrameElement: class {},
    addEventListener() {},
    removeEventListener() {},
    setTimeout: globalThis.setTimeout.bind(globalThis),
    clearTimeout: globalThis.clearTimeout.bind(globalThis),
  });
  vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
  ({ createRoot } = await import('react-dom/client'));
  ({ TriggerDetail } = await import('./TriggersEditor'));
});

type Handler = (e?: unknown) => void;

/** The props React keeps on an element, its handlers among them. */
function props(el: FakeElement): Record<string, Handler> & Record<string, unknown> {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$'));
  if (!key) throw new Error('the element has no React props');
  return (el as unknown as Record<string, Record<string, Handler>>)[key];
}

const cleanups: (() => Promise<void>)[] = [];
afterEach(async () => {
  for (const clean of cleanups.splice(0)) await clean();
  played.splice(0);
  delete doc.documentElement.dataset.platform;
});

/** The visitor of board 1, with no alert yet. */
const VISITOR: TriggerRecord = {
  name: 'visitor',
  patterns: [{ pattern: '^(\\w+) walks in\\.$', enabled: true }],
  priority: 5,
  enabled: true,
  actions: [],
};

/** The card on `start` with Advanced open. */
async function mount(start: TriggerRecord) {
  let current = start;
  function Card() {
    const [value, setValue] = useState(start);
    current = value;
    return (
      <TriggerDetail
        uid="t1"
        value={value}
        update={(fn) => setValue(fn)}
        fresh={false}
        revealInList={() => {}}
      />
    );
  }
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  await act(async () => root.render(<Card />));
  cleanups.push(async () => {
    await act(async () => root.unmount());
    doc.body.removeChild(container);
  });
  const fire = (el: FakeElement | undefined, name: string, event: unknown) =>
    act(async () => {
      if (!el) throw new Error(`nothing to ${name}`);
      props(el)[name](event);
    });
  const advanced = findAll(
    container,
    (el) => el.nodeName === 'BUTTON' && el.textContent.startsWith('Advanced'),
  )[0];
  await fire(advanced, 'onClick', {});

  /** The row whose label reads `label`. */
  const row = (label: string) => {
    const name = findAll(
      container,
      (el) => el.getAttribute('class') === 'st-row-label' && el.textContent === label,
    )[0];
    if (!name) throw new Error(`no ${label} row`);
    return name.parentNode?.parentNode as FakeElement;
  };
  const within = (label: string, tag: string) => findAll(row(label), (el) => el.nodeName === tag);
  const select = () => within('Sound', 'SELECT')[0];
  const play = () => within('Sound', 'BUTTON')[0];
  const segments = (label: string) => within(label, 'BUTTON');
  const toggle = () => within('Only while you are not looking at its session', 'INPUT')[0];
  return {
    /** The trigger as the card last wrote it. */
    value: () => current,
    tone: () => props(select()).value,
    tones: () => findAll(select(), (el) => el.nodeName === 'OPTION').map((o) => o.textContent),
    playLabel: () => play().getAttribute('aria-label'),
    /** The pressed segment of the row named `label`. */
    pressed: (label: string) =>
      segments(label).find((b) => b.getAttribute('aria-pressed') === 'true')?.textContent,
    background: () => props(toggle()).checked,
    /** Whether Sound in the Alert row reads as pressed. */
    soundOn: () =>
      findAll(
        container,
        (el) =>
          el.nodeName === 'BUTTON' &&
          el.textContent === 'Sound' &&
          el.getAttribute('aria-pressed') === 'true',
      ).length === 1,
    /** Every control of the four rows, by whether it is disabled. */
    disabled: () =>
      [play(), select(), ...segments('Bounce'), ...segments('Banner shows'), toggle()].map((el) =>
        el.hasAttribute('disabled'),
      ),
    pick: (tone: string) => fire(select(), 'onChange', { target: { value: tone } }),
    press: (label: string, segment: string) =>
      fire(
        segments(label).find((b) => b.textContent === segment),
        'onClick',
        {},
      ),
    turn: (on: boolean) => fire(toggle(), 'onChange', { target: { checked: on } }),
    play: () => fire(play(), 'onClick', {}),
  };
}

describe('the alert rows under Advanced', () => {
  it('shows Chime, Once, Title only and the switch on for a trigger with no alert', async () => {
    const card = await mount(VISITOR);
    expect(card.tone()).toBe('chime');
    expect(card.tones()).toEqual(['Chime', 'Bell', 'Knock', 'Low']);
    expect(card.playLabel()).toBe('Play Chime');
    expect(card.pressed('Bounce')).toBe('Once');
    expect(card.pressed('Banner shows')).toBe('Title only');
    expect(card.background()).toBe(true);
  });

  it('turns Sound on when you pick Bell with Sound off', async () => {
    const card = await mount(VISITOR);
    await card.pick('bell');
    expect(card.value().alert).toEqual({
      banner: false,
      sound: 'bell',
      background: true,
      words: false,
    });
    expect(card.soundOn()).toBe(true);
    expect(card.playLabel()).toBe('Play Bell');
  });

  it('sets attention until when you press Until you return', async () => {
    const card = await mount(VISITOR);
    await card.press('Bounce', 'Until you return');
    expect(card.value().alert).toEqual({
      banner: false,
      attention: 'until',
      background: true,
      words: false,
    });
    expect(card.pressed('Bounce')).toBe('Until you return');
  });

  it('names the Bounce row for what it does on Windows and Linux', async () => {
    doc.documentElement.dataset.platform = 'windows';
    expect((await mount(VISITOR)).pressed('Flash')).toBe('Once');
    doc.documentElement.dataset.platform = 'linux';
    expect((await mount(VISITOR)).pressed('Mark')).toBe('Once');
  });

  it('sets words when you press Title and words', async () => {
    const card = await mount({
      ...VISITOR,
      alert: { banner: true, background: true, words: false },
    });
    await card.press('Banner shows', 'Title and words');
    expect(card.value().alert).toEqual({ banner: true, background: true, words: true });
    expect(card.pressed('Banner shows')).toBe('Title and words');
  });

  it('keeps the table with no part on while the switch is off, and drops it once back on', async () => {
    const card = await mount(VISITOR);
    await card.turn(false);
    expect(card.value().alert).toEqual({ banner: false, background: false, words: false });
    expect(card.background()).toBe(false);

    await card.turn(true);
    expect('alert' in card.value()).toBe(false);
  });

  it('plays the tone shown, whether Sound is on or not', async () => {
    const card = await mount(VISITOR);
    await card.play();
    expect(played).toEqual(['chime']);

    const knock = await mount({
      ...VISITOR,
      alert: { banner: false, sound: 'knock', background: true, words: false },
    });
    expect(knock.playLabel()).toBe('Play Knock');
    await knock.play();
    expect(played).toEqual(['chime', 'knock']);
  });

  it('shows a stored tone Vosh does not know as its own option', async () => {
    const card = await mount({
      ...VISITOR,
      alert: { banner: false, sound: 'harp', background: true, words: false },
    });
    expect(card.tone()).toBe('harp');
    expect(card.tones()).toEqual(['Chime', 'Bell', 'Knock', 'Low', 'harp']);
    expect(card.playLabel()).toBe('Play harp');
  });

  it('shows every row of a preset trigger disabled', async () => {
    const card = await mount({
      ...VISITOR,
      preset: 'room_colors',
      alert: { banner: true, sound: 'bell', attention: 'once', background: true, words: true },
    });
    expect(card.disabled()).toEqual(Array(7).fill(true));
    expect(card.tone()).toBe('bell');
  });
});
