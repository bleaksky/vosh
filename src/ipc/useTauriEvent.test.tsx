import type { UnlistenFn } from '@tauri-apps/api/event';
import { act, createElement } from 'react';
import type { Root } from 'react-dom/client';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import { FakeDocument, FakeElement, FakeNode } from '../test/fakeDom';
import { useTauriEvent } from './useTauriEvent';

// The hook mounted in a component that draws nothing, on a subscribe the
// test answers by hand: it keeps the callback it was given and resolves
// only when the test says.

const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;

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
  // React DOM checks for a DOM once, when it loads.
  ({ createRoot } = await import('react-dom/client'));
});

afterAll(() => {
  vi.unstubAllGlobals();
});

/** An event the test sends, with the subscriptions made to it. */
function fakeEvent() {
  const heard: ((payload: string) => void)[] = [];
  const settle: ((fn: UnlistenFn) => void)[] = [];
  const unlisten = vi.fn();
  const subscribe = vi.fn(
    (cb: (payload: string) => void) =>
      new Promise<UnlistenFn>((resolve) => {
        heard.push(cb);
        settle.push(resolve);
      }),
  );
  return {
    subscribe,
    unlisten,
    /** Put the listener in place, as Tauri does once listen resolves. */
    resolve: () => act(async () => settle.forEach((resolve) => resolve(unlisten))),
    /** Send a payload to every callback subscribe was given. */
    send: (payload: string) => act(async () => heard.forEach((cb) => cb(payload))),
  };
}

function Hearer(props: {
  subscribe: (cb: (payload: string) => void) => Promise<UnlistenFn>;
  handler: (payload: string) => void;
}) {
  useTauriEvent(props.subscribe, props.handler);
  return null;
}

async function mount(event: ReturnType<typeof fakeEvent>, handler: (payload: string) => void) {
  const root: Root = createRoot(doc.createElement('div') as unknown as HTMLElement);
  const render = (next: (payload: string) => void) =>
    act(async () =>
      root.render(createElement(Hearer, { subscribe: event.subscribe, handler: next })),
    );
  await render(handler);
  return { render, unmount: () => act(async () => root.unmount()) };
}

describe('useTauriEvent', () => {
  it('subscribes once, at mount, however often the component renders', async () => {
    const event = fakeEvent();
    const shown = await mount(event, () => {});
    await event.resolve();
    await shown.render(() => {});
    await shown.render(() => {});
    expect(event.subscribe).toHaveBeenCalledTimes(1);
    await shown.unmount();
  });

  it('hands each payload to the handler of the newest render', async () => {
    const event = fakeEvent();
    const first = vi.fn();
    const newest = vi.fn();
    const shown = await mount(event, first);
    await event.resolve();
    await shown.render(newest);
    await event.send('obsidian-ember');
    expect(first).not.toHaveBeenCalled();
    expect(newest).toHaveBeenCalledWith('obsidian-ember');
    await shown.unmount();
  });

  it('unlistens at unmount, and hands on nothing after', async () => {
    const event = fakeEvent();
    const handler = vi.fn();
    const shown = await mount(event, handler);
    await event.resolve();
    expect(event.unlisten).not.toHaveBeenCalled();
    await shown.unmount();
    expect(event.unlisten).toHaveBeenCalledTimes(1);
    await event.send('rubric');
    expect(handler).not.toHaveBeenCalled();
  });

  it('unlistens a subscription that resolves after unmount', async () => {
    const event = fakeEvent();
    const handler = vi.fn();
    const shown = await mount(event, handler);
    await shown.unmount();
    expect(event.unlisten).not.toHaveBeenCalled();
    await event.send('triad');
    await event.resolve();
    expect(event.unlisten).toHaveBeenCalledTimes(1);
    expect(handler).not.toHaveBeenCalled();
  });
});
