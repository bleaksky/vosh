import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ADD_PANE_MENU_EVENT } from '../../lib/appMenu';

// Show me on each step opens what the step is about and rings the
// right thing, with the shell's openers stood in for.

const spy = vi.hoisted(() => ({
  fold: vi.fn(),
  showCoach: vi.fn(),
  openSettingsTab: vi.fn(),
  profile: null as string | null,
}));

vi.mock('./getStartedStore', () => ({ fold: spy.fold }));
vi.mock('../../ui/coach', async (actual) => ({
  ...(await actual<typeof import('../../ui/coach')>()),
  showCoach: spy.showCoach,
}));
vi.mock('../../lib/settingsLink', () => ({ openSettingsTab: spy.openSettingsTab }));
vi.mock('../../stores/session/sessionsStore', async (actual) => ({
  ...(await actual<typeof import('../../stores/session/sessionsStore')>()),
  profileInFront: () => spy.profile,
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

const { showMe } = await import('./showMe');

let dispatched: string[];

beforeEach(() => {
  dispatched = [];
  vi.stubGlobal('window', { dispatchEvent: (e: Event) => dispatched.push(e.type) });
  vi.stubGlobal('requestAnimationFrame', (fn: () => void) => fn());
  for (const fn of [spy.fold, spy.showCoach, spy.openSettingsTab]) fn.mockClear();
  spy.profile = null;
});

afterEach(() => {
  vi.unstubAllGlobals();
});

function shell() {
  return { openPanel: vi.fn(), openTerminalMenu: vi.fn() };
}

/** The line the ring shows, or null when nothing rings. */
const ringLine = () =>
  (spy.showCoach.mock.calls.at(-1)?.[0] as { line: string } | undefined)?.line ?? null;

describe('Show me', () => {
  it('opens the panel and Add a pane on Add Chat and Group, and rings Chat and Group', () => {
    const s = shell();
    showMe('panes', s);
    expect(spy.fold).toHaveBeenCalledTimes(1);
    expect(s.openPanel).toHaveBeenCalledTimes(1);
    expect(s.openTerminalMenu).not.toHaveBeenCalled();
    expect(dispatched).toEqual([ADD_PANE_MENU_EVENT]);
    expect(ringLine()).toBe('Pick Chat or Group.');
  });

  it('opens the terminal menu on Customize your prompt, and rings Customize prompt…', () => {
    const s = shell();
    showMe('prompt', s);
    expect(spy.fold).toHaveBeenCalledTimes(1);
    expect(s.openTerminalMenu).toHaveBeenCalledTimes(1);
    expect(s.openPanel).not.toHaveBeenCalled();
    expect(dispatched).toEqual([]);
    expect(ringLine()).toBe('Pick Customize prompt…');
  });

  it('opens Add affect for the profile in front on the affects step', () => {
    spy.profile = 'Orla';
    showMe('affects', shell());
    expect(spy.openSettingsTab).toHaveBeenCalledWith('characters:Orla#add-affect');
    // The Settings window rings it, so the main window does not.
    expect(spy.showCoach).not.toHaveBeenCalled();
  });

  it('opens Add affect on Characters with no profile in front', () => {
    showMe('affects', shell());
    expect(spy.openSettingsTab).toHaveBeenCalledWith('characters#add-affect');
  });
});
