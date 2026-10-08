import type { ReactNode } from 'react';
import type { VitalsOpponent } from '../ipc/uiConfig';
import type { CombatOpponent } from '../stores/gmcp/combatStore';
import {
  opponentHealth,
  toneProps,
  VITAL_LABELS,
  widestOpponentHealth,
  type ShownVital,
  type VitalInks,
} from './vitalsView';

// The rows Gauges and Pips share (VitalsGauges.tsx, VitalsPips.tsx):
// each vital's label, its mark and its value on a 22 px pane row, or
// the mark under the label and value on a narrow panel. Each value sits
// over its widest form, the vital at its max, so a fight never moves a
// mark. Your opponent draws its name and its health with no mark, so a
// long name keeps the room.

export interface MarkedVitalsProps {
  rows: readonly ShownVital[];
  /** Your vitals are on but have not come yet, so a line says so. */
  waiting: boolean;
  combat: CombatOpponent | null;
  place: VitalsOpponent;
  inks: VitalInks;
}

export function VitalsMarks({
  kind,
  under,
  mark,
  rows,
  waiting,
  combat,
  place,
  inks,
}: MarkedVitalsProps & {
  kind: 'gauges' | 'pips';
  /** Each mark sits under its label and value. */
  under: boolean;
  /** A vital's mark. */
  mark: (row: ShownVital) => ReactNode;
}) {
  const foe = combat && <MarkedOpponent combat={combat} />;
  return (
    <div className={`vitals-marks is-${kind}${under ? ' is-under' : ''}`}>
      {place === 'top' && foe}
      {waiting && <p className="panel-vitals-empty">Vitals appear when you log in.</p>}
      {rows.map((row) => (
        <MarkRow key={row.key} row={row} ink={inks[row.key]}>
          {mark(row)}
        </MarkRow>
      ))}
      {place === 'bottom' && foe}
    </div>
  );
}

/** One vital's row: its label, `children` for its mark, and its value.
 *  Every row style draws its vitals with it. */
export function MarkRow({
  row,
  ink,
  children,
}: {
  row: ShownVital;
  ink: string | undefined;
  children: ReactNode;
}) {
  return (
    <div {...toneProps(row.tone, ink, 'vitals-mark-row')}>
      <span className="vitals-mark-label">{VITAL_LABELS[row.key]}</span>
      {children}
      <MarkValue value={row.value} widest={row.widest} />
    </div>
  );
}

/** A value over its widest form, which holds the column. */
export function MarkValue({ value, widest }: { value: string; widest: string }) {
  return (
    <span className="vitals-mark-value">
      <span className="vitals-mark-widest" aria-hidden="true">
        {widest}
      </span>
      <span>{value}</span>
    </span>
  );
}

function MarkedOpponent({ combat }: { combat: CombatOpponent }) {
  const health = opponentHealth(combat);
  return (
    <div
      className={`vitals-mark-row vitals-mark-opponent vitals-tone is-foe${health.hidden ? ' is-hidden' : ''}`}
    >
      <span className="vitals-mark-label">{combat.name}</span>
      <MarkValue value={health.value} widest={widestOpponentHealth(health)} />
    </div>
  );
}
