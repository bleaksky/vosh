import { Fragment, useState } from 'react';
import { profileImportApply, type ImportAddAs, type ImportResult } from '../../ipc/characters';
import type { ProfileEntry } from '../../ipc/profiles';
import {
  loginLabel,
  profileDisplayName,
  takenProfileName,
  takenSentence,
} from '../../lib/characterProfiles';
import { errorText } from '../../lib/text';
import { Button, CardNote, Field, Row, Section, Segmented, Select, Toggle } from '../../ui';
import {
  claimNote,
  importedSentence,
  importSummary,
  LUA_WARNING,
  type ImportFile,
} from './profileImport';

// The import sheet. It takes the detail column while you decide where a
// profile export goes. New profile is the default and starts from the
// name in the file name. Replace a profile swaps in a select of your
// profiles and keeps that profile's world and characters, so the world
// section leaves. In this file says what the file holds, with the Lua
// warning when a trigger or an alias runs Lua. Each character the file
// names gets the login switch: one no profile has starts on, and one
// another profile has starts off, with a note on what turning it on
// moves.

interface Props {
  file: ImportFile;
  /** Your profiles, whose names are taken and which Replace offers. */
  profiles: readonly ProfileEntry[];
  /** The profile Replace starts on when none has the file's name. */
  fallback: string;
  /** The import is done, with the line to show under the list. */
  onImported: (result: ImportResult, sentence: string) => void;
  onCancel: () => void;
  onError: (message: string | null) => void;
}

const ADD_AS = [
  { value: 'new', label: 'New profile' },
  { value: 'replace', label: 'Replace a profile' },
] as const;

export function ImportSheet({ file, profiles, fallback, onImported, onCancel, onError }: Props) {
  const { fileName, text, preview } = file;
  const names = profiles.map((p) => p.name);
  const [addAs, setAddAs] = useState<ImportAddAs>('new');
  const [name, setName] = useState(preview.name ?? '');
  const [target, setTarget] = useState(
    () => takenProfileName(names, preview.name ?? '') ?? fallback,
  );
  const [on, setOn] = useState<Readonly<Record<string, boolean>>>(() =>
    Object.fromEntries(preview.characters.map((c) => [c.name, c.claimed_by === null])),
  );
  const [busy, setBusy] = useState(false);

  const adding = addAs === 'new';
  const taken = adding ? takenProfileName(names, name) : null;
  const ready = !busy && (adding ? name.trim() !== '' && taken === null : names.includes(target));
  const summary = importSummary(preview);
  const world = adding && preview.world && preview.characters.length > 0 ? preview.world : null;

  const apply = () => {
    if (!ready) return;
    const logins = adding ? preview.characters.filter((c) => on[c.name]).map((c) => c.name) : [];
    setBusy(true);
    profileImportApply(fileName, text, addAs, adding ? name.trim() : target, logins)
      .then((result) => {
        onError(null);
        onImported(result, importedSentence(fileName, addAs, result, logins));
      })
      .catch((e: unknown) => {
        onError(errorText(e));
        setBusy(false);
      });
  };

  return (
    <>
      <Section
        title={`Import ${fileName}`}
        actions={
          <>
            <Button onClick={onCancel}>Cancel</Button>
            <Button variant="primary" disabled={!ready} onClick={apply}>
              {adding ? 'Import' : 'Replace'}
            </Button>
          </>
        }
      >
        <Row label="Add as">
          <Segmented options={ADD_AS} value={addAs} onChange={setAddAs} />
        </Row>
        {adding ? (
          <Row
            label="Name"
            description={taken === null ? undefined : takenSentence(taken)}
            descriptionTone="danger"
          >
            <Field
              width={120}
              value={name}
              invalid={taken !== null}
              placeholder="Profile name"
              onChange={setName}
              onKeyDown={(e) => {
                if (e.key !== 'Enter') return;
                e.preventDefault();
                apply();
              }}
            />
          </Row>
        ) : (
          <Row label="Profile">
            <Select
              value={target}
              options={profiles.map((p) => ({ value: p.name, label: profileDisplayName(p.name) }))}
              onChange={setTarget}
            />
          </Row>
        )}
      </Section>

      <Section title="In this file">
        {preview.runs_lua.length > 0 && <CardNote tone="warn">{LUA_WARNING}</CardNote>}
        <dl className="st-import-grid">
          {summary.map((row) => (
            <div key={row.label} className={row.wide ? 'st-import-row is-wide' : 'st-import-row'}>
              <dt className="st-import-key">{row.label}</dt>
              <dd className="st-import-value">
                {row.value.map((part, i) =>
                  typeof part === 'string' ? (
                    <Fragment key={i}>{part}</Fragment>
                  ) : (
                    <span key={i} className="st-auto-mono">
                      {part.mono}
                    </span>
                  ),
                )}
              </dd>
            </div>
          ))}
        </dl>
      </Section>

      {world && (
        <Section title={world.name}>
          {preview.characters.map((character) => (
            <Fragment key={character.name}>
              <Row label={loginLabel(character.name)}>
                <Toggle
                  checked={on[character.name] ?? false}
                  onChange={(checked) => setOn((prev) => ({ ...prev, [character.name]: checked }))}
                />
              </Row>
              {character.claimed_by !== null && (
                <CardNote tone="warn">
                  {claimNote(character.name, character.claimed_by, profiles)}
                </CardNote>
              )}
            </Fragment>
          ))}
        </Section>
      )}
    </>
  );
}
