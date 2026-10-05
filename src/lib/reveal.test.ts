import { describe, expect, it, vi } from 'vitest';
import mainSource from '../shell/useUiConfigFollow.ts?raw';
import settingsSource from '../settings/SettingsWindow.tsx?raw';
import { REPAINT_WAIT_MS, showAfterThemePaint, type RevealDeps } from './reveal';

vi.mock('../theme/theme', () => ({ paintMatchesBoot: () => false }));

function fakeDeps(painted: boolean) {
  const frames: Array<() => void> = [];
  const timers: Array<{ fn: () => void; ms: number; live: boolean }> = [];
  const deps: RevealDeps = {
    painted: () => painted,
    frame: (fn) => {
      frames.push(fn);
    },
    wait: (fn, ms) => {
      const timer = { fn, ms, live: true };
      timers.push(timer);
      return () => {
        timer.live = false;
      };
    },
  };
  const runFrame = () => frames.shift()?.();
  const runTimers = () => {
    for (const t of timers.splice(0)) if (t.live) t.fn();
  };
  return { deps, frames, timers, runFrame, runTimers };
}

describe('showAfterThemePaint', () => {
  it('shows at once when the startup paint already holds the theme', () => {
    const { deps, frames, timers } = fakeDeps(true);
    const show = vi.fn();
    showAfterThemePaint(show, deps);
    expect(show).toHaveBeenCalledTimes(1);
    expect(frames).toHaveLength(0);
    expect(timers).toHaveLength(0);
  });

  it('waits for a frame with the new theme to go out', () => {
    const { deps, runFrame, runTimers } = fakeDeps(false);
    const show = vi.fn();
    showAfterThemePaint(show, deps);
    expect(show).not.toHaveBeenCalled();
    // The first frame paints the theme. The second runs after it.
    runFrame();
    expect(show).not.toHaveBeenCalled();
    runFrame();
    expect(show).toHaveBeenCalledTimes(1);
    // The backstop timer was cancelled.
    runTimers();
    expect(show).toHaveBeenCalledTimes(1);
  });

  it('shows after a short wait when a hidden window runs no frames', () => {
    const { deps, timers, runFrame, runTimers } = fakeDeps(false);
    const show = vi.fn();
    showAfterThemePaint(show, deps);
    expect(timers.map((t) => t.ms)).toEqual([REPAINT_WAIT_MS]);
    runTimers();
    expect(show).toHaveBeenCalledTimes(1);
    // Frames that run late never show it twice.
    runFrame();
    runFrame();
    expect(show).toHaveBeenCalledTimes(1);
  });

  it('keeps the wait short', () => {
    expect(REPAINT_WAIT_MS).toBeLessThanOrEqual(150);
  });
});

describe('the windows', () => {
  it('show themselves only after a paint in the theme', () => {
    for (const source of [mainSource, settingsSource]) {
      expect(source).toContain('.finally(() => showAfterThemePaint(reveal))');
      expect(source).not.toMatch(/\.finally\(reveal\)/);
    }
  });
});
