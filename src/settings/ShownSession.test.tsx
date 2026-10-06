import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { SessionRow } from '../ipc/session';

// The session and profile Settings names at the right of its header,
// board 7 and board 9 of the Sessions review. The sessions come through
// a fake Tauri event bus, and each test loads fresh modules, since the
// stores keep the list at module scope.

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();
let rows: SessionRow[] = [];

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
  invoke: async (cmd: string) => (cmd === 'sessions_list' ? rows : null),
}));

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const row = (id: number, patch: Partial<SessionRow>): SessionRow => ({
  id,
  name: null,
  character: 'Tolliver',
  host: 'play.theforsakenlands.com',
  port: 1848,
  tls: false,
  profile: 'default',
  connected: true,
  selected: false,
  ...patch,
});

const TOLLIVER = row(1, {});
const ORLA = row(2, { character: 'Orla', port: 1825, profile: 'Build' });

/** The header as Settings draws it over `list`. */
async function header(list: SessionRow[]): Promise<string> {
  rows = list;
  const { ShownSession } = await import('./ShownSession');
  // The first draw starts the stores, and the list lands after.
  renderToStaticMarkup(<ShownSession />);
  await settle();
  return renderToStaticMarkup(<ShownSession />);
}

/** The header's words, one entry for each part. */
const parts = (html: string) =>
  [...html.matchAll(/<span class="(st-who-[^"]+)">([^<]*)</g)].map((m) => [m[1], m[2]]);

beforeEach(() => {
  vi.resetModules();
  handlers.clear();
});

describe('the Settings header', () => {
  it('stays away with one session', async () => {
    expect(await header([{ ...TOLLIVER, selected: true }])).toBe('');
  });

  it('names the selected session and its profile', async () => {
    const html = await header([TOLLIVER, { ...ORLA, selected: true }]);
    expect(html).toContain('class="shell-dot is-connected"');
    expect(parts(html)).toEqual([
      ['st-who-name', 'Orla'],
      ['st-who-profile', 'Build'],
    ]);
  });

  it('shows Also in while another session plays the same profile', async () => {
    const builder = row(2, { name: 'Builder', port: 1825, selected: true });
    expect(parts(await header([TOLLIVER, builder]))).toEqual([
      ['st-who-name', 'Builder'],
      ['st-who-profile', 'Default'],
      ['st-who-also', 'Also in Tolliver'],
    ]);
  });

  it('names a session before login by its world and port', async () => {
    const login = { ...ORLA, character: null, connected: false, selected: true };
    const html = await header([TOLLIVER, login]);
    expect(html).toContain('class="shell-dot is-idle"');
    expect(parts(html)[0]).toEqual(['st-who-name', 'The Forsaken Lands 1825']);
  });

  it('follows the selection to another session', async () => {
    await header([{ ...TOLLIVER, selected: true }, ORLA]);
    for (const cb of handlers.get('vosh://sessions-changed') ?? []) {
      cb({ payload: [TOLLIVER, { ...ORLA, selected: true }] });
    }
    const { ShownSession } = await import('./ShownSession');
    expect(parts(renderToStaticMarkup(<ShownSession />))[0]).toEqual(['st-who-name', 'Orla']);
  });
});
