import { act, createElement } from 'react';
import type { Root } from 'react-dom/client';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { onAlert, type AlertPayload } from '../ipc/alerts';
import { playAlertTone } from '../stores/session/alertTones';
import { FakeDocument, FakeElement, FakeNode } from '../test/fakeDom';
import { useAlertTones } from './useAlertTones';

// The session's alerts reach the hook through a stand in for onAlert, and
// the tones it plays land in a spy.
let heard: ((alert: AlertPayload) => void) | null = null;
vi.mock('../ipc/alerts', () => ({
  onAlert: vi.fn((cb: (alert: AlertPayload) => void) => {
    heard = cb;
    return Promise.resolve(() => {});
  }),
}));
vi.mock('../stores/session/alertTones', () => ({ playAlertTone: vi.fn() }));

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

beforeEach(() => {
  heard = null;
  vi.mocked(playAlertTone).mockClear();
});

function Hearer() {
  useAlertTones();
  return null;
}

async function mount() {
  const root: Root = createRoot(doc.createElement('div') as unknown as HTMLElement);
  await act(async () => root.render(createElement(Hearer)));
  return () => act(async () => root.unmount());
}

/** A tell from Tolliver as the alert_tells preset rings it in session 2,
 *  one you are not looking at. */
function tell(sound: string | null): AlertPayload {
  return {
    session: 2,
    title: 'Tell from Tolliver',
    label: null,
    words: null,
    sound,
    banner: false,
    notice: true,
    source: 'preset:alert_tells',
    owner: null,
  };
}

describe('useAlertTones', () => {
  it('plays the tone an alert names, from any session', async () => {
    const unmount = await mount();
    expect(onAlert).toHaveBeenCalled();
    await act(async () => heard?.(tell('bell')));
    expect(playAlertTone).toHaveBeenCalledTimes(1);
    expect(playAlertTone).toHaveBeenCalledWith('bell');
    await unmount();
  });

  it('plays nothing when the alert has no tone or a system sound played it', async () => {
    const unmount = await mount();
    await act(async () => heard?.(tell(null)));
    await act(async () => heard?.(tell('')));
    expect(playAlertTone).not.toHaveBeenCalled();
    await unmount();
  });
});
