import { act, useState } from 'react';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { FakeDocument, FakeElement, findAll } from '../test/fakeDom';

// The field that also offers choices: ArrowDown opens the list, the
// arrows light a choice, Enter takes it, typing narrows the list, and
// Esc closes the list before it reaches the field's own keys. This
// renders into the fake DOM and drives the handlers React keeps on each
// element, since this DOM sends no events. The list draws into the body
// at the field's box, which the fake DOM gives every element.

const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;
let ComboField: typeof import('./ComboField').ComboField;

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
  Object.assign(doc, {
    getElementById: (id: string) => findAll(doc.body, (el) => el.getAttribute('id') === id)[0],
  });
  Object.assign(FakeElement.prototype, {
    getBoundingClientRect: () => ({ left: 10, top: 20, right: 250, bottom: 48, width: 240 }),
  });
  ({ createRoot } = await import('react-dom/client'));
  ({ ComboField } = await import('./ComboField'));
});

type Handler = (e?: unknown) => void;

function on(el: FakeElement): Record<string, Handler> {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$'));
  if (!key) throw new Error('the element has no React props');
  return (el as unknown as Record<string, Record<string, Handler>>)[key];
}

const cleanups: (() => Promise<void>)[] = [];
afterEach(async () => {
  for (const clean of cleanups.splice(0)) await clean();
});

const GROUPS = ['Maren', 'Orla', 'Tolliver'];

async function mount() {
  const picked: string[] = [];
  const fieldKeys: string[] = [];
  function Harness() {
    const [value, setValue] = useState('');
    return (
      <ComboField
        aria-label="Group"
        value={value}
        onChange={setValue}
        options={GROUPS}
        listLabel="Groups"
        onPick={(g) => {
          picked.push(g);
          setValue(g);
        }}
        onKeyDown={(e) => fieldKeys.push(e.key)}
      />
    );
  }
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  await act(async () => root.render(<Harness />));
  cleanups.push(async () => {
    await act(async () => root.unmount());
    doc.body.removeChild(container);
  });
  const input = () => findAll(container, (el) => el.nodeName === 'INPUT')[0];
  const stops: string[] = [];
  const key = (k: string) =>
    act(async () =>
      on(input()).onKeyDown({
        key: k,
        preventDefault() {},
        stopPropagation() {
          stops.push(k);
        },
      }),
    );
  return {
    picked,
    fieldKeys,
    stops,
    key,
    type: (text: string) => act(async () => on(input()).onChange({ target: { value: text } })),
    options: () =>
      findAll(doc.body, (el) => el.getAttribute('role') === 'option').map(
        (el) => (el.getAttribute('aria-selected') === 'true' ? '>' : '') + el.textContent,
      ),
    expanded: () => input().getAttribute('aria-expanded'),
  };
}

describe('ComboField', () => {
  it('opens on ArrowDown, lights a choice with the arrows, and takes it on Enter', async () => {
    const f = await mount();
    expect(f.options()).toEqual([]);
    await f.key('ArrowDown');
    expect(f.expanded()).toBe('true');
    expect(f.options()).toEqual(['>Maren', 'Orla', 'Tolliver']);
    await f.key('ArrowDown');
    await f.key('Enter');
    expect(f.picked).toEqual(['Orla']);
    expect(f.options()).toEqual([]);
    // Enter took the choice, so the field's own keys never saw it.
    expect(f.fieldKeys).toEqual([]);
  });

  it('narrows the list to the choices that hold your text', async () => {
    const f = await mount();
    await f.type('r');
    expect(f.options()).toEqual(['Maren', 'Orla', 'Tolliver']);
    await f.type('ol');
    expect(f.options()).toEqual(['Tolliver']);
    // Text that no choice holds closes the list, and Enter keeps it.
    await f.type('Fen');
    expect(f.expanded()).toBe('false');
    await f.key('Enter');
    expect(f.fieldKeys).toEqual(['Enter']);
  });

  it('closes the list on Esc before the field hears it', async () => {
    const f = await mount();
    await f.key('ArrowDown');
    await f.key('Escape');
    expect(f.options()).toEqual([]);
    expect(f.stops).toEqual(['Escape']);
    expect(f.fieldKeys).toEqual([]);
    await f.key('Escape');
    expect(f.fieldKeys).toEqual(['Escape']);
  });
});
