import { act, useState } from 'react';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { TriggerRecord } from '../../ipc/automation';
import { FakeDocument, FakeElement, findAll } from '../../test/fakeDom';

// The trigger card's Pattern row, board 6: the mode beside the label,
// the line under it that says what the mode does, and the field under
// both. This mounts the card on one trigger and drives it through the
// handlers React keeps on each element, since this DOM sends no events.

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

/** The handlers React keeps on an element. */
function on(el: FakeElement): Record<string, Handler> {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$'));
  if (!key) throw new Error('the element has no React props');
  return (el as unknown as Record<string, Record<string, Handler>>)[key];
}

const cleanups: (() => Promise<void>)[] = [];
afterEach(async () => {
  for (const clean of cleanups.splice(0)) await clean();
});

/** A trigger with one pattern row, as the store sends it. */
function trigger(patch: Partial<TriggerRecord>): TriggerRecord {
  return { name: 'fog', patterns: [], priority: 5, enabled: true, actions: [], ...patch };
}

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
