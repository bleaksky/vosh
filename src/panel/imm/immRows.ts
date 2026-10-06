import type { ImmQueues } from '../../stores/gmcp/immStore';

// Rows for the Staff queues pane, a triage list. Only queues with work
// show, worst first: anything past its deadline, then anything in the
// last quarter before it, then the rest, and within a tier the bigger
// backlog first. The canonical order below breaks ties, so equal rows
// never trade places. Pure so the ordering is unit tested.

/** A queue's deadline tier. The worst one wins. */
export type ImmTier = 'overdue' | 'nearing' | 'none';

export interface ImmRow {
  key: keyof ImmQueues;
  label: string;
  count: number;
  tier: ImmTier;
  /** Short aside after the label, like `2 overdue` or `3 unread`. */
  note: string | null;
  /** What the count means, for the row's tooltip. */
  title: string;
}

const TIER_RANK: Record<ImmTier, number> = { overdue: 0, nearing: 1, none: 2 };

function tierOf(overdue: number, nearing: number): ImmTier {
  if (overdue > 0) return 'overdue';
  if (nearing > 0) return 'nearing';
  return 'none';
}

// The deadline tier speaks first, then the queue's own subset.
function noteOf(overdue: number, nearing: number, extra: string | null): string | null {
  if (overdue > 0) return `${overdue} overdue`;
  if (nearing > 0) return `${nearing} nearing`;
  return extra;
}

export function immRows(q: ImmQueues): ImmRow[] {
  const rows: ImmRow[] = [
    {
      key: 'dcheck',
      label: 'Description checks',
      count: q.dcheck,
      tier: tierOf(q.overdueDcheck, q.nearingDcheck),
      note: noteOf(q.overdueDcheck, q.nearingDcheck, null),
      title: 'Pending description checks, offline players included. One day deadline.',
    },
    {
      key: 'appsOpen',
      label: 'Applications',
      count: q.appsOpen,
      tier: tierOf(q.overdueApps, q.nearingApps),
      note: noteOf(
        q.overdueApps,
        q.nearingApps,
        q.appsUnread > 0 ? `${q.appsUnread} unread` : null,
      ),
      title: 'Applications in your queue, read and unread. Four day deadline.',
    },
    {
      key: 'journalsUnread',
      label: 'Journals',
      count: q.journalsUnread,
      tier: tierOf(q.overdueJournals, q.nearingJournals),
      note: noteOf(
        q.overdueJournals,
        q.nearingJournals,
        q.journalsUnawarded > 0 ? `${q.journalsUnawarded} unawarded` : null,
      ),
      title: 'Unread journals addressed to you. Three day deadline.',
    },
    plain('votes', 'Votes', q.votes, 'Votes waiting on your ballot.'),
    plain('notes', 'Notes', q.notes, 'Unread notes.'),
    plain('bugs', 'Bugs', q.bugs, 'Unread bug reports.'),
    plain('penalties', 'Penalties', q.penalties, 'Unread penalty notes.'),
    plain('ideas', 'Ideas', q.ideas, 'Unread ideas.'),
    plain('typos', 'Typos', q.typos, 'Typo reports in the queue, read and unread.'),
  ];
  // A row with deadline pressure shows even at a zero count (a journal
  // read but not awarded past its deadline), so the header can never
  // report overdue work over an empty list.
  return rows
    .filter((r) => r.count > 0 || r.tier !== 'none')
    .sort((a, b) => TIER_RANK[a.tier] - TIER_RANK[b.tier] || b.count - a.count);
}

function plain(key: keyof ImmQueues, label: string, count: number, title: string): ImmRow {
  return { key, label, count, tier: 'none', note: null, title };
}

/** Header meta: the overdue total, else the nearing total, else null. */
export function immSummary(q: ImmQueues): { text: string; tier: ImmTier } | null {
  const overdue = q.overdueApps + q.overdueJournals + q.overdueDcheck;
  if (overdue > 0) return { text: `${overdue} overdue`, tier: 'overdue' };
  const nearing = q.nearingApps + q.nearingJournals + q.nearingDcheck;
  if (nearing > 0) return { text: `${nearing} nearing`, tier: 'nearing' };
  return null;
}
