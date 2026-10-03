import { afterEach, describe, expect, it, vi } from 'vitest';
import { NATIVE_FAILED_KEY, nativeSurfaceEnabled, nativeUnderlay } from './Terminal';

// Which renderer draws the live terminal, as the page decides it. The
// xterm copy draws whenever the native underlay does not, so these hold
// the cases where xterm has to show what the session wrote.

function storage(values: Record<string, string>) {
  return {
    getItem: (key: string) => values[key] ?? null,
    setItem: (key: string, value: string) => {
      values[key] = value;
    },
    removeItem: (key: string) => {
      delete values[key];
    },
  };
}

function platform(mac: boolean, local: Record<string, string> = {}, session = {}) {
  vi.stubGlobal('navigator', {
    platform: mac ? 'MacIntel' : 'Win32',
    userAgent: mac
      ? 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)'
      : 'Mozilla/5.0 (Windows NT 10.0)',
  });
  vi.stubGlobal('localStorage', storage(local));
  vi.stubGlobal('sessionStorage', storage(session));
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('the renderer that draws the live terminal', () => {
  it('is the native underlay on macOS by default', () => {
    platform(true);
    expect(nativeSurfaceEnabled()).toBe(true);
    expect(nativeUnderlay()).toBe(true);
  });

  it('is xterm on macOS once the native surface failed to come up', () => {
    platform(true, {}, { [NATIVE_FAILED_KEY]: '1' });
    expect(nativeSurfaceEnabled()).toBe(false);
    expect(nativeUnderlay()).toBe(false);
  });

  it('is xterm on macOS when you turned the native surface off', () => {
    platform(true, { 'vosh.nativesurface': '0' });
    expect(nativeUnderlay()).toBe(false);
  });

  it('is xterm on Windows and Linux, even with the native surface forced on', () => {
    platform(false);
    expect(nativeSurfaceEnabled()).toBe(false);
    platform(false, { 'vosh.nativesurface': '1' });
    expect(nativeSurfaceEnabled()).toBe(false);
    expect(nativeUnderlay()).toBe(false);
  });
});
