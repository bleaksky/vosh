import { act, useState } from 'react';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { TriggerRecord } from '../../ipc/automation';
import { FakeDocument, FakeElement, findAll } from '../../test/fakeDom';

// The four rows that close a trigger card under Advanced and tune its
// alert: Sound with its play button, Bounce, Banner shows and the
// switch. This mounts the card on one trigger, opens Advanced, and
// drives the rows through the handlers React keeps on each element,
// since this DOM sends no events. Then the Alert row on that card asks
// before the first banner and wears the warn ring while the system
// turns banners off.

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => undefined)),
  emit: vi.fn(() => Promise.resolve()),
}));
/** What alerts_permission and alerts_ask_permission answer, and every
 *  command the card sent. */
const system = vi.hoisted(() => ({
  permission: 'granted',
  answer: 'granted',
  sent: [] as string[],
}));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((cmd: string) => {
    system.sent.push(cmd);
    if (cmd === 'alerts_permission') return Promise.resolve(system.permission);
    if (cmd === 'alerts_ask_permission') return Promise.resolve(system.answer);
    return Promise.resolve();
  }),
}));
// The ask as its props draw it. Its focus trap needs a real DOM.
vi.mock('../../ui/ConfirmDialog', async () => {
  const { createElement: h } = await import('react');
  return {
    ConfirmDialog: (p: { title: string; confirmLabel: string; onConfirm: () => void }) =>
      h(
        'div',
        { className: 'ov-confirm' },
        h('h2', null, p.title),
        h('button', { type: 'button', onClick: p.onConfirm }, p.confirmLabel),
      ),
  };
});
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
  system.permission = 'granted';
  system.sent.splice(0);
  delete doc.documentElement.dataset.platform;
});

/** A trigger on a visitor walking in, with no alert yet. */
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
  await act(async () => {
    root.render(<Card />);
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
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
    /** Press a part of the Alert row, and let the system's answer land. */
    alert: (part: string) =>
      act(async () => {
        props(segments('Alert').find((b) => b.textContent === part) as FakeElement).onClick({});
        await new Promise((resolve) => setTimeout(resolve, 0));
      }),
    /** The Banner part, with its warn ring and its title. */
    banner: () => {
      const b = segments('Alert').find((el) => el.textContent === 'Banner') as FakeElement;
      return {
        warn: b.getAttribute('class') === 'st-seg-item is-warn',
        title: b.getAttribute('title'),
      };
    },
    /** The ask's title, or undefined while it is closed. */
    ask: () =>
      findAll(container, (el) => el.getAttribute('class') === 'ov-confirm')[0]?.childNodes[0]
        ?.textContent,
    confirm: () =>
      act(async () => {
        const card = findAll(container, (el) => el.getAttribute('class') === 'ov-confirm')[0];
        props(findAll(card, (el) => el.nodeName === 'BUTTON')[0]).onClick({});
        await new Promise((resolve) => setTimeout(resolve, 0));
      }),
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

describe('the Alert row of a trigger', () => {
  it('asks before the first banner, and Continue asks the system and keeps its answer', async () => {
    system.permission = 'not_asked';
    system.answer = 'denied';
    const card = await mount(VISITOR);
    await card.alert('Banner');
    expect(card.ask()).toBe('Let Vosh post banners?');
    expect(card.value().alert).toBeUndefined();

    await card.confirm();
    expect(card.ask()).toBeUndefined();
    expect(system.sent).toContain('alerts_ask_permission');
    expect(card.value().alert?.banner).toBe(true);
    expect(card.banner()).toEqual({
      warn: true,
      title: 'Banners from Vosh are off in System Settings, so Banner shows nothing.',
    });
  });

  it('rings Banner while the system turns banners off, and names Windows Settings there', async () => {
    system.permission = 'denied';
    doc.documentElement.dataset.platform = 'windows';
    const card = await mount(VISITOR);
    expect(card.banner()).toEqual({
      warn: true,
      title: 'Banners from Vosh are off in Windows Settings, so Banner shows nothing.',
    });
    await card.alert('Banner');
    expect(card.ask()).toBeUndefined();
    expect(card.value().alert?.banner).toBe(true);
  });

  it('asks nothing and rings nothing while banners are allowed', async () => {
    const card = await mount(VISITOR);
    await card.alert('Banner');
    expect(card.ask()).toBeUndefined();
    expect(card.banner()).toEqual({ warn: false, title: null });
    expect(system.sent).not.toContain('alerts_ask_permission');
  });
});
