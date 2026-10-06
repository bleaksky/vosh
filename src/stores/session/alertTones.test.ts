import { afterEach, describe, expect, it, vi } from 'vitest';
import { ALERT_TONES, playAlertTone } from './alertTones';

/** A stand in for Web Audio that records each note it was asked for. */
function fakeAudio() {
  const made: FakeContext[] = [];
  class FakeParam {
    value = 0;
    /** Each point the envelope passes, as [seconds, level]. */
    points: [number, number][] = [];
    setValueAtTime = vi.fn((level: number, at: number) => this.points.push([at, level]));
    exponentialRampToValueAtTime = vi.fn((level: number, at: number) =>
      this.points.push([at, level]),
    );
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
    gains: FakeGain[] = [];
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
      const gain = new FakeGain();
      this.gains.push(gain);
      return gain;
    }
  }
  return { made, FakeContext };
}

/** How loud an envelope is at `t`, each ramp exponential as Web Audio
 *  draws it, silent before its first point. */
function level(points: [number, number][], t: number): number {
  if (t < points[0][0]) return 0;
  for (let i = 1; i < points.length; i++) {
    const [t0, v0] = points[i - 1];
    const [t1, v1] = points[i];
    if (t <= t1) return v0 * (v1 / v0) ** ((t - t0) / (t1 - t0));
  }
  return points[points.length - 1][1];
}

/** The notes `tone` builds, as frequency and wave for each. */
function notesOf(tone: string) {
  const { made, FakeContext } = fakeAudio();
  vi.stubGlobal('window', { AudioContext: FakeContext });
  playAlertTone(tone);
  return made[0].oscillators.map((osc) => [osc.frequency.value, osc.type]);
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('playAlertTone', () => {
  it('names the four tones in the order Alerts Q4 gives them', () => {
    expect(ALERT_TONES.map((tone) => [tone.value, tone.label])).toEqual([
      ['chime', 'Chime'],
      ['bell', 'Bell'],
      ['knock', 'Knock'],
      ['low', 'Low'],
    ]);
  });

  it.each(ALERT_TONES.map((tone) => tone.value))(
    'starts and stops each note of %s and lets its audio go once the last ends',
    (tone) => {
      const { made, FakeContext } = fakeAudio();
      vi.stubGlobal('window', { AudioContext: FakeContext });
      playAlertTone(tone);
      expect(made).toHaveLength(1);
      const ctx = made[0];
      expect(ctx.oscillators.length).toBeGreaterThan(0);
      for (const osc of ctx.oscillators) {
        expect(osc.connect).toHaveBeenCalled();
        expect(osc.start).toHaveBeenCalledTimes(1);
        expect(osc.stop).toHaveBeenCalledTimes(1);
        expect(osc.stop.mock.calls[0][0]).toBeGreaterThan(osc.start.mock.calls[0][0]);
        expect(osc.start.mock.calls[0][0]).toBeGreaterThanOrEqual(5);
      }
      for (const gain of ctx.gains) expect(gain.connect).toHaveBeenCalledWith(ctx.destination);
      const ends = [...ctx.oscillators].sort(
        (a, b) => a.stop.mock.calls[0][0] - b.stop.mock.calls[0][0],
      );
      for (const osc of ends.slice(0, -1)) osc.onended?.();
      expect(ctx.close).not.toHaveBeenCalled();
      ends[ends.length - 1].onended?.();
      expect(ctx.close).toHaveBeenCalledTimes(1);
    },
  );

  it.each(ALERT_TONES.map((tone) => tone.value))(
    'rings %s no louder than the tick, which peaks at 0.18',
    (tone) => {
      const { made, FakeContext } = fakeAudio();
      vi.stubGlobal('window', { AudioContext: FakeContext });
      playAlertTone(tone);
      const envelopes = made[0].gains.map((gain) => gain.gain.points);
      let loudest = 0;
      for (let t = 5; t < 7; t += 0.0005) {
        loudest = Math.max(
          loudest,
          envelopes.reduce((sum, points) => sum + level(points, t), 0),
        );
      }
      expect(loudest).toBeGreaterThan(0.1);
      expect(loudest).toBeLessThanOrEqual(0.18 + 1e-9);
    },
  );

  it('draws each tone apart from the others', () => {
    const drawn = ALERT_TONES.map((tone) => JSON.stringify(notesOf(tone.value)));
    expect(new Set(drawn).size).toBe(ALERT_TONES.length);
  });

  it('plays Chime for a name it does not know, as the system sound falls back to Glass', () => {
    const chime = notesOf('chime');
    expect(notesOf('siren')).toEqual(chime);
    expect(notesOf('')).toEqual(chime);
    expect(notesOf('constructor')).toEqual(chime);
  });

  it('stays quiet where the web view has no audio', () => {
    vi.stubGlobal('window', {});
    expect(() => playAlertTone('bell')).not.toThrow();
  });
});
