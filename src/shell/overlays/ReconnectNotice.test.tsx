import { act, createElement } from 'react';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Reconnect } from '../../stores/session/reconnectStore';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../../test/fakeDom';

// The reconnect notice of board 7 in the Alerts review, for Tolliver's
// session (3). Each state's words and buttons as the frames draw them,
// the countdown, and what each button reaches.

const redial = vi.hoisted(() => ({ now: { kind: 'none' } as Reconnect }));
const calls = vi.hoisted(() => [] as unknown[]);
const answer = vi.hoisted(() => ({ error: null as string | null }));

vi.mock('../../stores/session/reconnectStore', () => ({ useReconnect: () => redial.now }));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd: string, args: unknown) => {
    calls.push([cmd, args]);
    return answer.error ? Promise.reject(answer.error) : Promise.resolve();
  },
}));

const TOLLIVER = 3;
const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;
let unmount: (() => Promise<void>) | null = null;

beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('document', doc);
  vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
  vi.stubGlobal('Node', FakeNode);
  vi.stubGlobal('Element', FakeElement);
  vi.stubGlobal('HTMLElement', FakeElement);
  ({ createRoot } = await import('react-dom/client'));
});

beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal('window', {
    document: doc,
    location: { protocol: 'about:' },
    HTMLIFrameElement: class {},
    addEventListener() {},
    removeEventListener() {},
    setInterval: (fn: () => void, ms: number) => setInterval(fn, ms),
    clearInterval: (id: ReturnType<typeof setInterval>) => clearInterval(id),
  });
  calls.length = 0;
  answer.error = null;
});

afterEach(async () => {
  await unmount?.();
  unmount = null;
  vi.useRealTimers();
});

afterAll(() => {
  vi.unstubAllGlobals();
});

const onTryAgain = vi.fn();
const onError = vi.fn();

async function mount(state: Reconnect) {
  redial.now = state;
  const { ReconnectNotice } = await import('./ReconnectNotice');
  const container = doc.createElement('div');
  const root = createRoot(container as unknown as HTMLElement);
  const render = () =>
    root.render(createElement(ReconnectNotice, { session: TOLLIVER, onTryAgain, onError }));
  await act(async () => render());
  unmount = () => act(async () => root.unmount());
  const card = () => findAll(container, (el) => el.nodeName === 'DIV')[0];
  return {
    card,
    text: (cls: string) =>
      findAll(container, (el) => el.getAttribute('class') === cls)[0]?.textContent,
    buttons: () =>
      findAll(container, (el) => el.nodeName === 'BUTTON').map((b) => [
        b.textContent,
        b.getAttribute('class'),
      ]),
    press: (label: string) => {
      const button = findAll(
        container,
        (el) => el.nodeName === 'BUTTON' && el.textContent === label,
      )[0];
      const key = Object.keys(button).find((k) => k.startsWith('__reactProps$')) as string;
      const props = (button as unknown as Record<string, { onClick: () => void }>)[key];
      return act(async () => props.onClick());
    },
    tick: (ms: number) => act(async () => void vi.advanceTimersByTime(ms)),
  };
}

const waiting = (n: number, seconds: number): Reconnect => ({
  kind: 'waiting',
  try: n,
  tries: 8,
  until: Date.now() + seconds * 1000,
});

describe('the reconnect notice', () => {
  it('shows nothing while no redial runs', async () => {
    const notice = await mount({ kind: 'none' });
    expect(notice.card()).toBeUndefined();
  });

  it('counts down while a try waits, with Cancel and Reconnect now', async () => {
    const notice = await mount(waiting(1, 3));
    expect(notice.card().getAttribute('class')).toBe('ov-update is-error');
    expect(notice.text('ov-update-dot dot is-danger')).toBe('');
    expect(notice.text('ov-update-msg')).toBe('Reconnecting in 3s');
    expect(notice.text('ov-update-meta')).toBe('Try 1 of 8');
    expect(notice.buttons()).toEqual([
      ['Cancel', 'btn'],
      ['Reconnect now', 'btn is-primary'],
    ]);
    await notice.tick(1000);
    expect(notice.text('ov-update-msg')).toBe('Reconnecting in 2s');
    await notice.tick(5000);
    expect(notice.text('ov-update-msg')).toBe('Reconnecting in 1s');
  });

  it('reads a longer wait in whole seconds', async () => {
    const notice = await mount(waiting(4, 24));
    expect(notice.text('ov-update-msg')).toBe('Reconnecting in 24s');
    expect(notice.text('ov-update-meta')).toBe('Try 4 of 8');
  });

  it('rings in the success tone while a try dials, with Cancel only', async () => {
    const notice = await mount({ kind: 'dialing', try: 2, tries: 8 });
    expect(notice.card().getAttribute('class')).toBe('ov-update is-wait');
    expect(notice.text('ov-update-dot dot is-off is-success')).toBe('');
    expect(notice.text('ov-update-msg')).toBe('Connecting');
    expect(notice.text('ov-update-meta')).toBe('Try 2 of 8');
    expect(notice.buttons()).toEqual([['Cancel', 'btn']]);
  });

  it('says it stopped once the tries run out, and Try again dials', async () => {
    const notice = await mount({ kind: 'stopped', tries: 8 });
    expect(notice.card().getAttribute('class')).toBe('ov-update is-error');
    expect(notice.text('ov-update-dot dot is-danger')).toBe('');
    expect(notice.text('ov-update-msg')).toBe('Vosh stopped after 8 tries');
    expect(notice.text('ov-update-meta')).toBeUndefined();
    expect(notice.buttons()).toEqual([['Try again', 'btn is-primary']]);
    await notice.press('Try again');
    expect(onTryAgain).toHaveBeenCalledOnce();
    expect(calls).toEqual([]);
  });

  it('cancels and dials now for the session it shows', async () => {
    const notice = await mount(waiting(1, 3));
    await notice.press('Cancel');
    await notice.press('Reconnect now');
    expect(calls).toEqual([
      ['session_reconnect_cancel', { session: TOLLIVER }],
      ['session_reconnect_now', { session: TOLLIVER }],
    ]);
  });

  it('hands a failed press to the error line of the session', async () => {
    answer.error = 'no redial waits';
    const notice = await mount(waiting(1, 3));
    await notice.press('Reconnect now');
    expect(onError).toHaveBeenCalledWith('no redial waits', TOLLIVER);
  });
});
