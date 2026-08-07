import { useEffect, useState } from 'react';
import { getImmState, subscribeImmState, type ImmCounterKey, type ImmState } from '../lib/immStore';

// Staff duty board, driven by the Imm.Queues GMCP snapshot. Reads as
// an annunciator strip: every queue is a slot with a lamp that sits
// hollow while the queue is clear and lights when work waits. A
// counter increase remounts its lamp (keyed on the store's strike
// generation) so the lamp fires one bright strike, then settles to
// lit. Decreases stay quiet — clearing your queues dims the board,
// which is the reward. Any lamp mount replays the strike, so the
// first snapshot after login flashes every lit lamp at once (the
// classic power-on lamp test) and reopening the pane repeats it.
//
// Two tiers, matching the server's own semantics. The duty strip is
// "someone is waiting on you" (dcheck globally, open applications,
// pending votes). The spool grid is unread mail-style counters. The
// server's deliberate asymmetries surface as small chips instead of
// being flattened away: applications shows total open with a "new"
// chip for the unread subset, journals carries an "rp due" chip for
// unawarded entries, and typos is marked as a queue total.
export function ImmPane() {
  const [state, setState] = useState<ImmState>(() => getImmState());
  useEffect(() => subscribeImmState(setState), []);

  const q = state.queues;
  // appsUnread is a strict subset of appsOpen. journalsUnawarded
  // OVERLAPS journalsUnread (a journal can be read but unawarded),
  // so adding it would double count the overlap; the rp-due chip
  // carries it instead. That means the header can read "clear"
  // while an rp-due chip still shows, which is intentional.
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

  return (
    <div className="imm-pane">
      <div className="chat-pane-header">
        <span className="chat-pane-title">imm</span>
        {state.received && (
          <span className={`chat-pane-count${total > 0 ? ' imm-total-hot' : ''}`}>
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

function Board({ state }: { state: ImmState }) {
  const q = state.queues;
  return (
    <>
      <div className="imm-duty" role="list">
        <DutySlot
          k="dcheck"
          count={q.dcheck}
          label="dcheck"
          danger
          state={state}
          title="players waiting in the description approval queue (global)"
        />
        <DutySlot
          k="appsOpen"
          count={q.appsOpen}
          label="applications"
          chip={q.appsUnread > 0 ? `${q.appsUnread} unread` : undefined}
          state={state}
          title="applications in your queue, read and unread"
        />
        <DutySlot
          k="votes"
          count={q.votes}
          label="votes"
          state={state}
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
          chip={q.journalsUnawarded > 0 ? `${q.journalsUnawarded} unawarded` : undefined}
          state={state}
          title="unread journals addressed to you"
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
          qualifier="queue"
          state={state}
          title="typo reports in the queue, read and unread"
        />
      </div>
    </>
  );
}

function Lamp({
  k,
  lit,
  danger,
  duty,
  state,
}: {
  k: ImmCounterKey;
  lit: boolean;
  danger?: boolean | undefined;
  duty?: boolean | undefined;
  state: ImmState;
}) {
  const cls = ['imm-lamp', lit ? 'is-lit' : '', duty ? 'is-duty' : '', danger ? 'is-danger' : '']
    .filter(Boolean)
    .join(' ');
  // Keying on the strike generation remounts the lamp when the count
  // increases, restarting the strike animation.
  return <span key={`${k}-${state.strikes[k]}`} className={cls} aria-hidden="true" />;
}

function DutySlot({
  k,
  count,
  label,
  chip,
  danger,
  state,
  title,
}: {
  k: ImmCounterKey;
  count: number;
  label: string;
  chip?: string | undefined;
  danger?: boolean | undefined;
  state: ImmState;
  title: string;
}) {
  const lit = count > 0;
  return (
    <div className={`imm-duty-slot${lit ? ' is-lit' : ''}`} role="listitem" title={title}>
      <Lamp k={k} lit={lit} duty danger={danger} state={state} />
      <span className="imm-duty-count">{count}</span>
      <span className="imm-duty-label">{label}</span>
      {chip && <span className="imm-chip imm-chip-new">{chip}</span>}
    </div>
  );
}

function Slot({
  k,
  count,
  label,
  chip,
  qualifier,
  state,
  title,
}: {
  k: ImmCounterKey;
  count: number;
  label: string;
  chip?: string | undefined;
  qualifier?: string | undefined;
  state: ImmState;
  title: string;
}) {
  const lit = count > 0;
  return (
    <div className={`imm-slot${lit ? ' is-lit' : ''}`} role="listitem" title={title}>
      <Lamp k={k} lit={lit} state={state} />
      <span className="imm-slot-count">{count}</span>
      <span className="imm-slot-label">
        {label}
        {qualifier && <span className="imm-qualifier"> {qualifier}</span>}
        {chip && <span className="imm-chip-inline"> {chip}</span>}
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
