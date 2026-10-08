import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { SessionRow } from '../../ipc/session';

// Drives the alert notice through a fake Tauri event bus with Orla's
// session (1) selected and Tolliver's (2) behind it, to hold it to the
// corner notice of board 5 of the Sessions review (Q10). Each test
// loads fresh store modules.

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

vi.mock('@tauri-apps/api/core', () => ({
  invoke: async (cmd: string) => {
    if (cmd === 'sessions_list') return [];
    if (cmd === 'session_select') return null;
    throw new Error(`no fake for ${cmd}`);
  },
}));

function fire(event: string, payload: unknown): void {
  for (const cb of handlers.get(event) ?? []) cb({ payload });
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const ORLA = 1;
const TOLLIVER = 2;

/** An alert that rang in a session, as alert.rs sends it. */
const alert = (
  session: number,
  { notice = true, title = 'Tell from Maren', owner = null as string | null } = {},
) =>
  fire('session://alert', {
    session,
    title,
    label: session === TOLLIVER ? 'Tolliver' : 'Orla',
    words: null,
    sound: null,
    banner: !notice,
    notice,
    source: owner ? `lua:${owner}` : 'preset:alert_tells',
    owner,
  });

const ended = (session: number, owner: string) =>
  fire('session://alerts-ended', { session, owner });

/** A session's row as the app lists it. */
const row = (id: number, character: string): SessionRow => ({
  id,
  name: null,
  character,
  host: 'play.theforsakenlands.com',
  port: 1825,
  tls: false,
  profile: 'Default',
  connected: true,
  since: null,
  selected: id === ORLA,
});

async function load() {
  const sessions = await import('./sessionsStore');
  const notices = await import('./alertNoticeStore');
  sessions.startSessionsStore();
  notices.startAlertNoticeStore();
  await settle();
  fire('vosh://sessions-changed', [row(ORLA, 'Orla'), row(TOLLIVER, 'Tolliver')]);
  return { sessions, now: notices.getAlertNotice, show: notices.showAlertNotice };
}

beforeEach(() => {
  vi.resetModules();
  handlers.clear();
});

describe('the alert notice', () => {
  it('keeps an alert from a session behind that asks for a notice', async () => {
    const { now } = await load();
    alert(TOLLIVER);
    expect(now()).toEqual({
      session: TOLLIVER,
      title: 'Tell from Maren',
      label: 'Tolliver',
      owner: null,
    });
  });

  it('keeps nothing for an alert that posts a banner, or one from the session you look at', async () => {
    const { now } = await load();
    alert(TOLLIVER, { notice: false });
    alert(ORLA);
    expect(now()).toBeNull();
  });

  it('keeps the newest alert alone', async () => {
    const { now } = await load();
    alert(TOLLIVER);
    alert(TOLLIVER, { title: 'Maren attacked you' });
    expect(now()).toMatchObject({ session: TOLLIVER, title: 'Maren attacked you' });
  });

  it('clears once its session shows', async () => {
    const { sessions, now } = await load();
    alert(TOLLIVER);
    await sessions.select(TOLLIVER);
    expect(now()).toBeNull();
  });

  it('clears as Show selects its session', async () => {
    const { sessions, now, show } = await load();
    alert(TOLLIVER);
    show();
    expect(sessions.getSelected()).toBe(TOLLIVER);
    expect(now()).toBeNull();
  });

  it('clears once the Lua that raised it ends its alerts there', async () => {
    const { now } = await load();
    alert(TOLLIVER, { owner: 'plugin:watch' });
    ended(TOLLIVER, 'plugin:watch');
    expect(now()).toBeNull();
  });

  it('stays for the end of another owner, in another session, or of an alert with no owner', async () => {
    const { now } = await load();
    alert(TOLLIVER, { owner: 'plugin:other' });
    ended(TOLLIVER, 'plugin:watch');
    expect(now()).toMatchObject({ owner: 'plugin:other' });

    alert(TOLLIVER, { owner: 'plugin:watch' });
    ended(ORLA, 'plugin:watch');
    expect(now()).toMatchObject({ owner: 'plugin:watch' });

    alert(TOLLIVER);
    ended(TOLLIVER, 'plugin:watch');
    expect(now()).toMatchObject({ session: TOLLIVER, owner: null });
  });

  it('clears once its session leaves the list', async () => {
    const { now } = await load();
    alert(TOLLIVER);
    fire('vosh://sessions-changed', [row(ORLA, 'Orla')]);
    expect(now()).toBeNull();
  });
});
