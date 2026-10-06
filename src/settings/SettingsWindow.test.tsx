import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SETTINGS_PENDING_KEY } from '../lib/settingsLink';
import { SettingsWindow } from './SettingsWindow';

// The frame's breadcrumb, drawn once on a cold open to the target the
// main window left. Effects do not run in a markup render, so nothing
// reaches the app. The crumb is the frame's, so the Scripts page under
// it draws nothing here.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve(null)) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));
vi.mock('./scripts/ScriptsPage', () => ({ ScriptsPage: () => null }));

const stored = new Map<string, string>();

beforeEach(() => {
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => stored.get(key) ?? null,
    setItem: (key: string, value: string) => void stored.set(key, value),
    removeItem: (key: string) => void stored.delete(key),
  });
  vi.stubGlobal('navigator', { userAgent: 'Macintosh', platform: 'MacIntel' });
});

afterEach(() => {
  stored.clear();
  vi.unstubAllGlobals();
});

/** The breadcrumb a cold open to `link` draws. */
function crumb(link: string): string {
  stored.set(SETTINGS_PENDING_KEY, link);
  const html = renderToStaticMarkup(<SettingsWindow />);
  return /<div class="st-crumb"[^>]*>.*?<\/h1>/.exec(html)?.[0] ?? '';
}

describe('the Settings breadcrumb', () => {
  it('titles a plugin page with its name and links back to Scripts', () => {
    const html = crumb('scripts:Vitals_Alert');
    expect(html).toMatch(/<a href="#scripts" class="st-crumb-link">Scripts<\/a>/);
    expect(html).toMatch(/<h1 class="st-crumb-title"[^>]*>Vitals_Alert<\/h1>/);
  });

  it('titles the Scripts list with the group alone', () => {
    const html = crumb('scripts');
    expect(html).not.toContain('st-crumb-link');
    expect(html).toMatch(/<h1 class="st-crumb-title"[^>]*>Scripts<\/h1>/);
  });
});
