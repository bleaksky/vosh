import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// Drives the row store through a fake Tauri event bus with two
// sessions, Tolliver's (1) and Orla's (2) on the build port, to hold it
// to what a row in the sessions sidebar says (board 3 of the Sessions
// review). Each test loads fresh store modules.

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

const TOLLIVER = 1;
const ORLA = 2;

/** A write to a session's terminal, as the session sends it. The prompt
 *  stage gives each of its writes an id, and Vosh's own echoes carry
 *  none. */
const output = (session: number, text: string, id: number | null = 7) =>
  fire('session://output', {
    session,
    b64: btoa(text),
    ...(id === null ? {} : { id }),
  });

/** An alert that rang in a session, as alert.rs sends it. */
const alert = (session: number) =>
  fire('session://alert', {
    session,
    title: 'Tell from Maren',
    label: null,
    words: null,
    sound: null,
    banner: true,
    notice: false,
    source: 'preset:alert_tells',
    owner: null,
  });

async function load() {
  const sessions = await import('./sessionsStore');
  const rows = await import('./sessionRowStore');
  sessions.startSessionsStore();
  rows.startSessionRowStore();
  await settle();
  return { sessions, rows };
}

beforeEach(() => {
  vi.resetModules();
  handlers.clear();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('the marks on a session row', () => {
  it('brighten the name of a session behind for a line from the game', async () => {
    const { rows } = await load();
    output(ORLA, 'A Blackwatch villager scurries about, taking care of business.\n\r');
    expect(rows.getSessionRow(ORLA)).toEqual({ lines: true, alert: false });
    expect(rows.getSessionRow(TOLLIVER)).toEqual({ lines: false, alert: false });
  });

  it('never count a prompt alone or an echo of Vosh', async () => {
    const { rows } = await load();
    output(ORLA, '<850hp 760m 250mv> ');
    output(ORLA, '[reconnect] Try 1 failed (connection refused)\r\n', null);
    expect(rows.getSessionRow(ORLA).lines).toBe(false);
  });

  it('show the dot for an alert that rang in a session behind', async () => {
    const { rows } = await load();
    alert(ORLA);
    expect(rows.getSessionRow(ORLA)).toEqual({ lines: false, alert: true });
  });

  it('never mark the selected session', async () => {
    const { rows } = await load();
    output(TOLLIVER, 'The Bank of Aabahran\n\r');
    alert(TOLLIVER);
    expect(rows.getSessionRow(TOLLIVER)).toEqual({ lines: false, alert: false });
  });

  it('clear as the session is selected, and stay clear while it shows', async () => {
    const { sessions, rows } = await load();
    output(ORLA, 'The Bank of Aabahran\n\r');
    alert(ORLA);
    sessions.select(ORLA);
    expect(rows.getSessionRow(ORLA)).toEqual({ lines: false, alert: false });
    output(ORLA, '[Exits: south]\n\r');
    expect(rows.getSessionRow(ORLA).lines).toBe(false);
    // Tolliver went behind, so his lines mark him now.
    output(TOLLIVER, '[Exits: south]\n\r');
    expect(rows.getSessionRow(TOLLIVER).lines).toBe(true);
  });

  it('outlast the link, so a drop keeps what came before it', async () => {
    const { rows } = await load();
    output(ORLA, 'The Bank of Aabahran\n\r');
    fire('session://state', { session: ORLA, kind: 'disconnected', reason: 'Connection reset' });
    expect(rows.getSessionRow(ORLA).lines).toBe(true);
  });
});

describe('how a row draws its marks', () => {
  it('shows the dot and the brighter name only on a row you are not looking at', async () => {
    const { rows } = await load();
    const marked = { lines: true, alert: true };
    expect(rows.rowLook(marked, false)).toEqual({ glyph: 'dot', tone: 'new' });
    expect(rows.rowLook(marked, true)).toEqual({ glyph: null, tone: null });
    expect(rows.rowLook({ lines: true, alert: false }, false)).toEqual({
      glyph: null,
      tone: 'new',
    });
  });
});
