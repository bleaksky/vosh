import { useState } from 'react';
import {
  applyImport,
  detectImportFormat,
  type ImportFormat,
  type ImportSummary,
} from '../lib/session';

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
      <div className="import-help">
        Import aliases, triggers, keyboard macros, and variables from another MUD client. Drop a
        file in or paste its contents below, pick a format (or leave on auto-detect), and hit apply.
        Existing entries with the same name get overwritten; anything that cannot be modeled is
        listed in the summary so you can port it by hand.
      </div>

      <div className="import-format-row">
        <span className="settings-section-label">format</span>
        <label className="import-format-opt">
          <input
            type="radio"
            name="import-format"
            value=""
            checked={format === ''}
            onChange={() => setFormat('')}
          />
          auto-detect
        </label>
        {FORMATS.map((f) => (
          <label key={f.id} className="import-format-opt" title={f.hint}>
            <input
              type="radio"
              name="import-format"
              value={f.id}
              checked={format === f.id}
              onChange={() => setFormat(f.id)}
            />
            {f.label}
          </label>
        ))}
      </div>

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

      {summary && <ImportSummaryView summary={summary} />}
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
