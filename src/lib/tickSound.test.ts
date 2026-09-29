import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

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
    playTickSound();
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
    playTickSound();
    playTickSound();
    expect(made).toHaveLength(1);
    vi.advanceTimersByTime(600);
    playTickSound();
    expect(made).toHaveLength(2);
  });

  it('stays quiet where the web view has no audio', async () => {
    vi.stubGlobal('window', {});
    const { playTickSound } = await import('./tickSound');
    expect(() => playTickSound()).not.toThrow();
  });
});
