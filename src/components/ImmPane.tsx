import { useEffect, useState } from 'react';
import { getImmState, subscribeImmState, type ImmCounterKey, type ImmState } from '../lib/immStore';

// Staff duty board, driven by the Imm.Queues GMCP snapshot. It is a
// triage list: only queues that actually have work show up, sorted
// worst-first, so a quiet board is nearly empty and a glance lands on
// whatever is on fire. The count carries the urgency color — red when
// anything in that queue is past its deadline, amber when anything is
// in the final quarter before it, plain otherwise. A fresh increase
// flashes the count (the number remounts on a strike-generation bump),
// so new work announces itself without the board ever shouting.
//
// The server's count asymmetries ride as small trailing chips instead
// of being flattened: overdue and nearing tiers, plus applications'
// unread subset and journals' unawarded parallel count.

// A queue's deadline tier. Worst wins; drives the count color.
type Tier = 'overdue' | 'nearing' | 'none';

interface Chip {
  text: string;
  kind: 'overdue' | 'nearing' | 'sub';
}

interface QueueRow {
  /** Store key for the count, also the strike-generation lookup. */
  key: ImmCounterKey;
  label: string;
  count: number;
  tier: Tier;
  chips: Chip[];
  /** Second strike key so a new overdue flashes the count even when
   *  the base count did not move. */
  overdueKey?: ImmCounterKey;
  title: string;
}

function tierOf(overdue: number, nearing: number): Tier {
  if (overdue > 0) return 'overdue';
  if (nearing > 0) return 'nearing';
  return 'none';
}

function deadlineChips(overdue: number, nearing: number): Chip[] {
  const chips: Chip[] = [];
  if (overdue > 0) chips.push({ text: `${overdue} overdue`, kind: 'overdue' });
  if (nearing > 0) chips.push({ text: `${nearing} nearing`, kind: 'nearing' });
  return chips;
}

// Every queue the board can show, in canonical order (the stable
// tiebreak when two rows share a tier and count).
function buildRows(q: ImmState['queues']): QueueRow[] {
  return [
    {
      key: 'dcheck',
      label: 'dcheck',
      count: q.dcheck,
      tier: tierOf(q.overdueDcheck, q.nearingDcheck),
      chips: deadlineChips(q.overdueDcheck, q.nearingDcheck),
      overdueKey: 'overdueDcheck',
      title: 'pending description checks, including offline players. one day deadline',
    },
    {
      key: 'appsOpen',
      label: 'applications',
      count: q.appsOpen,
      tier: tierOf(q.overdueApps, q.nearingApps),
      chips: [
        ...deadlineChips(q.overdueApps, q.nearingApps),
        ...(q.appsUnread > 0 ? [{ text: `${q.appsUnread} unread`, kind: 'sub' as const }] : []),
      ],
      overdueKey: 'overdueApps',
      title: 'applications in your queue, read and unread. four day deadline',
    },
    {
      key: 'journalsUnread',
      label: 'journals',
      count: q.journalsUnread,
      tier: tierOf(q.overdueJournals, q.nearingJournals),
      chips: [
        ...deadlineChips(q.overdueJournals, q.nearingJournals),
        ...(q.journalsUnawarded > 0
          ? [{ text: `${q.journalsUnawarded} unawarded`, kind: 'sub' as const }]
          : []),
      ],
      overdueKey: 'overdueJournals',
      title: 'unread journals addressed to you. three day deadline',
    },
    {
      key: 'votes',
      label: 'votes',
      count: q.votes,
      tier: 'none',
      chips: [],
      title: 'votes awaiting your ballot',
    },
    {
      key: 'notes',
      label: 'notes',
      count: q.notes,
      tier: 'none',
      chips: [],
      title: 'unread notes',
    },
    {
      key: 'bugs',
      label: 'bugs',
      count: q.bugs,
      tier: 'none',
      chips: [],
      title: 'unread bug reports',
    },
    {
      key: 'penalties',
      label: 'penalties',
      count: q.penalties,
      tier: 'none',
      chips: [],
      title: 'unread penalty notes',
    },
    {
      key: 'ideas',
      label: 'ideas',
      count: q.ideas,
      tier: 'none',
      chips: [],
      title: 'unread ideas',
    },
    {
      key: 'typos',
      label: 'typos',
      count: q.typos,
      tier: 'none',
      chips: [],
      title: 'typo reports in the queue, read and unread',
    },
  ];
}

const TIER_RANK: Record<Tier, number> = { overdue: 0, nearing: 1, none: 2 };

export function ImmPane() {
  const [state, setState] = useState<ImmState>(() => getImmState());
  useEffect(() => subscribeImmState(setState), []);

  const q = state.queues;
  const overdueTotal = q.overdueApps + q.overdueJournals + q.overdueDcheck;
  const nearingTotal = q.nearingApps + q.nearingJournals + q.nearingDcheck;

  // Only queues that need something, worst-first: overdue tier, then
  // nearing, then plain pending, and within a tier the bigger backlog
  // first. A row shows if it has work OR carries deadline pressure —
  // the second clause keeps an overdue/nearing item visible even if
  // its base count reads zero (a journal read but not yet awarded past
  // its deadline), so the header can never say "N overdue" over an
  // empty board. The canonical order in buildRows is the final
  // tiebreak (stable sort), so equal rows never jitter.
  const rows = buildRows(q)
    .filter((r) => r.count > 0 || r.tier !== 'none')
    .sort((a, b) => TIER_RANK[a.tier] - TIER_RANK[b.tier] || b.count - a.count);

  let statusText = '';
  let statusClass = '';
  if (overdueTotal > 0) {
    statusText = `${overdueTotal} overdue`;
    statusClass = ' imm-total-overdue';
  } else if (nearingTotal > 0) {
    statusText = `${nearingTotal} nearing`;
    statusClass = ' imm-total-nearing';
  } else if (state.received && rows.length === 0) {
    statusText = 'clear';
    statusClass = ' imm-total-clear';
  }

  return (
    <div className="imm-pane">
      <div className="chat-pane-header">
        <span className="chat-pane-title">imm</span>
        {state.received && statusText && (
          <span className={`chat-pane-count${statusClass}`}>{statusText}</span>
        )}
      </div>
      <div className="chat-pane-body imm-body">
        {!state.received ? (
          <QuietNote />
        ) : rows.length === 0 ? (
          <AllClear />
        ) : (
          <div className="imm-list" role="list">
            {rows.map((r) => (
              <Row key={r.key} row={r} state={state} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function Row({ row, state }: { row: QueueRow; state: ImmState }) {
  const overdueGen = row.overdueKey ? state.strikes[row.overdueKey] : 0;
  // Remount the count on any increase (base count or a fresh overdue)
  // so the strike animation replays; a decrease keeps the same token
  // and stays quiet.
  const strikeToken = `${state.strikes[row.key]}-${overdueGen}`;
  return (
    <div className={`imm-row imm-row-${row.tier}`} role="listitem" title={row.title}>
      <span key={strikeToken} className="imm-row-count">
        {row.count}
      </span>
      <span className="imm-row-label">{row.label}</span>
      <span className="imm-row-chips">
        {row.chips.map((c) => (
          <span key={c.kind} className={`imm-chip imm-chip-${c.kind}`}>
            {c.text}
          </span>
        ))}
      </span>
    </div>
  );
}

function AllClear() {
  return (
    <div className="imm-quiet">
      <span className="imm-quiet-line">all clear</span>
      <span className="imm-quiet-sub">no staff queues need you right now</span>
    </div>
  );
}

function QuietNote() {
  return (
    <div className="imm-quiet">
      <span className="imm-quiet-line">no staff feed</span>
      <span className="imm-quiet-sub">the server lights this panel for immortals</span>
    </div>
  );
}
