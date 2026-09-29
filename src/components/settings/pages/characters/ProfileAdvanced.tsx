import { useId, useState } from 'react';
import { normalizeAffectName } from '../../../../lib/affects';
import { sentenceCase } from '../../../../lib/affectsView';
import {
  formatCharacterNames,
  parseCharacterNames,
  parsePort,
} from '../../../../lib/characterProfiles';
import {
  profileSetMetadata,
  profileSetWorld,
  type ProfileAutoMatch,
  type ProfileEntry,
  type TrackedAffect,
} from '../../../../lib/session';
import {
  moveTrackedAffect,
  setTrackedAffectLabel,
  trackedAffectLabel,
} from '../../../../lib/trackedAffectEdit';
import { Card, ChevronDownIcon, ChevronUpIcon, Disclosure, IconButton, Row } from '../../ui';
import { CommitField } from './CommitField';

// The quiet Advanced row at the end of the Characters page, the recipe
// Automation and Appearance use. It holds what the old Profiles tab and
// the old tracked affects editor let you set and the board has no
// place for: a description, a host and port the World select does not
// list, every character name the profile claims, and a label and an
// order for each tracked affect.

interface Props {
  /** The selected profile as the index has it. */
  entry: ProfileEntry;
  tracked: TrackedAffect[];
  onTracked: (edit: (list: TrackedAffect[]) => TrackedAffect[]) => void;
  onError: (message: string | null) => void;
  /** Read the profile again after an edit. */
  onChanged: () => void;
}

/** The login claim as the index has it, with the toggle state spelled
 *  out. The backend leaves `enabled` out while it is on, and a claim
 *  sent without it turns the toggle on. */
function claimOf(entry: ProfileEntry): ProfileAutoMatch | null {
  const am = entry.auto_match;
  if (!am) return null;
  return {
    host: am.host ?? null,
    port: am.port ?? null,
    characters: am.characters ?? [],
    enabled: am.enabled !== false,
  };
}

export function ProfileAdvanced({ entry, tracked, onTracked, onError, onChanged }: Props) {
  const [open, setOpen] = useState(false);
  const rowsId = useId();
  const claim = claimOf(entry);
  const host = claim?.host ?? '';
  const port = claim?.port ?? null;

  const save = (action: () => Promise<unknown>) => {
    action()
      .then(() => onError(null))
      .catch((e: unknown) => onError(String(e)))
      .finally(onChanged);
  };

  const setDescription = (text: string) => {
    const description = text.trim();
    save(() => profileSetMetadata(entry.name, description.length > 0 ? description : null, claim));
  };

  const setHost = (text: string) => {
    const next = text.trim();
    save(() => profileSetWorld(entry.name, next.length > 0 ? next : null, port));
  };

  const setPort = (text: string) => {
    const next = parsePort(text);
    if (next === undefined) {
      onError('Enter a port from 1 to 65535.');
      onChanged();
      return;
    }
    save(() => profileSetWorld(entry.name, host.length > 0 ? host : null, next));
  };

  const setCharacters = (text: string) => {
    const characters = parseCharacterNames(text);
    // A profile with no world and no names needs no claim at all. A
    // new claim with no world stays off so it never matches.
    const next: ProfileAutoMatch | null =
      claim === null
        ? characters.length > 0
          ? { host: null, port: null, characters, enabled: false }
          : null
        : { ...claim, characters };
    save(() => profileSetMetadata(entry.name, entry.description ?? null, next));
  };

  return (
    <Card className="st-advanced">
      <Disclosure
        label="Advanced"
        description="Set a description, a custom host, character names, and affect labels."
        expanded={open}
        aria-controls={rowsId}
        onClick={() => setOpen((v) => !v)}
      />
      <div id={rowsId} className="st-advanced-rows" hidden={!open}>
        {open && (
          <>
            <Row label="Description">
              <CommitField
                value={entry.description ?? ''}
                placeholder="Optional"
                onCommit={setDescription}
              />
            </Row>
            <Row label="Host" description="Use a host the World list does not have.">
              <CommitField value={host} mono placeholder="Host name" onCommit={setHost} />
            </Row>
            <Row label="Port">
              <CommitField
                value={port === null ? '' : String(port)}
                mono
                width={80}
                inputMode="numeric"
                placeholder="Any"
                onCommit={setPort}
              />
            </Row>
            <Row
              label="Character names"
              description="The login toggle uses the first name. Separate names with commas."
            >
              <CommitField
                value={formatCharacterNames(claim?.characters)}
                placeholder="None yet"
                onCommit={setCharacters}
              />
            </Row>
            {tracked.length > 0 && (
              <>
                <Row
                  label="Affect labels and order"
                  description="The Affects pane shows your label in place of the server name and lists missing affects in this order."
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
                        placeholder={sentenceCase(affect.name)}
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
          </>
        )}
      </div>
    </Card>
  );
}
