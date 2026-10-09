import { useId, useState } from 'react';
import { normalizeAffectName } from '../../lib/affects';
import type { TrackedAffect } from '../../ipc/affects';
import { moveTrackedAffect, setTrackedAffectLabel, trackedAffectLabel } from './trackedAffectEdit';
import { Card, ChevronDownIcon, ChevronUpIcon, Disclosure, IconButton, Row } from '../../ui';
import { CommitField } from './CommitField';

// The quiet Advanced row at the end of the Characters page, the recipe
// Automation and Appearance use. It holds the one thing the Tracked
// affects chips need and the page has no place for: a label and an
// order for each affect. The login claim stays with the page's own
// rows, the login toggle and the World select, so its rules hold.

interface Props {
  tracked: TrackedAffect[];
  onTracked: (edit: (list: TrackedAffect[]) => TrackedAffect[]) => void;
}

export function ProfileAdvanced({ tracked, onTracked }: Props) {
  const [open, setOpen] = useState(false);
  const rowsId = useId();

  return (
    <Card className="st-advanced">
      <Disclosure
        label="Advanced"
        description="Set a label and an order for each tracked affect."
        expanded={open}
        aria-controls={rowsId}
        onClick={() => setOpen((v) => !v)}
      />
      <div id={rowsId} className="st-advanced-rows" hidden={!open}>
        {open && (
          <>
            <Row
              label="Affect labels and order"
              description="The Affects pane shows your label in place of the name the game sends and keeps your tracked affects in this order, each in its own slot."
            />
            {tracked.map((affect, index) => {
              const display = trackedAffectLabel(affect);
              const key = normalizeAffectName(affect.name);
              const at = (list: TrackedAffect[]) =>
                list.findIndex((e) => normalizeAffectName(e.name) === key);
              return (
                <Row key={key} label={affect.name} className="st-affect-row">
                  <CommitField
                    value={affect.label ?? ''}
                    width={140}
                    aria-label={`Label for ${affect.name}`}
                    placeholder={affect.name}
                    onCommit={(text) =>
                      onTracked((list) => setTrackedAffectLabel(list, at(list), text))
                    }
                  />
                  <IconButton
                    label={`Move ${display} up`}
                    icon={<ChevronUpIcon />}
                    disabled={index === 0}
                    onClick={() => onTracked((list) => moveTrackedAffect(list, at(list), -1))}
                  />
                  <IconButton
                    label={`Move ${display} down`}
                    icon={<ChevronDownIcon />}
                    disabled={index === tracked.length - 1}
                    onClick={() => onTracked((list) => moveTrackedAffect(list, at(list), 1))}
                  />
                </Row>
              );
            })}
          </>
        )}
      </div>
    </Card>
  );
}
