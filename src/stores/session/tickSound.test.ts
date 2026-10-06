import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// The sessions store hears the list on a fake Tauri event bus, so a test
// can select a session.
type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();

vi.mock('@tauri-apps/api/event', () => ({
  listen: async (event: string, cb: Handler) => {
    let set = handlers.get(event);
    if (!set) handlers.set(event, (set = new Set()));
    set.add(cb);
    return () => set.delete(cb);
  },
  emit: async () => undefined,
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: async () => [] }));

const TOLLIVER = 1;
const ORLA = 2;

/** The list the app sends, with Orla's session selected. */
function selectOrla(): void {
  const rows = [TOLLIVER, ORLA].map((id) => ({
    id,
    name: null,
    character: id === TOLLIVER ? 'Tolliver' : 'Orla',
    host: 'play.theforsakenlands.com',
    port: 1848,
    tls: false,
    profile: id === TOLLIVER ? 'Tolliver' : 'Orla',
    connected: true,
    selected: id === ORLA,
  }));
  for (const cb of handlers.get('vosh://sessions-changed') ?? []) cb({ payload: rows });
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

/** A stand in for Web Audio that records the tone it was asked for. */
function fakeAudio() {
  const made: FakeContext[] = [];
  class FakeParam {
    value = 0;
    setValueAtTime = vi.fn();
    exponentialRampToValueAtTime = vi.fn();
  }
  class FakeNode {
    connect = vi.fn();
  }
  class FakeOscillator extends FakeNode {
    frequency = new FakeParam();
    type = 'square';
    onended: (() => void) | null = null;
    start = vi.fn();
    stop = vi.fn();
  }
  class FakeGain extends FakeNode {
    gain = new FakeParam();
  }
  class FakeContext {
    currentTime = 5;
    destination = {};
    oscillators: FakeOscillator[] = [];
    close = vi.fn(() => Promise.resolve());
    constructor() {
      made.push(this);
    }
    createOscillator() {
      const osc = new FakeOscillator();
      this.oscillators.push(osc);
      return osc;
    }
    createGain() {
      return new FakeGain();
    }
  }
  return { made, FakeContext };
}

beforeEach(() => {
  vi.resetModules();
  handlers.clear();
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe('playTickSound', () => {
  it('plays one short tone and lets its audio go when it ends', async () => {
    const { made, FakeContext } = fakeAudio();
    vi.stubGlobal('window', { AudioContext: FakeContext });
    const { playTickSound } = await import('./tickSound');
    playTickSound(TOLLIVER);
    expect(made).toHaveLength(1);
    const osc = made[0].oscillators[0];
    expect(osc.frequency.value).toBe(880);
    expect(osc.type).toBe('sine');
    expect(osc.start).toHaveBeenCalledWith(5);
    expect(osc.stop).toHaveBeenCalledWith(5.2);
    osc.onended?.();
    expect(made[0].close).toHaveBeenCalled();
  });

  it('plays once for reports that land together', async () => {
    vi.useFakeTimers();
    const { made, FakeContext } = fakeAudio();
    vi.stubGlobal('window', { AudioContext: FakeContext });
    const { playTickSound } = await import('./tickSound');
    playTickSound(TOLLIVER);
    playTickSound(TOLLIVER);
    expect(made).toHaveLength(1);
    vi.advanceTimersByTime(600);
    playTickSound(TOLLIVER);
    expect(made).toHaveLength(2);
  });

  it('stays quiet where the web view has no audio', async () => {
    vi.stubGlobal('window', {});
    const { playTickSound } = await import('./tickSound');
    expect(() => playTickSound(TOLLIVER)).not.toThrow();
  });

  it('plays nothing for a tick in a session behind', async () => {
    const { made, FakeContext } = fakeAudio();
    vi.stubGlobal('window', { AudioContext: FakeContext });
    const { playTickSound } = await import('./tickSound');
    const sessions = await import('./sessionsStore');
    // Before the list comes, the first session is in front.
    playTickSound(ORLA);
    expect(made).toHaveLength(0);
    sessions.startSessionsStore();
    await settle();
    selectOrla();
    playTickSound(TOLLIVER);
    expect(made).toHaveLength(0);
    playTickSound(ORLA);
    expect(made).toHaveLength(1);
  });
});
