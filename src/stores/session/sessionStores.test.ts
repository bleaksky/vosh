import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// Drives the session stores through a fake Tauri event bus with two
// sessions, Tolliver's (1) and Orla's (2), to hold each to one value for
// each session: your target and quick keys, the tick, the pinned prompt,
// the triggers that hid your prompt, and the masked field. Each test
// loads fresh store modules.

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();
const commands = new Map<string, (args: { session?: number }) => unknown>();
/** The session each command was asked for, by command. */
const asked = new Map<string, (number | undefined)[]>();

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
  invoke: async (cmd: string, args: { session?: number } = {}) => {
    asked.set(cmd, [...(asked.get(cmd) ?? []), args.session]);
    const answer = commands.get(cmd);
    if (!answer) throw new Error(`no fake for ${cmd}`);
    return answer(args);
  },
}));

// The tick sound is Web Audio. tickSound.test checks that a session
// behind plays nothing, so here the store only has to name the session.
const playTickSound = vi.hoisted(() => vi.fn());
vi.mock('./tickSound', () => ({ playTickSound }));

function fire(event: string, payload: unknown): void {
  for (const cb of handlers.get(event) ?? []) cb({ payload });
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const TOLLIVER = 1;
const ORLA = 2;

const state = (session: number, kind: string) =>
  kind === 'disconnected'
    ? fire('session://state', { session, kind, reason: null })
    : fire('session://state', {
        session,
        kind,
        host: 'play.theforsakenlands.com',
        port: 1848,
        tls: false,
      });

/** The list the app sends, with `selected` the one selected. */
const select = (selected: number) =>
  fire(
    'vosh://sessions-changed',
    [TOLLIVER, ORLA].map((id) => ({
      id,
      name: null,
      character: id === TOLLIVER ? 'Tolliver' : 'Orla',
      host: 'play.theforsakenlands.com',
      port: id === TOLLIVER ? 1848 : 1825,
      tls: false,
      profile: id === TOLLIVER ? 'Tolliver' : 'Orla',
      connected: true,
      selected: id === selected,
    })),
  );

const QUICK_KEYS = [
  { name: '1', verb: 'kill' },
  { name: '2', verb: 'bash' },
];

/** A tick report `elapsed_ms` into a 30 second tick. */
const tick = (session: number, elapsed_ms: number, extra: object = {}) =>
  fire('session://tick', {
    session,
    enabled: true,
    interval_ms: 30_000,
    remaining_ms: Math.max(0, 30_000 - elapsed_ms),
    elapsed_ms,
    overdue: false,
    synced: true,
    fired: false,
    sound: false,
    ...extra,
  });

/** Output that pins `pin` above the command line. */
const pinned = (session: number, pin: string) =>
  fire('session://output', { session, b64: '', pin: btoa(pin) });

async function load() {
  const stores = await import('../index');
  stores.startStores();
  await settle();
  return {
    target: await import('./targetStore'),
    tick: await import('./tickStore'),
    pinned: await import('./pinnedPromptStore'),
    inputMode: await import('./inputModeStore'),
  };
}

beforeEach(() => {
  vi.resetModules();
  handlers.clear();
  commands.clear();
  asked.clear();
  playTickSound.mockClear();
  vi.stubGlobal('window', globalThis);
  commands.set('ui_get_config', () => ({}));
  commands.set('sessions_list', () => []);
  commands.set('hidden_get', () => null);
  commands.set('affects_snapshot_get', () => null);
  commands.set('affect_full_get', () => ({}));
  commands.set('tick_get_config', () => ({ enabled: true, interval_secs: 30, warn_at_secs: 5 }));
  commands.set('target_get', ({ session }) =>
    session === ORLA
      ? { name: 'Maren', room_idx: 2, quick_keys: [] }
      : { name: null, room_idx: null, quick_keys: QUICK_KEYS },
  );
  commands.set('prompt_gags_without_reader', () => []);
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe('the target store with two sessions', () => {
  it('keeps the target and quick keys of each session apart', async () => {
    const s = await load();
    expect(s.target.getTargetState()).toEqual({
      name: null,
      room_idx: null,
      quick_keys: QUICK_KEYS,
    });
    // Orla's first selection reads her target.
    select(ORLA);
    await settle();
    expect(s.target.getTargetState()).toEqual({ name: 'Maren', room_idx: 2, quick_keys: [] });
    fire('session://target', { session: TOLLIVER, name: 'Orla', room_idx: 1, quick_keys: [] });
    expect(s.target.getTargetState().name).toBe('Maren');
    select(TOLLIVER);
    expect(s.target.getTargetState()).toEqual({ name: 'Orla', room_idx: 1, quick_keys: [] });
    expect(asked.get('target_get')).toEqual([TOLLIVER, ORLA]);
  });

  it('clears only the target of the session that disconnects, and keeps its quick keys', async () => {
    const s = await load();
    fire('session://target', {
      session: TOLLIVER,
      name: 'Maren',
      room_idx: 3,
      quick_keys: QUICK_KEYS,
    });
    fire('session://target', { session: ORLA, name: 'Tolliver', room_idx: 1, quick_keys: [] });
    const before = s.target.getTargetState();
    state(ORLA, 'disconnected');
    expect(s.target.getTargetState()).toBe(before);
    state(TOLLIVER, 'disconnected');
    expect(s.target.getTargetState()).toEqual({
      name: null,
      room_idx: null,
      quick_keys: QUICK_KEYS,
    });
    select(ORLA);
    expect(s.target.getTargetState()).toEqual({ name: null, room_idx: null, quick_keys: [] });
  });
});

describe('the tick store with two sessions', () => {
  it('shows the count of the selected session', async () => {
    const s = await load();
    tick(TOLLIVER, 12_000);
    tick(ORLA, 27_000);
    expect(s.tick.getTick()).toMatchObject({ active: true, secsSinceTick: 12, warn: false });
    select(ORLA);
    expect(s.tick.getTick()).toMatchObject({ active: true, secsSinceTick: 27, warn: true });
  });

  it('leaves the count in front as it was when a session behind ticks, stops or drops', async () => {
    const s = await load();
    vi.useFakeTimers();
    tick(TOLLIVER, 12_000);
    tick(ORLA, 27_000, { fired: true, sound: true });
    expect(playTickSound).toHaveBeenCalledWith(ORLA);
    const shown = s.tick.getTick();
    const heard = vi.fn();
    s.tick.subscribeTick(heard);
    state(ORLA, 'disconnected');
    expect(s.tick.getTick()).toBe(shown);
    expect(heard).not.toHaveBeenCalled();

    // Tolliver keeps reporting while Orla's reports stop, and her count
    // hides on its own.
    tick(ORLA, 28_000);
    for (let at = 0; at < 2_000; at += 250) {
      vi.advanceTimersByTime(250);
      tick(TOLLIVER, 12_000);
    }
    expect(s.tick.getTick()).toBe(shown);
    select(ORLA);
    expect(s.tick.getTick().active).toBe(false);
  });
});

describe('the connection store with two sessions', () => {
  /** The list the app keeps, with Orla playing on the build port while
   *  Tolliver waits unconnected, and `selected` the one selected. */
  const rows = (selected: number, orla = true) =>
    [TOLLIVER, ORLA].map((id) => ({
      id,
      name: null,
      character: id === ORLA && orla ? 'Orla' : null,
      host: 'play.theforsakenlands.com',
      port: id === TOLLIVER ? 1848 : 1825,
      tls: false,
      profile: 'Default',
      connected: id === ORLA && orla,
      since: id === ORLA && orla ? 1_000 : null,
      selected: id === selected,
    }));

  const ORLA_LIVE = {
    status: { kind: 'connected', host: 'play.theforsakenlands.com', port: 1825, tls: false },
    character: 'Orla',
  };

  it('reads a session that connected before the page started from the list', async () => {
    commands.set('sessions_list', () => rows(ORLA));
    await load();
    const connection = await import('./connectionStore');
    expect(connection.getSessionConnection()).toEqual(ORLA_LIVE);
    expect(connection.sessionLive(ORLA)).toBe(true);
    expect(connection.sessionLive(TOLLIVER)).toBe(false);
  });

  it('keeps it through a connect of another session and a switch back', async () => {
    commands.set('sessions_list', () => rows(ORLA));
    await load();
    const connection = await import('./connectionStore');
    fire('vosh://sessions-changed', rows(TOLLIVER));
    expect(connection.getSessionConnection().status.kind).toBe('idle');
    state(TOLLIVER, 'connecting');
    state(TOLLIVER, 'connected');
    fire('vosh://sessions-changed', rows(TOLLIVER));
    expect(connection.getSessionConnection().status.kind).toBe('connected');
    fire('vosh://sessions-changed', rows(ORLA));
    expect(connection.getSessionConnection()).toEqual(ORLA_LIVE);
  });

  it('follows the events of a session once one names it', async () => {
    commands.set('sessions_list', () => rows(ORLA));
    await load();
    const connection = await import('./connectionStore');
    state(ORLA, 'disconnected');
    // A list that still says connected never brings the link back.
    fire('vosh://sessions-changed', rows(ORLA));
    expect(connection.getSessionConnection().status.kind).toBe('idle');
    fire('vosh://sessions-changed', rows(ORLA, false));
    expect(connection.sessionLive(ORLA)).toBe(false);
  });
});

describe('the pinned prompt store with two sessions', () => {
  it('shows the band of the selected session and clears only the one that disconnects', async () => {
    const s = await load();
    pinned(TOLLIVER, '<850/900hp 760/820m>');
    pinned(ORLA, '<1020/1020hp 410/410m>');
    expect(s.pinned.getPinnedPrompt()).toBe('<850/900hp 760/820m>');
    state(ORLA, 'disconnected');
    expect(s.pinned.getPinnedPrompt()).toBe('<850/900hp 760/820m>');
    select(ORLA);
    expect(s.pinned.getPinnedPrompt()).toBeNull();
    pinned(ORLA, '<1020/1020hp 410/410m>');
    expect(s.pinned.getPinnedPrompt()).toBe('<1020/1020hp 410/410m>');
    select(TOLLIVER);
    expect(s.pinned.getPinnedPrompt()).toBe('<850/900hp 760/820m>');
  });
});

describe('the prompt gag store with two sessions', () => {
  async function loadGags() {
    const s = await load();
    const gags = await import('./promptGagStore');
    gags.subscribePromptGags(() => {});
    await settle();
    return { ...s, gags };
  }
  const gag = (session: number, trigger: string) =>
    fire('session://prompt-gag-without-reader', { session, trigger });
  const names = (set: ReadonlySet<string>) => [...set];

  it('keeps the triggers each session named, and starts over only where a connection opens', async () => {
    const s = await loadGags();
    gag(TOLLIVER, 'tolliver-capture');
    gag(ORLA, 'orla-capture');
    expect(names(s.gags.getPromptGags())).toEqual(['tolliver-capture']);
    state(ORLA, 'connecting');
    expect(names(s.gags.getPromptGags())).toEqual(['tolliver-capture']);
    gag(ORLA, 'orla-capture');
    state(TOLLIVER, 'disconnected');
    expect(s.gags.getPromptGags().size).toBe(0);
    select(ORLA);
    expect(names(s.gags.getPromptGags())).toEqual(['orla-capture']);
  });

  it('asks the session now selected for its list and takes it whole', async () => {
    commands.set('prompt_gags_without_reader', ({ session }) =>
      session === ORLA ? [] : ['tolliver-capture'],
    );
    const s = await loadGags();
    expect(names(s.gags.getPromptGags())).toEqual(['tolliver-capture']);
    // Orla's profile came to read her prompt while she sat behind, so her
    // session forgot the trigger it named.
    gag(ORLA, 'orla-capture');
    select(ORLA);
    await settle();
    expect(s.gags.getPromptGags().size).toBe(0);
    expect(asked.get('prompt_gags_without_reader')).toContain(ORLA);
  });
});

describe('the masked field with two sessions', () => {
  it('masks only while the selected session asks for a password', async () => {
    const s = await load();
    fire('session://input-mode', { session: ORLA, password: true });
    expect(s.inputMode.getPasswordMode()).toBe(false);
    select(ORLA);
    expect(s.inputMode.getPasswordMode()).toBe(true);
    state(TOLLIVER, 'disconnected');
    expect(s.inputMode.getPasswordMode()).toBe(true);
    select(TOLLIVER);
    expect(s.inputMode.getPasswordMode()).toBe(false);
  });
});

describe('the connection of a session that drops', () => {
  it('is not live once the link drops, with a reason or without', async () => {
    await load();
    const { sessionLive } = await import('./connectionStore');
    state(TOLLIVER, 'connected');
    state(ORLA, 'connected');
    expect(sessionLive(TOLLIVER)).toBe(true);
    fire('session://state', {
      session: TOLLIVER,
      kind: 'disconnected',
      reason: 'server closed connection',
    });
    fire('session://reconnect', {
      session: TOLLIVER,
      kind: 'failed',
      try: 1,
      tries: 8,
      reason: 'no answer in 10 seconds',
    });
    state(ORLA, 'disconnected');
    expect(sessionLive(TOLLIVER)).toBe(false);
    expect(sessionLive(ORLA)).toBe(false);
  });
});
