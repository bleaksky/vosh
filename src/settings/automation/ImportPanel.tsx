import { useId, useRef, useState } from 'react';
import { importErrorMessage } from '../../automation/automationRecords';
import { presetTriggerNames } from '../../automation/presets';
import {
  applyImport,
  detectImportFormat,
  type ImportFormat,
  type ImportSummary,
} from '../../ipc/automation';
import { getShownProfile } from '../shownProfile';
import { MigrationWizard } from './MigrationWizard';
import { Button, CardNote, Disclosure, Row, Section, Select } from '../../ui';

// Import from another client, the old Import tab's logic in the new
// frame. Pick a file or paste its contents, pick the format or let
// Vosh detect it, and Import merges what Vosh can read into your
// aliases, triggers, macros, and variables. The summary lists what it
// could not bring over. The shared catalog preview opens the migration
// wizard, as before.

const FORMATS: readonly { value: ImportFormat; label: string; hint: string }[] = [
  { value: '', label: 'Detect automatically', hint: 'Vosh reads the file and picks the format.' },
  { value: 'mushclient', label: 'MUSHclient', hint: 'World files that end in .mcl or .xml.' },
  { value: 'mudlet', label: 'Mudlet', hint: 'Package exports that end in .xml.' },
  { value: 'gmud', label: 'GMUD', hint: 'Plain text files like gmud.cfg.' },
  {
    value: 'cmud',
    label: 'CMUD or zMUD',
    hint: 'XML exports. Vosh flattens classes and translates wildcards.',
  },
];

const isFormat = (value: string | null): value is ImportFormat =>
  value !== null && FORMATS.some((f) => f.value === value);

interface ImportPanelProps {
  onError: (message: string | null) => void;
}

export function ImportPanel({ onError }: ImportPanelProps) {
  const [text, setText] = useState('');
  const [fileName, setFileName] = useState<string | null>(null);
  const [format, setFormat] = useState<ImportFormat>('');
  const [busy, setBusy] = useState(false);
  const [summary, setSummary] = useState<ImportSummary | null>(null);
  const [wizard, setWizard] = useState(false);
  const fileRef = useRef<HTMLInputElement | null>(null);
  const pasteId = useId();

  const pick = async (file: File) => {
    setSummary(null);
    try {
      const body = await file.text();
      setText(body);
      setFileName(file.name);
      const detected = await detectImportFormat(body);
      if (isFormat(detected)) setFormat(detected);
      onError(null);
    } catch (e) {
      onError(importErrorMessage(e, 'read'));
    }
  };

  const run = async () => {
    if (!text.trim()) return;
    setBusy(true);
    setSummary(null);
    try {
      setSummary(await applyImport(format, text, presetTriggerNames(), getShownProfile()));
      onError(null);
    } catch (e) {
      onError(importErrorMessage(e, 'import'));
    } finally {
      setBusy(false);
    }
  };

  const clear = () => {
    setText('');
    setFileName(null);
    setFormat('');
    setSummary(null);
    if (fileRef.current) fileRef.current.value = '';
  };

  const hint = FORMATS.find((f) => f.value === format)?.hint;

  return (
    <div className="st-auto-import">
      <Section
        id="import"
        title="Import from another client"
        actions={
          <>
            <Button onClick={clear} disabled={busy || (!text && !summary)}>
              Clear
            </Button>
            <Button variant="primary" onClick={() => void run()} disabled={busy || !text.trim()}>
              {busy ? 'Importing…' : 'Import'}
            </Button>
          </>
        }
      >
        <CardNote>
          Vosh adds the aliases, triggers, macros, and variables it can read and replaces any with
          the same name. The summary lists what it could not bring over.
        </CardNote>
        <Row label="File" description={fileName ?? 'Or paste the contents below.'}>
          <Button onClick={() => fileRef.current?.click()} disabled={busy}>
            Choose file…
          </Button>
          <input
            ref={fileRef}
            type="file"
            accept=".xml,.mcl,.cfg,.txt,.tin"
            className="st-visually-hidden"
            tabIndex={-1}
            aria-hidden="true"
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (file) void pick(file);
            }}
          />
        </Row>
        <Row label="Format" description={hint}>
          <Select
            width={240}
            value={format}
            options={FORMATS}
            onChange={(value) => {
              if (isFormat(value)) setFormat(value);
            }}
          />
        </Row>
        <div className="st-row st-auto-block">
          <label htmlFor={pasteId} className="st-row-label">
            Contents
          </label>
          <textarea
            id={pasteId}
            className="st-field st-field-mono st-auto-paste"
            spellCheck={false}
            placeholder="Paste a file here"
            value={text}
            onChange={(e) => {
              setText(e.target.value);
              setFileName(null);
            }}
          />
        </div>
      </Section>

      {summary && <ImportResult summary={summary} />}

      <Section title="Shared catalog">
        <Row
          label="Preview a shared catalog"
          description="See how your profiles would merge into one catalog with a loadout for each. Nothing changes until you apply it."
        >
          <Button onClick={() => setWizard(true)}>Preview…</Button>
        </Row>
      </Section>
      {wizard && (
        <div className="settings-app" data-interim="">
          <MigrationWizard onClose={() => setWizard(false)} />
        </div>
      )}
    </div>
  );
}

function ImportResult({ summary }: { summary: ImportSummary }) {
  const total = summary.aliases + summary.triggers + summary.macros + summary.vars;
  const lists: { label: string; items: string[] }[] = [
    { label: 'Rejected', items: summary.rejected },
    { label: 'Not supported', items: summary.unsupported.map(([kind, what]) => `${kind} ${what}`) },
    { label: 'Lines Vosh could not read', items: summary.unparsed },
  ].filter((l) => l.items.length > 0);
  return (
    <Section title={total === 1 ? 'Vosh imported 1 item' : `Vosh imported ${total} items`}>
      <Row label="Aliases">
        <span className="st-auto-value">{summary.aliases}</span>
      </Row>
      <Row label="Triggers">
        <span className="st-auto-value">{summary.triggers}</span>
      </Row>
      <Row label="Macros">
        <span className="st-auto-value">{summary.macros}</span>
      </Row>
      <Row label="Variables">
        <span className="st-auto-value">{summary.vars}</span>
      </Row>
      {lists.map((list) => (
        <ImportList key={list.label} label={list.label} items={list.items} />
      ))}
    </Section>
  );
}

function ImportList({ label, items }: { label: string; items: string[] }) {
  const [open, setOpen] = useState(false);
  const listId = useId();
  return (
    <>
      <Disclosure
        label={label}
        description={items.length === 1 ? '1 item' : `${items.length} items`}
        expanded={open}
        aria-controls={listId}
        onClick={() => setOpen((o) => !o)}
      />
      {open && (
        <ul id={listId} className="st-auto-importlist">
          {items.map((item, i) => (
            <li key={i} className="st-auto-mono">
              {item}
            </li>
          ))}
        </ul>
      )}
    </>
  );
}
