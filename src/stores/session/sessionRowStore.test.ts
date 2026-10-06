import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { SessionRow } from '../../ipc/session';

// Drives the row store through a fake Tauri event bus with Tolliver's
// session (1) selected and Orla's (2) on the build port behind it, to
// hold it to what a row in the sessions sidebar says (board 3 of the
// Sessions review). Each test loads fresh store modules.

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
const PLAY = 'play.theforsakenlands.com';

/** A session's row as the app lists it. */
const row = (fields: Partial<SessionRow> = {}): SessionRow => ({
  id: ORLA,
  name: null,
  character: 'Orla',
  host: PLAY,
  port: 1825,
  tls: false,
  profile: 'Default',
  connected: true,
  selected: false,
  ...fields,
});
/** Orla's row at the build port's login, before Char.Status. */
const LOGIN = row({ character: null });

/** A write to a session's terminal, as the session sends it. The prompt
 *  stage gives each of its writes an id, and Vosh's own echoes carry
 *  none. */
const output = (session: number, text: string, id: number | null = 7) =>
  fire('session://output', { session, b64: btoa(text), ...(id === null ? {} : { id }) });

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

const connecting = (session: number, host = PLAY) =>
  fire('session://state', { session, kind: 'connecting', host, port: 1825, tls: false });
const connected = (session: number, host = PLAY) =>
  fire('session://state', { session, kind: 'connected', host, port: 1825, tls: false });
const disconnected = (session: number, reason: string | null) =>
  fire('session://state', { session, kind: 'disconnected', reason });
const redial = (session: number, payload: object) =>
  fire('session://reconnect', { session, ...payload });
const gmcp = (session: number, pkg: string, data: unknown) =>
  fire(`session://gmcp/${pkg.replace(/\./g, '-')}`, { session, data });

async function load() {
  const sessions = await import('./sessionsStore');
  const rows = await import('./sessionRowStore');
  sessions.startSessionsStore();
  rows.startSessionRowStore();
  await settle();
  /** How Orla's row draws now, with `of` her row in the list. */
  const look = (of: SessionRow = LOGIN) =>
    rows.rowLook(rows.getSessionRow(ORLA), of, sessions.getSelected() === ORLA);
  return { sessions, rows, look };
}

beforeEach(() => {
  vi.resetModules();
  handlers.clear();
});

describe('the marks on a session row', () => {
  it('brighten the name of a session behind for a line from the game', async () => {
    const { rows } = await load();
    output(ORLA, 'A Blackwatch villager scurries about, taking care of business.\n\r');
    expect(rows.getSessionRow(ORLA)).toMatchObject({ lines: true, alert: false });
    expect(rows.getSessionRow(TOLLIVER)).toMatchObject({ lines: false, alert: false });
  });

  it('never count a prompt alone or an echo of Vosh', async () => {
    const { rows } = await load();
    output(ORLA, '<850hp 760m 250mv> ');
    output(ORLA, '[walk] You are not walking.\r\n', null);
    expect(rows.getSessionRow(ORLA).lines).toBe(false);
  });

  it('show the dot for an alert that rang in a session behind', async () => {
    const { rows, look } = await load();
    alert(ORLA);
    expect(rows.getSessionRow(ORLA)).toMatchObject({ lines: false, alert: true });
    expect(look(row())).toEqual({ glyph: 'dot', tone: null });
  });

  it('show the dot for something an alert that is off would ring', async () => {
    const { rows, look } = await load();
    fire('session://mark', { session: ORLA });
    expect(rows.getSessionRow(ORLA)).toMatchObject({ lines: false, alert: true });
    expect(look(row())).toEqual({ glyph: 'dot', tone: null });
    fire('session://mark', { session: TOLLIVER });
    expect(rows.getSessionRow(TOLLIVER).alert).toBe(false);
  });

  it('never mark the selected session', async () => {
    const { rows } = await load();
    output(TOLLIVER, 'The Bank of Aabahran\n\r');
    alert(TOLLIVER);
    expect(rows.getSessionRow(TOLLIVER)).toMatchObject({ lines: false, alert: false });
  });

  it('clear as the session is selected, and stay clear while it shows', async () => {
    const { sessions, rows, look } = await load();
    output(ORLA, 'The Bank of Aabahran\n\r');
    alert(ORLA);
    expect(look(row())).toEqual({ glyph: 'dot', tone: 'new' });
    sessions.select(ORLA);
    expect(rows.getSessionRow(ORLA)).toMatchObject({ lines: false, alert: false });
    output(ORLA, '[Exits: south]\n\r');
    alert(ORLA);
    expect(look(row())).toEqual({ glyph: null, tone: null });
    // Tolliver went behind, so his lines mark him now.
    output(TOLLIVER, '[Exits: south]\n\r');
    expect(rows.getSessionRow(TOLLIVER).lines).toBe(true);
  });

  it('outlast the link, so a drop keeps what came before it', async () => {
    const { rows } = await load();
    output(ORLA, 'The Bank of Aabahran\n\r');
    disconnected(ORLA, 'server closed connection');
    expect(rows.getSessionRow(ORLA).lines).toBe(true);
  });
});

describe('the glyph of a session row', () => {
  it('reads the list until the session says more', async () => {
    const { look } = await load();
    expect(look(row())).toEqual({ glyph: null, tone: null });
    expect(look(LOGIN)).toEqual({ glyph: 'hand', tone: null });
    // A session launch restored, or one never connected, dims.
    expect(look(row({ connected: false }))).toEqual({ glyph: null, tone: 'off' });
  });

  it('turns while Vosh dials, then shows the hand until Char.Status', async () => {
    const { look } = await load();
    connecting(ORLA);
    expect(look()).toEqual({ glyph: 'spinner', tone: null });
    connected(ORLA);
    expect(look()).toEqual({ glyph: 'hand', tone: null });
    gmcp(ORLA, 'Char.Status', { name: 'Orla', level: 51 });
    expect(look()).toEqual({ glyph: null, tone: null });
  });

  it('shows no hand on a world Vosh does not know sends Char.Status', async () => {
    const { look } = await load();
    connecting(ORLA, 'mud.example.org');
    connected(ORLA, 'mud.example.org');
    expect(look(row({ character: null, host: 'mud.example.org' }))).toEqual({
      glyph: null,
      tone: null,
    });
  });

  it('shows the triangle when the first dial fails, and dims after your disconnect', async () => {
    const { look } = await load();
    connecting(ORLA);
    disconnected(ORLA, 'io error: Connection refused (os error 61)');
    expect(look()).toEqual({ glyph: 'triangle', tone: null });
    connecting(ORLA);
    connected(ORLA);
    disconnected(ORLA, null);
    expect(look()).toEqual({ glyph: null, tone: 'off' });
  });

  it('turns through every try of a redial, a failed one included', async () => {
    const { look } = await load();
    connected(ORLA);
    gmcp(ORLA, 'Char.Status', { name: 'Orla' });
    disconnected(ORLA, 'server closed connection');
    redial(ORLA, { kind: 'waiting', try: 1, tries: 8, seconds: 3 });
    expect(look(row())).toEqual({ glyph: 'spinner', tone: null });
    redial(ORLA, { kind: 'dialing', try: 1, tries: 8 });
    connecting(ORLA);
    disconnected(ORLA, null);
    expect(look()).toEqual({ glyph: 'spinner', tone: null });
    redial(ORLA, { kind: 'failed', try: 1, tries: 8, reason: 'connection refused' });
    expect(look()).toEqual({ glyph: 'spinner', tone: null });
    redial(ORLA, { kind: 'waiting', try: 2, tries: 8, seconds: 6 });
    expect(look()).toEqual({ glyph: 'spinner', tone: null });
  });

  it('names the character it played through a redial, then shows the hand at the game', async () => {
    const { look } = await load();
    const { sessionLabel } = await import('../../lib/sessionLabel');
    connected(ORLA);
    gmcp(ORLA, 'Char.Status', { name: 'Orla' });
    disconnected(ORLA, 'server closed connection');
    redial(ORLA, { kind: 'waiting', try: 1, tries: 8, seconds: 3 });
    redial(ORLA, { kind: 'dialing', try: 1, tries: 8 });
    connecting(ORLA);
    // The list keeps Orla on her row through every try, so two sessions
    // that redial on one world read apart.
    const kept = row({ connected: false });
    expect(look(kept)).toEqual({ glyph: 'spinner', tone: null });
    expect(sessionLabel(kept, [kept]).name).toBe('Orla');
    connected(ORLA);
    redial(ORLA, { kind: 'reached', try: 1 });
    expect(look(row())).toEqual({ glyph: 'hand', tone: null });
    gmcp(ORLA, 'Char.Status', { name: 'Orla' });
    expect(look(row())).toEqual({ glyph: null, tone: null });
  });

  it('shows the hand once a redial reached the game, on any world, until you play', async () => {
    const { look } = await load();
    const there = row({ character: null, host: 'mud.example.org' });
    redial(ORLA, { kind: 'dialing', try: 2, tries: 8 });
    connecting(ORLA, 'mud.example.org');
    connected(ORLA, 'mud.example.org');
    redial(ORLA, { kind: 'reached', try: 2 });
    expect(look(there)).toEqual({ glyph: 'hand', tone: null });
    // A character left link dead takes you back with no Char.Status, and
    // the vitals say you play.
    gmcp(ORLA, 'Char.Vitals', { hp: 850, maxhp: 900 });
    expect(look(there)).toEqual({ glyph: null, tone: null });
  });

  it('shows the triangle when the tries run out or Reconnect is off', async () => {
    const { look } = await load();
    redial(ORLA, { kind: 'waiting', try: 8, tries: 8, seconds: 60 });
    redial(ORLA, { kind: 'stopped', tries: 8 });
    expect(look()).toEqual({ glyph: 'triangle', tone: null });
    connecting(ORLA);
    connected(ORLA);
    disconnected(ORLA, 'server closed connection');
    redial(ORLA, { kind: 'declined', why: 'off' });
    expect(look(row({ connected: false }))).toEqual({ glyph: 'triangle', tone: null });
  });

  it('dims the name when a drop is expected or the redial is cancelled', async () => {
    const { look } = await load();
    for (const why of ['quit', 'banned', 'taken']) {
      connected(ORLA);
      disconnected(ORLA, 'server closed connection');
      redial(ORLA, { kind: 'declined', why });
      expect(look(row({ connected: false }))).toEqual({ glyph: null, tone: 'off' });
    }
    // A cancel can cut a try short as it dials, with no state after it.
    redial(ORLA, { kind: 'dialing', try: 3, tries: 8 });
    connecting(ORLA);
    redial(ORLA, { kind: 'cancelled' });
    expect(look()).toEqual({ glyph: null, tone: 'off' });
  });
});

describe('which glyph a row shows', () => {
  const quiet = {
    link: null,
    redialing: false,
    reached: false,
    playing: false,
    lines: false,
    alert: false,
  } as const;

  it('puts the triangle first, then the hand, then the spinner, then the dot', async () => {
    const { rows } = await load();
    const glyph = (state: object, of = LOGIN, selected = false) =>
      rows.rowLook({ ...quiet, ...state }, of, selected).glyph;
    expect(glyph({ link: 'failed', redialing: true, alert: true })).toBe('triangle');
    expect(glyph({ link: 'live', redialing: true, alert: true })).toBe('hand');
    expect(glyph({ link: 'dialing', alert: true })).toBe('spinner');
    expect(glyph({ link: 'live', alert: true }, row())).toBe('dot');
  });

  it('never shows the dot or a tone on the selected row, and still shows its link', async () => {
    const { rows } = await load();
    const marked = { ...quiet, lines: true, alert: true };
    expect(rows.rowLook(marked, row(), true)).toEqual({ glyph: null, tone: null });
    expect(rows.rowLook({ ...marked, link: 'dialing' }, row(), true)).toEqual({
      glyph: 'spinner',
      tone: null,
    });
    expect(rows.rowLook({ ...quiet, link: 'down' }, row(), true)).toEqual({
      glyph: null,
      tone: null,
    });
  });
});
