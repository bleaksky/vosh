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
  since: null,
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
const alert = (session: number, source = 'preset:alert_tells') =>
  fire('session://alert', {
    session,
    title: 'Tell from Maren',
    label: null,
    words: null,
    sound: null,
    banner: true,
    notice: false,
    source,
    owner: null,
  });
/** Something an alert would ring that rang nothing, as alert.rs sends
 *  it. */
const mark = (session: number, source: string) => fire('session://mark', { session, source });

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
    expect(rows.getSessionRow(ORLA)).toMatchObject({ lines: true, waiting: [] });
    expect(rows.getSessionRow(TOLLIVER)).toMatchObject({ lines: false, waiting: [] });
  });

  it('never count a prompt alone or an echo of Vosh', async () => {
    const { rows } = await load();
    output(ORLA, '<850hp 760m 250mv> ');
    output(ORLA, '[walk] You are not walking.\r\n', null);
    expect(rows.getSessionRow(ORLA).lines).toBe(false);
  });

  it('count an alert that rang in a session behind', async () => {
    const { rows, look } = await load();
    alert(ORLA);
    expect(rows.getSessionRow(ORLA)).toMatchObject({
      lines: false,
      waiting: ['preset:alert_tells'],
    });
    expect(look(row())).toEqual({ mark: 'live', count: 1, tone: null });
  });

  it('count something an alert that is off would ring', async () => {
    const { rows, look } = await load();
    mark(ORLA, 'preset:alert_tells');
    expect(rows.getSessionRow(ORLA)).toMatchObject({
      lines: false,
      waiting: ['preset:alert_tells'],
    });
    expect(look(row())).toEqual({ mark: 'live', count: 1, tone: null });
    mark(TOLLIVER, 'preset:alert_tells');
    expect(rows.getSessionRow(TOLLIVER).waiting).toEqual([]);
  });

  it('never mark the selected session', async () => {
    const { rows } = await load();
    output(TOLLIVER, 'The Bank of Aabahran\n\r');
    alert(TOLLIVER);
    expect(rows.getSessionRow(TOLLIVER)).toMatchObject({ lines: false, waiting: [] });
  });

  it('clear as the session is selected, and stay clear while it shows', async () => {
    const { sessions, rows, look } = await load();
    output(ORLA, 'The Bank of Aabahran\n\r');
    alert(ORLA);
    expect(look(row())).toEqual({ mark: 'live', count: 1, tone: 'new' });
    sessions.select(ORLA);
    expect(rows.getSessionRow(ORLA)).toMatchObject({ lines: false, waiting: [] });
    output(ORLA, '[Exits: south]\n\r');
    alert(ORLA);
    expect(look(row())).toEqual({ mark: 'live', count: 0, tone: null });
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

describe('what waits in a session row', () => {
  it('counts each tell, and names what waits in plain words', async () => {
    const { rows } = await load();
    alert(ORLA);
    mark(ORLA, 'preset:alert_tells');
    expect(rows.getSessionRow(ORLA).waiting).toHaveLength(2);
    expect(rows.waitingWords(rows.getSessionRow(ORLA).waiting)).toBe('2 tells');
    mark(ORLA, 'preset:alert_attacked');
    mark(ORLA, 'preset:alert_name');
    expect(rows.waitingWords(rows.getSessionRow(ORLA).waiting)).toBe(
      '2 tells, your name came up, a fight started',
    );
  });

  it('counts low health once however often it falls', async () => {
    const { rows } = await load();
    mark(ORLA, 'preset:alert_attacked');
    alert(ORLA, 'preset:alert_low_health');
    mark(ORLA, 'preset:alert_low_health');
    const { waiting } = rows.getSessionRow(ORLA);
    expect(waiting).toEqual(['preset:alert_attacked', 'preset:alert_low_health']);
    expect(rows.waitingWords(waiting)).toBe('A fight started, health is low');
  });

  it('never counts the connection, a trigger or Lua', async () => {
    const { rows } = await load();
    alert(ORLA, 'preset:alert_connection');
    mark(ORLA, 'preset:alert_connection');
    mark(ORLA, 'trigger:visitor');
    alert(ORLA, 'lua:plugin:vitals_alert');
    expect(rows.getSessionRow(ORLA).waiting).toEqual([]);
    expect(rows.waitingWords([])).toBeNull();
  });

  it('clears as you select the session, and totals the others for the band', async () => {
    const { sessions, rows } = await load();
    const list = [row({ id: TOLLIVER }), row()];
    mark(ORLA, 'preset:alert_tells');
    mark(ORLA, 'preset:alert_attacked');
    expect(rows.waitingElsewhere(list, TOLLIVER)).toBe(2);
    expect(rows.waitingElsewhere(list, ORLA)).toBe(0);
    sessions.select(ORLA);
    expect(rows.getSessionRow(ORLA).waiting).toEqual([]);
    mark(TOLLIVER, 'preset:alert_name');
    expect(rows.waitingElsewhere(list, ORLA)).toBe(1);
  });
});

describe('the link of a session row', () => {
  it('keeps when the link dropped through a redial that stopped, until a connect', async () => {
    const { rows } = await load();
    vi.spyOn(Date, 'now').mockReturnValue(1000);
    connected(ORLA);
    expect(rows.getSessionRow(ORLA).downAt).toBeNull();
    disconnected(ORLA, 'server closed connection');
    vi.spyOn(Date, 'now').mockReturnValue(5000);
    redial(ORLA, { kind: 'dialing', try: 1, tries: 8 });
    connecting(ORLA);
    disconnected(ORLA, 'io error: Connection refused (os error 61)');
    redial(ORLA, { kind: 'stopped', tries: 8 });
    expect(rows.getSessionRow(ORLA).downAt).toBe(1000);
    connecting(ORLA);
    connected(ORLA);
    expect(rows.getSessionRow(ORLA).downAt).toBeNull();
    // Your Disconnect is no drop.
    disconnected(ORLA, null);
    expect(rows.getSessionRow(ORLA).downAt).toBeNull();
    vi.restoreAllMocks();
  });

  it('keeps the time a first dial failed, and that it never reached the game', async () => {
    const { rows } = await load();
    vi.spyOn(Date, 'now').mockReturnValue(2000);
    connecting(ORLA);
    disconnected(ORLA, 'io error: Connection refused (os error 61)');
    expect(rows.getSessionRow(ORLA)).toMatchObject({ downAt: 2000, refused: true });
    connecting(ORLA);
    connected(ORLA);
    disconnected(ORLA, 'server closed connection');
    expect(rows.getSessionRow(ORLA).refused).toBe(false);
    vi.restoreAllMocks();
  });

  it('carries the try a redial is on while it waits and dials', async () => {
    const { rows } = await load();
    const tried = () => {
      const { try: on, tries } = rows.getSessionRow(ORLA);
      return [on, tries];
    };
    redial(ORLA, { kind: 'waiting', try: 3, tries: 8, seconds: 12 });
    expect(tried()).toEqual([3, 8]);
    redial(ORLA, { kind: 'dialing', try: 3, tries: 8 });
    expect(tried()).toEqual([3, 8]);
    redial(ORLA, { kind: 'failed', try: 3, tries: 8, reason: 'connection refused' });
    expect(tried()).toEqual([3, 8]);
    redial(ORLA, { kind: 'reached', try: 4 });
    expect(tried()).toEqual([null, null]);
    redial(ORLA, { kind: 'waiting', try: 1, tries: 8, seconds: 3 });
    redial(ORLA, { kind: 'cancelled' });
    expect(tried()).toEqual([null, null]);
  });
});

describe('the mark of a session row', () => {
  it('reads the list until the session says more', async () => {
    const { look } = await load();
    expect(look(row())).toEqual({ mark: 'live', count: 0, tone: null });
    expect(look(LOGIN)).toEqual({ mark: 'hand', count: 0, tone: null });
    // A session launch restored, or one never connected, dims.
    expect(look(row({ connected: false }))).toEqual({ mark: 'off', count: 0, tone: 'off' });
  });

  it('turns while Vosh dials, then shows the hand until Char.Status', async () => {
    const { look } = await load();
    connecting(ORLA);
    expect(look()).toEqual({ mark: 'spinner', count: 0, tone: null });
    connected(ORLA);
    expect(look()).toEqual({ mark: 'hand', count: 0, tone: null });
    gmcp(ORLA, 'Char.Status', { name: 'Orla', level: 51 });
    expect(look()).toEqual({ mark: 'live', count: 0, tone: null });
  });

  it('shows no hand on a world Vosh does not know sends Char.Status', async () => {
    const { look } = await load();
    connecting(ORLA, 'mud.example.org');
    connected(ORLA, 'mud.example.org');
    expect(look(row({ character: null, host: 'mud.example.org' }))).toEqual({
      mark: 'live',
      count: 0,
      tone: null,
    });
  });

  it('shows the triangle when the first dial fails, and dims after your disconnect', async () => {
    const { look } = await load();
    connecting(ORLA);
    disconnected(ORLA, 'io error: Connection refused (os error 61)');
    expect(look()).toEqual({ mark: 'triangle', count: 0, tone: null });
    connecting(ORLA);
    connected(ORLA);
    disconnected(ORLA, null);
    expect(look()).toEqual({ mark: 'off', count: 0, tone: 'off' });
  });

  it('turns through every try of a redial, a failed one included', async () => {
    const { look } = await load();
    connected(ORLA);
    gmcp(ORLA, 'Char.Status', { name: 'Orla' });
    disconnected(ORLA, 'server closed connection');
    redial(ORLA, { kind: 'waiting', try: 1, tries: 8, seconds: 3 });
    expect(look(row())).toEqual({ mark: 'spinner', count: 0, tone: null });
    redial(ORLA, { kind: 'dialing', try: 1, tries: 8 });
    connecting(ORLA);
    disconnected(ORLA, null);
    expect(look()).toEqual({ mark: 'spinner', count: 0, tone: null });
    redial(ORLA, { kind: 'failed', try: 1, tries: 8, reason: 'connection refused' });
    expect(look()).toEqual({ mark: 'spinner', count: 0, tone: null });
    redial(ORLA, { kind: 'waiting', try: 2, tries: 8, seconds: 6 });
    expect(look()).toEqual({ mark: 'spinner', count: 0, tone: null });
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
    expect(look(kept)).toEqual({ mark: 'spinner', count: 0, tone: null });
    expect(sessionLabel(kept, [kept]).name).toBe('Orla');
    connected(ORLA);
    redial(ORLA, { kind: 'reached', try: 1 });
    expect(look(row())).toEqual({ mark: 'hand', count: 0, tone: null });
    gmcp(ORLA, 'Char.Status', { name: 'Orla' });
    expect(look(row())).toEqual({ mark: 'live', count: 0, tone: null });
  });

  it('shows the hand once a redial reached the game, on any world, until you play', async () => {
    const { look } = await load();
    const there = row({ character: null, host: 'mud.example.org' });
    redial(ORLA, { kind: 'dialing', try: 2, tries: 8 });
    connecting(ORLA, 'mud.example.org');
    connected(ORLA, 'mud.example.org');
    redial(ORLA, { kind: 'reached', try: 2 });
    expect(look(there)).toEqual({ mark: 'hand', count: 0, tone: null });
    // A character left link dead takes you back with no Char.Status, and
    // the vitals say you play.
    gmcp(ORLA, 'Char.Vitals', { hp: 850, maxhp: 900 });
    expect(look(there)).toEqual({ mark: 'live', count: 0, tone: null });
  });

  it('shows the triangle when the tries run out or Reconnect is off', async () => {
    const { look } = await load();
    redial(ORLA, { kind: 'waiting', try: 8, tries: 8, seconds: 60 });
    redial(ORLA, { kind: 'stopped', tries: 8 });
    expect(look()).toEqual({ mark: 'triangle', count: 0, tone: null });
    connecting(ORLA);
    connected(ORLA);
    disconnected(ORLA, 'server closed connection');
    redial(ORLA, { kind: 'declined', why: 'off' });
    expect(look(row({ connected: false }))).toEqual({ mark: 'triangle', count: 0, tone: null });
  });

  it('dims the name when a drop is expected or the redial is cancelled', async () => {
    const { look } = await load();
    for (const why of ['quit', 'closing', 'taken']) {
      connected(ORLA);
      disconnected(ORLA, 'server closed connection');
      redial(ORLA, { kind: 'declined', why });
      expect(look(row({ connected: false }))).toEqual({ mark: 'off', count: 0, tone: 'off' });
    }
    // A cancel can cut a try short as it dials, with no state after it.
    redial(ORLA, { kind: 'dialing', try: 3, tries: 8 });
    connecting(ORLA);
    redial(ORLA, { kind: 'cancelled' });
    expect(look()).toEqual({ mark: 'off', count: 0, tone: 'off' });
  });
});

describe('which mark a row shows', () => {
  const quiet = {
    link: null,
    redialing: false,
    reached: false,
    playing: false,
    lines: false,
    waiting: [],
    downAt: null,
    refused: false,
    try: null,
    tries: null,
  } as const;
  const tells = ['preset:alert_tells'];

  it('puts the triangle first, then the hand, then the spinner, then the dot or the ring', async () => {
    const { rows } = await load();
    const mark = (state: object, of = LOGIN) =>
      rows.rowLook({ ...quiet, ...state }, of, false).mark;
    expect(mark({ link: 'failed', redialing: true, waiting: tells })).toBe('triangle');
    expect(mark({ link: 'live', redialing: true, waiting: tells })).toBe('hand');
    expect(mark({ link: 'dialing', waiting: tells })).toBe('spinner');
    expect(mark({ link: 'live', waiting: tells }, row())).toBe('live');
    expect(mark({ link: 'down', waiting: tells })).toBe('off');
  });

  it('draws each state of board 02 with its mark, its words, its count and its tone', async () => {
    const { rows } = await load();
    const two = ['preset:alert_attacked', 'preset:alert_low_health'];
    const states: [string, object, SessionRow, boolean, object][] = [
      ['selected, playing', { link: 'live', playing: true }, row(), true, { mark: 'live' }],
      ['playing', { link: 'live', playing: true }, row(), false, { mark: 'live' }],
      ['new lines', { link: 'live', lines: true }, row(), false, { mark: 'live', tone: 'new' }],
      ['waiting', { link: 'live', waiting: two }, row(), false, { mark: 'live', count: 2 }],
      ['logging in', { link: 'live' }, LOGIN, false, { mark: 'hand' }],
      ['connecting', { link: 'dialing' }, LOGIN, false, { mark: 'spinner' }],
      [
        'reconnecting',
        { link: 'dialing', redialing: true, try: 3, tries: 8 },
        row(),
        false,
        { mark: 'spinner' },
      ],
      ['dropped', { link: 'failed', downAt: 1000 }, row(), false, { mark: 'triangle' }],
      [
        'could not connect',
        { link: 'failed', downAt: 1000, refused: true },
        row(),
        false,
        { mark: 'triangle' },
      ],
      ['not connected', { link: 'down' }, row(), false, { mark: 'off', tone: 'off' }],
      [
        'not connected, from the list',
        {},
        row({ connected: false }),
        false,
        { mark: 'off', tone: 'off' },
      ],
    ];
    for (const [, state, of, selected, want] of states) {
      expect(rows.rowLook({ ...quiet, ...state }, of, selected)).toEqual({
        count: 0,
        tone: null,
        ...want,
      });
    }
    expect(rows.MARK_WORDS).toEqual({
      live: 'Playing',
      off: 'Not connected',
      hand: 'Logging in',
      spinner: 'Connecting',
      triangle: 'Connect again',
    });
  });

  it('gives the selected row its mark and no tone', async () => {
    const { rows } = await load();
    const marked = { ...quiet, lines: true };
    expect(rows.rowLook(marked, row(), true)).toEqual({ mark: 'live', count: 0, tone: null });
    expect(rows.rowLook({ ...marked, link: 'dialing' }, row(), true)).toEqual({
      mark: 'spinner',
      count: 0,
      tone: null,
    });
    expect(rows.rowLook({ ...quiet, link: 'down' }, row(), true)).toEqual({
      mark: 'off',
      count: 0,
      tone: null,
    });
  });
});
