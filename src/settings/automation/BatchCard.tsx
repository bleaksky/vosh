import { useEffect, useState } from 'react';
import { countPhrase, type KindNoun } from '../../automation/automationDraft';
import { Button, Card, CardNote, ComboField, Row } from '../../ui';
import { shortcutLabel } from '../../lib/shortcuts';

export interface BatchCardProps {
  noun: KindNoun;
  /** How many rows are checked. */
  count: number;
  /** How many of them are on now. */
  on: number;
  /** How many of them you can turn on and off here, and how many of
   *  those are on. */
  turnable: number;
  turnableOn: number;
  /** How many of them you can delete. */
  deletable: number;
  /** The group every checked item shares, blank for none, or null when
   *  they differ. */
  shared: string | null;
  /** Every group in the four lists, for the Group field. */
  groups: readonly string[];
  /** What the card says before a move to `group` that Save would
   *  refuse, or null. */
  clashNote: (group: string) => string | null;
  onMove: (group: string) => void;
  onTurn: (on: boolean) => void;
  /** Copy each one, or null for a kind you cannot copy. */
  onDuplicate: (() => void) | null;
  onDelete: () => void;
  onDone: () => void;
}

/** The card for the rows you checked, in place of one item's card. Its
 *  changes go into the draft as an edit on each item would, so Save
 *  writes them and Discard puts them back. */
export function BatchCard({
  noun,
  count,
  on,
  turnable,
  turnableOn,
  deletable,
  shared,
  groups,
  clashNote,
  onMove,
  onTurn,
  onDuplicate,
  onDelete,
  onDone,
}: BatchCardProps) {
  const [group, setGroup] = useState(shared ?? '');
  // A move, or a change to what you checked, starts the field again
  // from the group they share.
  useEffect(() => setGroup(shared ?? ''), [shared]);

  const head = (
    <div className="st-auto-batch-head">
      <p className="st-auto-batch-title" role="status">
        {count === 0 ? `No ${noun.many} selected` : `${countPhrase(count, noun)} selected`}
      </p>
      <Button className="st-auto-quiet" onClick={onDone}>
        Done
      </Button>
    </div>
  );

  if (count === 0) {
    const all = shortcutLabel('Mod+A');
    return (
      <div className="st-auto-detail-inner">
        {head}
        <p className="st-auto-batch-hint">
          Click the {noun.many} you want to change. Shift click takes every row between, and {all}{' '}
          takes them all.
        </p>
      </div>
    );
  }

  const target = group.trim();
  const canMove = target !== (shared ?? '') && !(shared === null && target === '');
  const note = canMove ? clashNote(target) : null;
  const move = () => {
    if (canMove) onMove(target);
  };
  const off = count - on;
  const state = on === count ? 'All on' : on === 0 ? 'All off' : `${on} on, ${off} off`;
  const held = count - turnable;

  return (
    <div className="st-auto-detail-inner">
      {head}
      <Card className="st-auto-card">
        {note && <CardNote tone="warn">{note}</CardNote>}
        <Row label="Group">
          <span className="st-auto-batch-move">
            <ComboField
              width="100%"
              value={group}
              placeholder={shared === null ? 'Several groups' : 'No group'}
              options={groups}
              listLabel="Groups"
              data-group-field=""
              onChange={setGroup}
              onPick={(picked) => {
                setGroup(picked);
                if (picked !== (shared ?? '')) onMove(picked);
              }}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  move();
                }
              }}
            />
            <Button disabled={!canMove} onClick={move}>
              Move
            </Button>
          </span>
        </Row>
        <Row
          label="Enabled"
          description={
            held > 0 ? `${state}. ${countPhrase(held, noun)} from presets stay as they are.` : state
          }
        >
          <span className="st-auto-batch-turn">
            <Button disabled={turnableOn === turnable} onClick={() => onTurn(true)}>
              Turn on
            </Button>
            <Button disabled={turnableOn === 0} onClick={() => onTurn(false)}>
              Turn off
            </Button>
          </span>
        </Row>
      </Card>
      <div className="st-auto-detail-actions">
        {onDuplicate && <Button onClick={onDuplicate}>Duplicate {countPhrase(count, noun)}</Button>}
        {deletable > 0 && (
          <Button variant="danger" onClick={onDelete}>
            Delete {countPhrase(deletable, noun)}
          </Button>
        )}
      </div>
      {deletable < count && (
        <p className="st-auto-batch-hint">
          {countPhrase(count - deletable, noun)} from presets stay. Turn a preset off under Presets
          to remove its {noun.many}.
        </p>
      )}
    </div>
  );
}
