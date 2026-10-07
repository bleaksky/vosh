import type { CSSProperties } from 'react';
import type { VitalsOpponent, VitalsValues } from '../ipc/uiConfig';
import type { CombatOpponent } from '../stores/gmcp/combatStore';
import { ledgerFigure, ledgerFigurePx, type LedgerFigure, type LedgerFit } from './vitalsLedgerFit';
import {
  opponentHealth,
  toneProps,
  VITAL_LABELS,
  type ShownVital,
  type VitalInks,
  type VitalTone,
} from './vitalsView';

// Ledger (Vitals Styles, board 1): a column for each vital, the pane
// label caps over the figure and its max, with a line under each
// column, and your opponent's name and health across the footer above
// or below them. The fit (vitalsLedger.ts) drops the max and steps the
// figure down on a narrow panel, and never stacks.

export function VitalsLedger({
  rows,
  waiting,
  combat,
  place,
  values,
  fit,
  size,
  meter,
  inks,
}: {
  rows: readonly ShownVital[];
  /** Your vitals are on but have not come yet, so a line says so. */
  waiting: boolean;
  combat: CombatOpponent | null;
  place: VitalsOpponent;
  values: VitalsValues;
  fit: LedgerFit;
  size: number;
  /** Draw the line under each column, as Meter asks. */
  meter: boolean;
  inks: VitalInks;
}) {
  const foe = combat && (
    <LedgerOpponent
      combat={combat}
      meter={meter}
      place={rows.length === 0 && !waiting ? 'alone' : place}
    />
  );
  return (
    <div
      className="vitals-ledger"
      style={{ '--vitals-figure': `${ledgerFigurePx(fit, size)}px` } as CSSProperties}
    >
      {place === 'top' && foe}
      {waiting && <p className="panel-vitals-empty">Vitals appear when you log in.</p>}
      {rows.length > 0 && (
        <div className="vitals-ledger-columns">
          {rows.map((row) => (
            <LedgerColumn
              key={row.key}
              label={VITAL_LABELS[row.key]}
              figure={ledgerFigure(values, row.current, row.max, row.tone === 'hidden')}
              showMax={fit === 'full'}
              pct={row.pct}
              tone={row.tone}
              ink={inks[row.key]}
              meter={meter}
            />
          ))}
        </div>
      )}
      {place === 'bottom' && foe}
    </div>
  );
}

function LedgerColumn({
  label,
  figure,
  showMax,
  pct,
  tone,
  ink,
  meter,
}: {
  label: string;
  figure: LedgerFigure;
  showMax: boolean;
  pct: number | null;
  tone: VitalTone;
  ink: string | undefined;
  meter: boolean;
}) {
  return (
    <div {...toneProps(tone, ink, 'vitals-ledger-column')}>
      <span className="vitals-ledger-label">{label}</span>
      <span className="vitals-ledger-figure">
        <span className="vitals-ledger-current">{figure.current}</span>
        {showMax && figure.max !== null && <span className="vitals-ledger-max">{figure.max}</span>}
      </span>
      {meter && <LedgerLine pct={pct} />}
    </div>
  );
}

/** Your opponent's name and health across the footer, with a line
 *  under both. */
function LedgerOpponent({
  combat,
  meter,
  place,
}: {
  combat: CombatOpponent;
  meter: boolean;
  place: 'top' | 'bottom' | 'alone';
}) {
  const health = opponentHealth(combat);
  return (
    <div
      className={`vitals-ledger-opponent vitals-tone is-foe${health.hidden ? ' is-hidden' : ''}${place === 'top' ? '' : ` is-${place}`}`}
    >
      <span className="vitals-ledger-name">{combat.name}</span>
      <span className="vitals-ledger-figure">
        <span className="vitals-ledger-current">{health.value}</span>
      </span>
      {meter && <LedgerLine pct={health.pct} />}
    </div>
  );
}

function LedgerLine({ pct }: { pct: number | null }) {
  return (
    <div className="vitals-ledger-line" aria-hidden="true">
      {pct !== null && <div className="vitals-ledger-fill" style={{ width: `${pct}%` }} />}
    </div>
  );
}
