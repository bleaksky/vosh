import { createSessionStore } from '../sessionStore';

// Staff work-queue counters from the Imm.Queues GMCP package. The
// server pushes a complete snapshot to immortals whenever any queue
// changes (a note posts, a vote opens, a dcheck resolves), so every
// message replaces the whole model and nothing merges. Mortals never
// receive the package, so `received` stays false and the pane shows
// its quiet placeholder. The store keeps the last snapshot, so the
// pane can close and reopen without losing it.

export interface ImmQueues {
  /** Pending description checks. Global — the same for every
   *  immortal — and includes offline players' unresolved checks.
   *  One day deadline server-side. */
  dcheck: number;
  /** Votes awaiting this immortal's ballot. No deadline. */
  votes: number;
  /** Total real applications addressed to this imm, read and
   *  unread. Excludes description checks, which are their own
   *  top-level count. Four day deadline server-side. */
  appsOpen: number;
  /** Unread subset of appsOpen. */
  appsUnread: number;
  /** Journal entries addressed to this imm, not yet read. */
  journalsUnread: number;
  /** Journals addressed to this imm with no RP awarded yet, read or
   *  not. */
  journalsUnawarded: number;
  /** Unread penalty notes. */
  penalties: number;
  /** Unread bug reports. */
  bugs: number;
  /** Total typo reports in the queue. By server design this is a
   *  total, not an unread count. */
  typos: number;
  /** Unread ideas. */
  ideas: number;
  /** Unread notes. */
  notes: number;
  /** Per-queue counts past their server-side deadline (applications
   *  4 days, journals 3 days, dcheck 1 day). Disjoint from nearing;
   *  the top-level counts are the full totals these tier. */
  overdueApps: number;
  overdueJournals: number;
  overdueDcheck: number;
  /** Per-queue counts in the final quarter before the deadline. */
  nearingApps: number;
  nearingJournals: number;
  nearingDcheck: number;
}

export type ImmCounterKey = keyof ImmQueues;

export interface ImmState {
  queues: ImmQueues;
  /** True once any Imm.Queues push has arrived this session. */
  received: boolean;
  /** Bumped per counter each time its value increases. The pane keys
   *  each lamp on this generation so an increase remounts the lamp
   *  and restarts the strike animation; decreases stay quiet. */
  strikes: Record<ImmCounterKey, number>;
}

export const ZERO_QUEUES: ImmQueues = {
  dcheck: 0,
  overdueApps: 0,
  overdueJournals: 0,
  overdueDcheck: 0,
  nearingApps: 0,
  nearingJournals: 0,
  nearingDcheck: 0,
  votes: 0,
  appsOpen: 0,
  appsUnread: 0,
  journalsUnread: 0,
  journalsUnawarded: 0,
  penalties: 0,
  bugs: 0,
  typos: 0,
  ideas: 0,
  notes: 0,
};

const ZERO_STRIKES: Record<ImmCounterKey, number> = {
  dcheck: 0,
  overdueApps: 0,
  overdueJournals: 0,
  overdueDcheck: 0,
  nearingApps: 0,
  nearingJournals: 0,
  nearingDcheck: 0,
  votes: 0,
  appsOpen: 0,
  appsUnread: 0,
  journalsUnread: 0,
  journalsUnawarded: 0,
  penalties: 0,
  bugs: 0,
  typos: 0,
  ideas: 0,
  notes: 0,
};

function asCount(v: unknown): number {
  const n = typeof v === 'number' ? v : typeof v === 'string' ? Number(v) : 0;
  return Number.isFinite(n) && n > 0 ? Math.floor(n) : 0;
}

function normalize(data: unknown): ImmQueues {
  const d = (data && typeof data === 'object' ? data : {}) as Record<string, unknown>;
  const journals = (d.journals && typeof d.journals === 'object' ? d.journals : {}) as Record<
    string,
    unknown
  >;
  const applications = (
    d.applications && typeof d.applications === 'object' ? d.applications : {}
  ) as Record<string, unknown>;
  const overdue = (d.overdue && typeof d.overdue === 'object' ? d.overdue : {}) as Record<
    string,
    unknown
  >;
  const nearing = (d.nearing && typeof d.nearing === 'object' ? d.nearing : {}) as Record<
    string,
    unknown
  >;
  return {
    dcheck: asCount(d.dcheck),
    votes: asCount(d.votes),
    appsOpen: asCount(applications.open),
    appsUnread: asCount(applications.unread),
    journalsUnread: asCount(journals.unread),
    journalsUnawarded: asCount(journals.unawarded),
    penalties: asCount(d.penalties),
    bugs: asCount(d.bugs),
    typos: asCount(d.typos),
    ideas: asCount(d.ideas),
    notes: asCount(d.notes),
    overdueApps: asCount(overdue.applications),
    overdueJournals: asCount(overdue.journals),
    overdueDcheck: asCount(overdue.dcheck),
    nearingApps: asCount(nearing.applications),
    nearingJournals: asCount(nearing.journals),
    nearingDcheck: asCount(nearing.dcheck),
  };
}

// No `connection`, so a disconnect brings back the zero state. Stale
// duty counts are worse than none. The next login gets a fresh
// snapshot, and a mortal alt should not inherit the imm board from the
// previous character.
const store = createSessionStore<ImmState>({
  state: { queues: ZERO_QUEUES, received: false, strikes: ZERO_STRIKES },
  packages: {
    'Imm.Queues': (state, data) => {
      const queues = normalize(data);
      const strikes = { ...state.strikes };
      for (const k of Object.keys(queues) as ImmCounterKey[]) {
        if (queues[k] > state.queues[k]) strikes[k] += 1;
      }
      return { queues, received: true, strikes };
    },
  },
});

export const startImmStore = store.start;

export function getImmState(): ImmState {
  store.start();
  return store.get();
}

export function subscribeImmState(cb: (state: ImmState) => void): () => void {
  return store.subscribe(() => cb(store.get()));
}
