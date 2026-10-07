import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import lament from '../../../fixtures/gmcp/aabahran/lament.json';
import { TimersView } from '../../panel/affects/AffectsTimers';
import { GroupPaneView } from '../../panel/group/GroupPane';
import { PaneLeafContext } from '../../panel/paneActions';
import { VitalsBlock } from '../../panel/VitalsFooter';
import type { PaneLeaf } from '../../panel/paneLayout';
import { DEFAULT_VITALS_OPTIONS } from '../../ipc/uiConfig';
import { aabahranPacket } from '../../test/aabahranGmcp';

// Drives the four stores that OR in the hidden state through a fake
// Tauri event bus, with the lamented tears cases in
// fixtures/gmcp/aabahran/lament.json: each server build's packets in
// the order it sends them, then the session://hidden report the
// backend works out from them. Each test loads fresh store modules.

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();
// What hidden_get answers, the state the backend last reported.
let reported: unknown = null;

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
    if (cmd === 'ui_get_config') return {};
    if (cmd === 'affects_snapshot_get') return null;
    if (cmd === 'hidden_get') return reported;
    if (cmd === 'target_get') return { name: null, room_idx: null, quick_keys: [] };
    throw new Error(`no fake for ${cmd}`);
  },
}));

vi.mock('../session/tickSound', () => ({ playTickSound: vi.fn() }));

function fire(event: string, payload: unknown): void {
  for (const cb of handlers.get(event) ?? []) cb({ payload });
}

/** Send one packet from fixtures/gmcp/aabahran, as the backend emits it
 *  from session 1. */
function packet(name: string): void {
  const p = aabahranPacket(name);
  fire(`session://gmcp/${p.package.replace(/\./g, '-')}`, { session: 1, data: p.data });
}

const promptVars = (data: unknown) => fire('session://prompt-vars', { session: 1, data });
const hidden = (payload: unknown) => fire('session://hidden', payload);
const disconnect = () => fire('session://state', { kind: 'disconnected', reason: null });
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const NOTHING = { vitals: false, tank: false, opponent: false, affects: false, group: false };
const EVERYTHING = { vitals: true, tank: true, opponent: true, affects: true, group: true };

async function load() {
  const stores = await import('../index');
  stores.startStores();
  await settle();
  return {
    hidden: await import('./hiddenStore'),
    vitals: await import('./vitalsStore'),
    affects: await import('./affectsStore'),
    combat: await import('./combatStore'),
    group: await import('./groupStore'),
  };
}

type Stores = Awaited<ReturnType<typeof load>>;

const leaf = (pane: PaneLeaf['pane']): PaneLeaf => ({
  id: `${pane}-1`,
  pane,
  weight: 1,
  props: {},
});

/** The Vitals, Affects and Group panes drawn from the stores now. */
function panes(s: Stores): { vitals: string; affects: string; group: string } {
  return {
    vitals: renderToStaticMarkup(
      <VitalsBlock
        vitals={s.vitals.getVitals()}
        combat={s.combat.getCombat()}
        fit={{ style: 'rows' }}
        options={DEFAULT_VITALS_OPTIONS}
      />,
    ),
    affects: renderToStaticMarkup(
      <PaneLeafContext.Provider value={leaf('affects')}>
        <TimersView
          current={s.affects.getAffects()}
          tracked={[{ name: 'sanctuary', label: null }]}
          hidden={s.affects.getAffectsHidden()}
        />
      </PaneLeafContext.Provider>,
    ),
    group: renderToStaticMarkup(
      <PaneLeafContext.Provider value={leaf('group')}>
        <GroupPaneView group={s.group.getGroupState().group} />
      </PaneLeafContext.Provider>,
    ),
  };
}

/** The value text of each Vitals row, the opponent's row first while
 *  you fight. */
function vitalValues(html: string): string[] {
  return [...html.matchAll(/<span class="panel-vitals-value">([^<]*)<\/span>/g)].map((m) => m[1]);
}

beforeEach(() => {
  vi.resetModules();
  handlers.clear();
  reported = null;
  vi.stubGlobal('window', globalThis);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('hiddenStore', () => {
  it('reads session://hidden and clears on a disconnect', async () => {
    const s = await load();
    expect(s.hidden.getHidden()).toEqual(NOTHING);
    hidden({ vitals: true, tank: false, opponent: true, affects: 1, group: 'yes' });
    expect(s.hidden.getHidden()).toEqual({ ...NOTHING, vitals: true, opponent: true });
    // A report that repeats keeps the same snapshot.
    const before = s.hidden.getHidden();
    hidden({ vitals: true, opponent: true });
    expect(s.hidden.getHidden()).toBe(before);
    disconnect();
    expect(s.hidden.getHidden()).toEqual(NOTHING);
  });

  for (const c of lament.cases) {
    it(`hides every pane when ${c.name}`, async () => {
      const s = await load();
      for (const name of c.packets) packet(name);
      hidden(c.hidden);

      expect(s.vitals.getVitals()).toMatchObject({
        hp: 0,
        maxhp: 0,
        mana: 0,
        maxmana: 0,
        move: 0,
        maxmove: 0,
        hidden: true,
      });
      expect(s.affects.getAffectsHidden()).toBe(true);
      expect(s.group.getGroupState().group).toEqual({ hidden: true });
      expect(s.combat.getCombat()).toMatchObject({
        name: 'a Blackwatch guard',
        hp_pct: null,
        condition: null,
        hidden: true,
      });

      const drawn = panes(s);
      // The opponent's row, then Health, Mana and Moves.
      expect(vitalValues(drawn.vitals)).toEqual(['?', '? / ?', '? / ?', '? / ?']);
      expect(drawn.affects).toContain('The game hides your affects right now.');
      expect(drawn.affects).not.toContain('bless');
      expect(drawn.group).toContain('The game hides your group right now.');
      expect(drawn.group).not.toContain('Tester');
    });
  }

  it('reads what the backend last reported in a window that opens during the song', async () => {
    // Settings opens, or the main window reloads, on the older build
    // while the song is on. The backend reported the change before this
    // window listened, and it reports each change once.
    const older = lament.cases[2];
    reported = older.hidden;
    const s = await load();
    expect(s.hidden.getHidden()).toEqual(older.hidden);
    // The older build keeps sending the true values.
    for (const name of older.packets) packet(name);
    expect(s.vitals.getVitals()).toMatchObject({ hp: 0, maxhp: 0, hidden: true });
    const drawn = panes(s);
    expect(vitalValues(drawn.vitals)).toEqual(['?', '? / ?', '? / ?', '? / ?']);
    expect(drawn.affects).toContain('The game hides your affects right now.');
    expect(drawn.group).toContain('The game hides your group right now.');
    expect(drawn.group).not.toContain('Tester');
  });

  it('keeps a report or a disconnect that lands before the answer', async () => {
    const late = () => {
      let answer: (value: unknown) => void = () => undefined;
      reported = new Promise((resolve) => {
        answer = resolve;
      });
      return (value: unknown) => answer(value);
    };

    let answer = late();
    let s = await load();
    hidden(NOTHING);
    answer(EVERYTHING);
    await settle();
    expect(s.hidden.getHidden()).toEqual(NOTHING);

    vi.resetModules();
    handlers.clear();
    answer = late();
    s = await load();
    hidden(EVERYTHING);
    disconnect();
    answer(EVERYTHING);
    await settle();
    expect(s.hidden.getHidden()).toEqual(NOTHING);
  });

  it('shows each pane again once the song ends', async () => {
    const s = await load();
    const older = lament.cases[2];
    for (const name of older.packets) packet(name);
    hidden(older.hidden);
    expect(s.vitals.getVitals()?.hidden).toBe(true);

    // The song ends. Char.Affects comes at once, the rest at the next
    // prompt, and the backend reports nothing hidden.
    packet('char-affects.gmcp');
    hidden(NOTHING);
    packet('char-vitals.gmcp');
    packet('char-combat-lament-older.gmcp');
    packet('group-info-own-row.gmcp');

    expect(s.vitals.getVitals()).toMatchObject({ hp: 850, maxhp: 900, hidden: false });
    expect(s.affects.getAffectsHidden()).toBe(false);
    expect(s.combat.getCombat()).toMatchObject({ hp_pct: 41, hidden: false });
    const drawn = panes(s);
    expect(drawn.group).toContain('Tester');
    expect(drawn.group).toContain('64%');
    expect(drawn.affects).not.toContain('The game hides your affects');
  });

  it('changes nothing on the new build, where the flags already hide', async () => {
    const s = await load();
    for (const name of lament.cases[0].packets) packet(name);
    const vitals = s.vitals.getVitals();
    const combat = s.combat.getCombat();
    const group = s.group.getGroupState().group;
    expect(vitals?.hidden).toBe(true);
    expect(s.affects.getAffectsHidden()).toBe(true);
    expect(combat).toMatchObject({ hidden: true, tank: { name: 'Tester', hp_pct: null } });
    expect(group).toEqual({ hidden: true });

    hidden(EVERYTHING);
    expect(s.vitals.getVitals()).toBe(vitals);
    expect(s.combat.getCombat()).toBe(combat);
    expect(s.group.getGroupState().group).toBe(group);
    expect(s.affects.getAffectsHidden()).toBe(true);
  });

  it('hides the tank health and keeps the opponent shown under mirror image', async () => {
    const s = await load();
    packet('char-combat-tank.gmcp');
    expect(s.combat.getCombat()).toMatchObject({ hidden: false, tank: { hp_pct: 78 } });
    hidden({ ...NOTHING, tank: true });
    expect(s.combat.getCombat()).toMatchObject({
      hp_pct: 54,
      hidden: false,
      tank: { name: 'Tester', hp_pct: null },
    });
    hidden(NOTHING);
    expect(s.combat.getCombat()).toMatchObject({ hidden: false, tank: { hp_pct: 78 } });
  });

  it('hides the vitals before any packet arrives', async () => {
    const s = await load();
    expect(s.vitals.getVitals()).toBeNull();
    hidden({ ...NOTHING, vitals: true });
    expect(s.vitals.getVitals()).toMatchObject({ hp: 0, hidden: true });
    hidden(NOTHING);
    expect(s.vitals.getVitals()).toBeNull();
  });

  it('never fills hidden vitals from prompt vars, before the song or during it', async () => {
    const s = await load();
    packet('char-vitals.gmcp');
    promptVars({ hp: '800', maxhp: '900' });
    expect(s.vitals.getVitals()).toMatchObject({ hp: 800, hidden: false });
    hidden({ ...NOTHING, vitals: true });
    // The backend sends a hidden prompt var as ?, which never fills in.
    promptVars({ hp: '?', maxhp: '?' });
    expect(s.vitals.getVitals()).toMatchObject({ hp: 0, hidden: true });
    hidden(NOTHING);
    // Char.Vitals fills in until the prompt sets a new value, and the
    // prompt var from before the song stays out.
    expect(s.vitals.getVitals()).toMatchObject({ hp: 850, maxhp: 900, hidden: false });
    promptVars({ hp: '870', maxhp: '900' });
    expect(s.vitals.getVitals()).toMatchObject({ hp: 870, hidden: false });
  });
});
