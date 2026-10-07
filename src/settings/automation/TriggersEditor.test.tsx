import { act, useState } from 'react';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { TriggerRecord } from '../../ipc/automation';
import { FakeDocument, FakeElement, findAll } from '../../test/fakeDom';

// The trigger card's Pattern row, board 6 of the Scripts review: the
// mode beside the label, the line under it that says what the mode does,
// and the field under both. Then its Alert row, board 1 of the Alerts
// review. This mounts the card on one trigger, or the whole editor over
// a fake store, and drives it through the handlers React keeps on each
// element, since this DOM sends no events.

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => undefined)),
  emit: vi.fn(() => Promise.resolve()),
}));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((cmd: string, args?: { json?: string }) => Promise.resolve(answer(cmd, args))),
}));

// CodeMirror needs a real DOM, and Advanced holds the Lua script row.
vi.mock('../../ui/CodeEditor', () => ({ CodeEditor: () => null }));
vi.mock('../../stores/session/promptGagStore', () => ({
  usePromptGags: () => new Set<string>(),
}));

/** The trigger list triggers_export answers with and triggers_import
 *  writes, as JSON. */
let stored = '[]';

function answer(cmd: string, args?: { json?: string }): unknown {
  if (cmd === 'triggers_export') return stored;
  if (cmd === 'triggers_import') stored = args?.json ?? stored;
  if (cmd === 'groups_list') return [];
  return undefined;
}

const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;
let TriggerDetail: typeof import('./TriggersEditor').TriggerDetail;
let TriggersEditor: typeof import('./TriggersEditor').TriggersEditor;

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
  vi.stubGlobal('localStorage', {
    getItem: () => null,
    setItem: () => undefined,
    removeItem: () => undefined,
  });
  // A link scrolls the row it opens into view, which the fake page has
  // no layout for.
  vi.stubGlobal('CSS', { escape: (s: string) => s });
  Object.assign(FakeElement.prototype, { querySelector: () => null });
  ({ createRoot } = await import('react-dom/client'));
  ({ TriggerDetail, TriggersEditor } = await import('./TriggersEditor'));
});

type Handler = (e?: unknown) => void;

/** The handlers React keeps on an element. */
function on(el: FakeElement): Record<string, Handler> {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$'));
  if (!key) throw new Error('the element has no React props');
  return (el as unknown as Record<string, Record<string, Handler>>)[key];
}

const cleanups: (() => Promise<void>)[] = [];
afterEach(async () => {
  for (const clean of cleanups.splice(0)) await clean();
  delete doc.documentElement.dataset.platform;
});

/** A trigger with one pattern row, as the store sends it. */
function trigger(patch: Partial<TriggerRecord>): TriggerRecord {
  return { name: 'fog', patterns: [], priority: 5, enabled: true, actions: [], ...patch };
}

/** An alert table with nothing on, as Rust reads an empty one. */
const QUIET = { banner: false, background: true, words: false };

const FOG = trigger({
  patterns: [{ pattern: '', enabled: true, mode: 'text', text: 'A thick fog rolls in' }],
});

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
  const byId = (id: string | null) => findAll(container, (el) => el.getAttribute('id') === id)[0];
  const modes = () =>
    findAll(
      findAll(container, (el) => el.getAttribute('aria-label') === 'Match the pattern as')[0],
      (el) => el.nodeName === 'BUTTON',
    );
  const field = () =>
    findAll(container, (el) => el.nodeName === 'INPUT' && el.hasAttribute('aria-labelledby'))[0];
  const button = (text: string) =>
    findAll(container, (el) => el.nodeName === 'BUTTON' && el.textContent.startsWith(text))[0];
  return {
    /** The trigger as the card last wrote it. */
    value: () => current,
    parts: () => alertParts(container),
    modes,
    pressed: () => modes().find((b) => b.getAttribute('aria-pressed') === 'true')?.textContent,
    field,
    /** The label and the line that name and describe the field. */
    label: () => byId(field().getAttribute('aria-labelledby'))?.textContent,
    description: () => byId(field().getAttribute('aria-describedby'))?.textContent,
    click: (text: string) =>
      act(async () => {
        const el = button(text);
        if (!el) throw new Error(`no ${text} button`);
        on(el).onClick({ currentTarget: el, preventDefault() {} });
      }),
  };
}

describe('the trigger Pattern row', () => {
  it('says what the mode does and sets it on every pattern', async () => {
    const card = await mount({
      ...FOG,
      patterns: [...FOG.patterns, { pattern: 'You feel better\\.$', enabled: false }],
    });
    expect(card.modes().map((b) => b.textContent)).toEqual(['Text', 'Starts with', 'Regex']);
    expect(card.pressed()).toBe('Text');
    expect(card.label()).toBe('Pattern');
    expect(card.description()).toBe('Matches a line that is exactly this text.');
    expect(card.field().value).toBe('A thick fog rolls in');

    await card.click('Starts with');
    expect(card.pressed()).toBe('Starts with');
    expect(card.description()).toBe('Matches any line that starts with this text.');
    expect(card.value().patterns.map((p) => p.mode)).toEqual(['starts_with', 'starts_with']);
    expect(card.field().value).toBe('A thick fog rolls in');

    await card.click('Regex');
    expect(card.description()).toBe('A regular expression. Groups fill $1 and on.');
    expect(card.value().patterns).toEqual([
      { pattern: 'A thick fog rolls in', enabled: true },
      { pattern: 'You feel better\\.$', enabled: false },
    ]);
  });

  it('shows a row with no mode as Regex', async () => {
    const card = await mount(
      trigger({ patterns: [{ pattern: '^You are hungry\\.$', enabled: true }] }),
    );
    expect(card.pressed()).toBe('Regex');
    expect(card.description()).toBe('A regular expression. Groups fill $1 and on.');
  });

  it('locks the mode and the pattern of a preset trigger', async () => {
    const card = await mount({ ...FOG, preset: 'room_colors' });
    expect(card.modes().map((b) => b.hasAttribute('disabled'))).toEqual([true, true, true]);
    expect(card.field().hasAttribute('disabled')).toBe(true);
  });

  it('adds a pattern in the mode the trigger reads', async () => {
    const card = await mount(FOG);
    await card.click('Advanced');
    await card.click('Add pattern');
    expect(card.value().patterns[1]).toEqual({
      pattern: '',
      enabled: true,
      mode: 'text',
      text: '',
    });
    await card.click('Regex');
    await card.click('Add pattern');
    expect(card.value().patterns[2]).toEqual({ pattern: '', enabled: true });
  });
});

/** The visitor of board 1, with no alert yet. */
const VISITOR = trigger({
  name: 'visitor',
  patterns: [{ pattern: '^(\\w+) walks in\\.$', enabled: true }],
});

/** Each part of the Alert row by its label, with `+` before a pressed
 *  one and ` (off)` after a disabled one. A part whose check does not
 *  match whether it is pressed fails the test. */
function alertParts(root: FakeElement) {
  const group = findAll(root, (el) => el.getAttribute('aria-label') === 'Alert with')[0];
  if (!group) throw new Error('no Alert row');
  return findAll(group, (el) => el.nodeName === 'BUTTON').map((b) => {
    const pressed = b.getAttribute('aria-pressed') === 'true';
    const check = b.childNodes[0] instanceof FakeElement && b.childNodes[0].nodeName === 'SVG';
    if (pressed !== check)
      throw new Error(`${b.textContent} is pressed ${pressed}, check ${check}`);
    return `${pressed ? '+' : ''}${b.textContent}${b.hasAttribute('disabled') ? ' (off)' : ''}`;
  });
}

/** The whole Triggers editor over the store, with nothing selected. */
async function mountEditor(
  list: TriggerRecord[],
  open: { select?: string; filter?: string; seq: number } | null = null,
) {
  stored = JSON.stringify(list);
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  await act(async () => {
    root.render(
      <TriggersEditor
        json={false}
        onJson={() => {}}
        onDirty={() => {}}
        onError={() => {}}
        open={open}
      />,
    );
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
  cleanups.push(async () => {
    await act(async () => root.unmount());
    doc.body.removeChild(container);
  });
  const click = (el: FakeElement | undefined) =>
    act(async () => {
      if (!el) throw new Error('nothing to click');
      on(el).onClick({ currentTarget: el, preventDefault() {} });
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
  const button = (text: string) =>
    findAll(container, (el) => el.nodeName === 'BUTTON' && el.textContent === text)[0];
  return {
    pick: (name: string) =>
      click(
        findAll(
          container,
          (el) => el.hasAttribute('data-uid') && el.textContent.startsWith(name),
        )[0],
      ),
    click: (text: string) => click(button(text)),
    /** The names of the rows the list shows, `*` after the selected one. */
    rows: () =>
      findAll(container, (el) => el.hasAttribute('data-uid')).map(
        (el) =>
          findAll(el, (n) => (n.getAttribute('class') ?? '').includes('st-auto-row-name'))[0]
            ?.textContent + (el.getAttribute('aria-current') === 'true' ? '*' : ''),
      ),
    parts: () => alertParts(container),
    status: () =>
      findAll(container, (el) => el.getAttribute('class') === 'st-savebar-status')[0]?.textContent,
  };
}

describe('the trigger Alert row', () => {
  it('presses Banner on, marks Unsaved changes, and saves the alert', async () => {
    const editor = await mountEditor([VISITOR]);
    await editor.pick('visitor');
    expect(editor.parts()).toEqual(['Banner', 'Sound', 'Bounce']);
    expect(editor.status()).toBe('');

    await editor.click('Banner');
    expect(editor.parts()).toEqual(['+Banner', 'Sound', 'Bounce']);
    expect(editor.status()).toBe('Unsaved changes');

    await editor.click('Save');
    const [saved] = JSON.parse(stored) as TriggerRecord[];
    expect(saved.alert).toEqual({ banner: true, background: true, words: false });
  });

  it('presses each part on its own, and releasing every one removes the table', async () => {
    const card = await mount(VISITOR);
    await card.click('Sound');
    await card.click('Bounce');
    expect(card.value().alert).toEqual({
      banner: false,
      sound: 'chime',
      attention: 'once',
      background: true,
      words: false,
    });
    await card.click('Banner');
    expect(card.value().alert?.banner).toBe(true);

    await card.click('Banner');
    await card.click('Sound');
    await card.click('Bounce');
    expect('alert' in card.value()).toBe(false);
  });

  it('keeps the table after its last part is released while it shows the words', async () => {
    const card = await mount({
      ...VISITOR,
      alert: { banner: true, sound: 'bell', attention: 'until', background: true, words: true },
    });
    await card.click('Banner');
    await card.click('Sound');
    await card.click('Bounce');
    expect(card.value().alert).toEqual({ banner: false, background: true, words: true });
  });

  it('shows the row of a preset trigger turned off', async () => {
    const card = await mount({ ...FOG, preset: 'room_colors', alert: { ...QUIET, banner: true } });
    expect(card.parts()).toEqual(['+Banner (off)', 'Sound (off)', 'Bounce (off)']);
  });

  it('names Bounce for what it does on Windows and Linux', async () => {
    doc.documentElement.dataset.platform = 'windows';
    expect((await mount(VISITOR)).parts()).toEqual(['Banner', 'Sound', 'Flash']);
    doc.documentElement.dataset.platform = 'linux';
    expect((await mount(VISITOR)).parts()).toEqual(['Banner', 'Sound', 'Mark']);
    doc.documentElement.dataset.platform = 'macos';
    expect((await mount(VISITOR)).parts()).toEqual(['Banner', 'Sound', 'Bounce']);
  });
});

// Presets board 1: the links on a preset's card open Triggers on one of
// its triggers, or filtered by the preset's name.
describe('a link from a preset card', () => {
  const sanctuary = trigger({
    name: 'buff.sanctuary',
    preset: 'disarm_buff_fade',
    patterns: [{ pattern: '^The white aura around your body fades\\.$', enabled: true }],
  });

  it('filters by the preset name, which a preset trigger now answers to', async () => {
    const editor = await mountEditor([VISITOR, sanctuary], {
      filter: 'Disarms and fading buffs',
      seq: 1,
    });
    expect(editor.rows()).toEqual(['buff.sanctuary*']);
  });

  it('opens on the trigger it names', async () => {
    const editor = await mountEditor([VISITOR, sanctuary], { select: 'buff.sanctuary', seq: 1 });
    expect(editor.rows()).toEqual(['visitor', 'buff.sanctuary*']);
  });
});
