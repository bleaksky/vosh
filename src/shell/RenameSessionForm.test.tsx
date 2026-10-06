import { act, createElement } from 'react';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { SessionRow } from '../ipc/session';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../test/fakeDom';

// The Rename session form the session popover holds while no sidebar
// shows, as with one session. Name starts on what the session reads,
// Save keeps what you typed, a blank Name clears the name, and Cancel
// keeps nothing.

const store = vi.hoisted(() => ({
  rows: [] as SessionRow[],
  rename: vi.fn(() => Promise.resolve()),
}));

vi.mock('../stores/session/sessionsStore', () => ({
  getSelected: () => 1,
  rename: store.rename,
  useSessions: () => store.rows,
}));

const tolliver: SessionRow = {
  id: 1,
  name: null,
  character: 'Tolliver',
  host: 'play.theforsakenlands.com',
  port: 1848,
  tls: false,
  profile: 'Default',
  connected: true,
  since: null,
  selected: true,
};

type Handler = (e?: unknown) => void;

/** The handlers React keeps on an element. This DOM sends no events. */
function on(el: FakeElement): Record<string, Handler> {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$'));
  if (!key) throw new Error('the element has no React props');
  return (el as unknown as Record<string, Record<string, Handler>>)[key];
}

const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;
const cleanups: (() => Promise<void>)[] = [];

beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('document', doc);
  vi.stubGlobal('window', {
    document: doc,
    location: { protocol: 'about:' },
    HTMLIFrameElement: class {},
    addEventListener() {},
    removeEventListener() {},
  });
  vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
  vi.stubGlobal('Node', FakeNode);
  vi.stubGlobal('Element', FakeElement);
  vi.stubGlobal('HTMLElement', FakeElement);
  // The field selects its text as it opens.
  (FakeElement.prototype as unknown as Record<string, unknown>).select = () => undefined;
  // React DOM checks for a DOM once, when it loads.
  ({ createRoot } = await import('react-dom/client'));
});

afterEach(async () => {
  for (const cleanup of cleanups.splice(0)) await cleanup();
  store.rename.mockClear();
});

afterAll(() => {
  vi.unstubAllGlobals();
});

async function mount(rows: SessionRow[] = [tolliver]) {
  store.rows = rows;
  const { RenameSessionForm } = await import('./RenameSessionForm');
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  const onCancel = vi.fn();
  const onClose = vi.fn();
  await act(async () => {
    root.render(createElement(RenameSessionForm, { onCancel, onClose }));
  });
  cleanups.push(async () => {
    await act(async () => root.unmount());
    doc.body.removeChild(container);
  });
  const field = findAll(container, (el) => el.nodeName === 'INPUT')[0];
  const form = findAll(container, (el) => el.nodeName === 'FORM')[0];
  const button = (label: string) =>
    findAll(container, (el) => el.nodeName === 'BUTTON' && el.textContent === label)[0];
  const run = (fn: () => void) => act(async () => fn());
  return {
    container,
    field,
    button,
    onCancel,
    onClose,
    type: (text: string) => run(() => on(field).onChange({ target: { value: text } })),
    save: () => run(() => on(form).onSubmit({ preventDefault() {} })),
    cancel: () => run(() => on(button('Cancel')).onClick()),
  };
}

describe('the Rename session form', () => {
  it('holds one Name field on what the session reads, with Cancel and Save', async () => {
    const m = await mount();
    expect(findAll(m.container, (el) => el.nodeName === 'H2')[0]?.textContent).toBe(
      'Rename session',
    );
    expect(
      findAll(m.container, (el) => el.getAttribute('class') === 'shell-field-label').map(
        (el) => el.textContent,
      ),
    ).toEqual(['Name']);
    expect(m.field.value).toBe('Tolliver');
    expect(m.field.getAttribute('placeholder')).toBe('Tolliver');
    expect(doc.activeElement).toBe(m.field);
    expect(m.button('Cancel')).toBeDefined();
    expect(m.button('Save')?.getAttribute('type')).toBe('submit');
  });

  it('keeps the name you typed on Save', async () => {
    const m = await mount();
    await m.type('Builder');
    await m.save();
    expect(store.rename).toHaveBeenCalledWith(1, 'Builder');
    expect(m.onClose).toHaveBeenCalled();
  });

  it('clears the name with a blank field, so the session reads its character again', async () => {
    const m = await mount([{ ...tolliver, name: 'Builder' }]);
    expect(m.field.value).toBe('Builder');
    await m.type('');
    await m.save();
    expect(store.rename).toHaveBeenCalledWith(1, null);
  });

  it('changes nothing on Save when the name still reads the same', async () => {
    const m = await mount();
    await m.save();
    expect(store.rename).not.toHaveBeenCalled();
    expect(m.onClose).toHaveBeenCalled();
  });

  it('keeps nothing on Cancel', async () => {
    const m = await mount();
    await m.type('Builder');
    await m.cancel();
    expect(m.onCancel).toHaveBeenCalled();
    expect(store.rename).not.toHaveBeenCalled();
  });
});
