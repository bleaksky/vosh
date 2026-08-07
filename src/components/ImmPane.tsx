import { useEffect, useState } from 'react';
import { getImmState, subscribeImmState, type ImmCounterKey, type ImmState } from '../lib/immStore';

// Staff duty board, driven by the Imm.Queues GMCP snapshot. Reads as
// an annunciator strip: every queue is a slot with a lamp that sits
// hollow while the queue is clear and lights when work waits. A
// counter increase remounts its lamp (keyed on the store's strike
// generations) so the lamp fires one bright strike, then settles to
// lit. Decreases stay quiet — clearing your queues dims the board,
// which is the reward. Any lamp mount replays the strike, so the
// first snapshot after login flashes every lit lamp at once (the
// classic power-on lamp test) and reopening the pane repeats it.
//
// Lamps on the deadline-tracked queues (dcheck, applications,
// journals) are a three-step traffic signal driven by the server's
// overdue / nearing tiers: red and breathing when anything is past
// its deadline, amber when anything sits in the final quarter before
// it, plain lit when work waits but is on schedule. The server's
// deliberate count asymmetries surface as small chips instead of
// being flattened away: applications shows total open with an unread
// chip, journals carries an unawarded chip, and overdue / nearing
// counts ride as chips on their queue's row.
export function ImmPane() {
  const [state, setState] = useState<ImmState>(() => getImmState());
  useEffect(() => subscribeImmState(setState), []);

  const q = state.queues;
  // appsUnread is a strict subset of appsOpen. journalsUnawarded
  // OVERLAPS journalsUnread (a journal can be read but unawarded),
  // so adding it would double count the overlap; the unawarded chip
  // carries it instead. Overdue / nearing tier existing totals and
  // never add to them. That means the header can read "clear" while
  // an unawarded chip still shows, which is intentional.
  const total =
    q.dcheck +
    q.votes +
    q.appsOpen +
    q.journalsUnread +
    q.penalties +
    q.bugs +
    q.typos +
    q.ideas +
    q.notes;
  const anyOverdue = q.overdueApps + q.overdueJournals + q.overdueDcheck > 0;

  return (
    <div className="imm-pane">
      <div className="chat-pane-header">
        <span className="chat-pane-title">imm</span>
        {state.received && (
          <span
            className={`chat-pane-count${
              anyOverdue ? ' imm-total-overdue' : total > 0 ? ' imm-total-hot' : ''
            }`}
          >
            {total > 0 ? total : 'clear'}
          </span>
        )}
      </div>
      <div className="chat-pane-body imm-body">
        {state.received ? <Board state={state} /> : <QuietNote />}
      </div>
    </div>
  );
}

// Deadline tier for a queue's lamp: worst state wins.
type Tier = 'overdue' | 'nearing' | 'none';

function tierOf(overdue: number, nearing: number): Tier {
  if (overdue > 0) return 'overdue';
  if (nearing > 0) return 'nearing';
  return 'none';
}

// Chips for a deadline-tracked queue, ordered worst first.
function tierChips(overdue: number, nearing: number): Array<{ text: string; kind: string }> {
  const chips: Array<{ text: string; kind: string }> = [];
  if (overdue > 0) chips.push({ text: `${overdue} overdue`, kind: 'overdue' });
  if (nearing > 0) chips.push({ text: `${nearing} nearing`, kind: 'nearing' });
  return chips;
}

function Board({ state }: { state: ImmState }) {
  const q = state.queues;
  return (
    <>
      <div className="imm-duty" role="list">
        <DutySlot
          k="dcheck"
          count={q.dcheck}
          label="dcheck"
          tier={tierOf(q.overdueDcheck, q.nearingDcheck)}
          chips={tierChips(q.overdueDcheck, q.nearingDcheck)}
          strikeToken={`${state.strikes.dcheck}-${state.strikes.overdueDcheck}`}
          title="pending description checks, including offline players. one day deadline"
        />
        <DutySlot
          k="appsOpen"
          count={q.appsOpen}
          label="applications"
          tier={tierOf(q.overdueApps, q.nearingApps)}
          chips={[
            ...tierChips(q.overdueApps, q.nearingApps),
            ...(q.appsUnread > 0 ? [{ text: `${q.appsUnread} unread`, kind: 'new' }] : []),
          ]}
          strikeToken={`${state.strikes.appsOpen}-${state.strikes.overdueApps}`}
          title="applications in your queue, read and unread. four day deadline"
        />
        <DutySlot
          k="votes"
          count={q.votes}
          label="votes"
          tier="none"
          chips={[]}
          strikeToken={`${state.strikes.votes}`}
          title="votes awaiting your ballot"
        />
      </div>
      <div className="imm-grid" role="list">
        <Slot k="notes" count={q.notes} label="notes" state={state} title="unread notes" />
        <Slot k="bugs" count={q.bugs} label="bugs" state={state} title="unread bug reports" />
        <Slot
          k="journalsUnread"
          count={q.journalsUnread}
          label="journals"
          tier={tierOf(q.overdueJournals, q.nearingJournals)}
          chips={[
            ...tierChips(q.overdueJournals, q.nearingJournals),
            ...(q.journalsUnawarded > 0
              ? [{ text: `${q.journalsUnawarded} unawarded`, kind: 'new' }]
              : []),
          ]}
          strikeToken={`${state.strikes.journalsUnread}-${state.strikes.overdueJournals}`}
          state={state}
          title="unread journals addressed to you. three day deadline"
        />
        <Slot k="ideas" count={q.ideas} label="ideas" state={state} title="unread ideas" />
        <Slot
          k="penalties"
          count={q.penalties}
          label="penalties"
          state={state}
          title="unread penalty notes"
        />
        <Slot
          k="typos"
          count={q.typos}
          label="typos"
          state={state}
          title="typo reports in the queue, read and unread"
        />
      </div>
    </>
  );
}

function Lamp({ lit, tier, strikeToken }: { lit: boolean; tier: Tier; strikeToken: string }) {
  const cls = [
    'imm-lamp',
    lit ? 'is-lit' : '',
    lit && tier === 'nearing' ? 'is-nearing' : '',
    lit && tier === 'overdue' ? 'is-overdue' : '',
  ]
    .filter(Boolean)
    .join(' ');
  // Keying on the strike token remounts the lamp when the count (or
  // its overdue tier) increases, restarting the strike animation.
  return <span key={strikeToken} className={cls} aria-hidden="true" />;
}

function DutySlot({
  k,
  count,
  label,
  tier,
  chips,
  strikeToken,
  title,
}: {
  k: ImmCounterKey;
  count: number;
  label: string;
  tier: Tier;
  chips: Array<{ text: string; kind: string }>;
  strikeToken: string;
  title: string;
}) {
  const lit = count > 0;
  return (
    <div className={`imm-duty-slot${lit ? ' is-lit' : ''}`} role="listitem" title={title}>
      <Lamp lit={lit} tier={tier} strikeToken={`${k}-${strikeToken}`} />
      <span className="imm-duty-count">{count}</span>
      <span className="imm-duty-label">{label}</span>
      <span className="imm-chip-row">
        {chips.map((c) => (
          <span key={c.kind} className={`imm-chip imm-chip-${c.kind}`}>
            {c.text}
          </span>
        ))}
      </span>
    </div>
  );
}

function Slot({
  k,
  count,
  label,
  tier,
  chips,
  strikeToken,
  state,
  title,
}: {
  k: ImmCounterKey;
  count: number;
  label: string;
  tier?: Tier | undefined;
  chips?: Array<{ text: string; kind: string }> | undefined;
  strikeToken?: string | undefined;
  state: ImmState;
  title: string;
}) {
  const lit = count > 0;
  return (
    <div className={`imm-slot${lit ? ' is-lit' : ''}`} role="listitem" title={title}>
      <Lamp
        lit={lit}
        tier={tier ?? 'none'}
        strikeToken={`${k}-${strikeToken ?? state.strikes[k]}`}
      />
      <span className="imm-slot-count">{count}</span>
      <span className="imm-slot-label">
        {label}
        {(chips ?? []).map((c) => (
          <span key={c.kind} className={`imm-chip-inline imm-chip-${c.kind}`}>
            {' '}
            {c.text}
          </span>
        ))}
      </span>
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
