import { useState } from 'react';
import {
  applyImport,
  detectImportFormat,
  type ImportFormat,
  type ImportSummary,
} from '../lib/session';
import { MigrationWizard } from './MigrationWizard';

interface Props {
  onError: (e: string | null) => void;
}

const FORMATS: { id: Exclude<ImportFormat, ''>; label: string; hint: string }[] = [
  {
    id: 'mushclient',
    label: 'MUSHclient',
    hint: '.mcl / .xml world files. Rooted at <muclient>.',
  },
  {
    id: 'mudlet',
    label: 'Mudlet',
    hint: '.xml package exports. Rooted at <MudletPackage>.',
  },
  {
    id: 'gmud',
    label: 'GMUD',
    hint: 'gmud.cfg-style plain text directives.',
  },
  {
    id: 'cmud',
    label: 'CMUD / zMUD',
    hint: '.xml export rooted at <cmud>. Classes flattened, wildcards translated.',
  },
];

// Import config files from other MUD clients into vosh. Drop a file
// in (or paste its contents), pick the format (or let auto-detect),
// and hit apply. Backend parses + merges into the live profile and
// returns a summary of what landed and what was skipped.
export function ImportTab({ onError }: Props) {
  const [text, setText] = useState('');
  const [format, setFormat] = useState<ImportFormat>('');
  const [busy, setBusy] = useState(false);
  const [summary, setSummary] = useState<ImportSummary | null>(null);
  // Scope-migration preview modal (moved here from the Profiles tab;
  // import is the tools tab, and the wizard is an import-shaped
  // operation even though it reads the profile catalog).
  const [showMigrationWizard, setShowMigrationWizard] = useState(false);

  const handleFile = async (file: File) => {
    setSummary(null);
    try {
      const body = await file.text();
      setText(body);
      // Auto-pick the format off the loaded contents so the user
      // does not have to choose first.
      const detected = await detectImportFormat(body);
      if (detected) setFormat(detected as ImportFormat);
    } catch (e) {
      onError(String(e));
    }
  };

  const handleApply = async () => {
    if (!text.trim()) {
      onError('paste a config or pick a file first');
      return;
    }
    setBusy(true);
    setSummary(null);
    try {
      const result = await applyImport(format, text);
      setSummary(result);
      onError(null);
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="import-tab">
      <div className="settings-tab-head">
        <div className="settings-pane-title">import</div>
        <span className="settings-tab-head-spacer" />
      </div>

      <div className="import-help">
        Import aliases, triggers, keyboard macros, and variables from another MUD client. Drop a
        file in or paste its contents below, pick a format (or leave on auto-detect), and hit apply.
        Existing entries with the same name get overwritten; anything that cannot be modeled is
        listed in the summary so you can port it by hand.
      </div>

      <div className="settings-sect settings-sect-first">
        <span className="settings-section-label">format</span>
        <div className="settings-fctrl">
          <button
            type="button"
            className={`opt-chip${format === '' ? ' is-on' : ''}`}
            aria-pressed={format === ''}
            onClick={() => setFormat('')}
          >
            auto-detect
          </button>
          {FORMATS.map((f) => (
            <button
              key={f.id}
              type="button"
              className={`opt-chip${format === f.id ? ' is-on' : ''}`}
              aria-pressed={format === f.id}
              title={f.hint}
              onClick={() => setFormat(f.id)}
            >
              {f.label}
            </button>
          ))}
        </div>
        <span className="settings-fhelp" style={{ gridColumn: 'auto' }}>
          {format === ''
            ? 'auto-detect reads the pasted contents and picks the format for you'
            : FORMATS.find((f) => f.id === format)?.hint}
        </span>
      </div>

      <div className="settings-sect">
        <span className="settings-section-label">source</span>
        <div className="import-source-row">
          <label className="settings-btn">
            pick file
            <input
              type="file"
              accept=".xml,.mcl,.cfg,.txt,.tin"
              style={{ display: 'none' }}
              onChange={(e) => {
                const file = e.target.files?.[0];
                if (file) void handleFile(file);
              }}
            />
          </label>
          <button
            type="button"
            className="settings-btn"
            onClick={() => void handleApply()}
            disabled={busy || !text.trim()}
          >
            {busy ? 'applying...' : 'apply'}
          </button>
          <button
            type="button"
            className="settings-btn settings-btn-mute"
            onClick={() => {
              setText('');
              setFormat('');
              setSummary(null);
            }}
            disabled={busy}
          >
            clear
          </button>
        </div>

        <textarea
          className="import-textarea"
          spellCheck={false}
          placeholder="paste config contents here (or use pick file)"
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
      </div>

      {summary && (
        <div className="settings-sect">
          <span className="settings-section-label">summary</span>
          <ImportSummaryView summary={summary} />
        </div>
      )}

      <div className="settings-sect">
        <span className="settings-section-label">migrate between scopes</span>
        <span className="settings-fhelp" style={{ gridColumn: 'auto' }}>
          see how your profiles would merge into a single shared catalog with one loadout per
          profile. read-only, nothing is written.
        </span>
        <div className="import-source-row">
          <button
            type="button"
            className="settings-btn"
            onClick={() => setShowMigrationWizard(true)}
          >
            preview migration
          </button>
        </div>
      </div>
      {showMigrationWizard && <MigrationWizard onClose={() => setShowMigrationWizard(false)} />}
    </div>
  );
}

function ImportSummaryView({ summary }: { summary: ImportSummary }) {
  const total = summary.aliases + summary.triggers + summary.macros + summary.vars;
  return (
    <div className="import-summary">
      <div className="import-summary-headline">
        imported {total} item{total === 1 ? '' : 's'}
      </div>
      <div className="import-summary-counts">
        <span>aliases: {summary.aliases}</span>
        <span>triggers: {summary.triggers}</span>
        <span>macros: {summary.macros}</span>
        <span>vars: {summary.vars}</span>
      </div>
      {summary.rejected.length > 0 && <ImportSection label="rejected" items={summary.rejected} />}
      {summary.unsupported.length > 0 && (
        <ImportSection
          label="unsupported"
          items={summary.unsupported.map((u) => `${u[0]}: ${u[1]}`)}
        />
      )}
      {summary.unparsed.length > 0 && (
        <ImportSection label="unparsed lines" items={summary.unparsed} />
      )}
    </div>
  );
}

function ImportSection({ label, items }: { label: string; items: string[] }) {
  return (
    <details className="import-summary-section">
      <summary>
        {label} ({items.length})
      </summary>
      <ul>
        {items.map((s, i) => (
          <li key={i}>{s}</li>
        ))}
      </ul>
    </details>
  );
}
