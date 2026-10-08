import type { ReactNode } from 'react';
import type { VitalsOpponent } from '../ipc/uiConfig';
import type { CombatOpponent } from '../stores/gmcp/combatStore';
import { MarkRow, MarkValue } from './VitalsMarks';
import { HitGhost } from './HitGhost';
import { hitFill, type HitView, type HitViews } from './vitalsHit';
import {
  opponentHealth,
  toneProps,
  widestOpponentHealth,
  type OpponentHealth,
  type ShownVital,
  type VitalInks,
} from './vitalsView';

// What the eight styles of the More Vitals Styles review share. Your
// opponent draws its name and its health on one line across the footer
// with the style's own mark under them in warn, 10 above your vitals or
// 10 under them. The row styles set each vital's label, mark and
// value on a pane row, as Gauges does, and drop each mark under its
// label and value on a narrow panel. The column styles stand
// your vitals in columns, as Ledger does, and never stack.

/** What every new style draws from. */
export interface DrawnVitalsProps {
  rows: readonly ShownVital[];
  /** Your vitals are on but have not come yet, so a line says so. */
  waiting: boolean;
  combat: CombatOpponent | null;
  place: VitalsOpponent;
  inks: VitalInks;
  /** What Show each hit leaves on each mark now. */
  hits: HitViews;
}

/** Your opponent's name and health on one line, with `under` across
 *  the footer below them, or `before` ahead of the name. It sits above
 *  your vitals or under them, and alone once none of them draw. */
export function DrawnOpponent({
  combat,
  rows,
  waiting,
  place,
  under,
  before,
}: Pick<DrawnVitalsProps, 'rows' | 'waiting' | 'place'> & {
  combat: CombatOpponent;
  under?: (health: OpponentHealth) => ReactNode;
  before?: (health: OpponentHealth) => ReactNode;
}) {
  const health = opponentHealth(combat);
  const at = rows.length === 0 && !waiting ? 'alone' : place;
  return (
    <div className={`vitals-foe vitals-tone is-foe is-${at}${health.hidden ? ' is-hidden' : ''}`}>
      <div className="vitals-foe-line">
        {before && (
          <span className="vitals-foe-before" aria-hidden="true">
            {before(health)}
          </span>
        )}
        <span className="vitals-mark-label">{combat.name}</span>
        <MarkValue value={health.value} widest={widestOpponentHealth(health)} />
      </div>
      {under && (
        <div className="vitals-foe-mark" aria-hidden="true">
          {under(health)}
        </div>
      )}
    </div>
  );
}

/** The footer of a new style: your opponent on top or at the bottom
 *  around `children`, your vitals. */
export function DrawnVitals({
  kind,
  waiting,
  place,
  foe,
  children,
}: {
  kind: string;
  waiting: boolean;
  place: VitalsOpponent;
  foe: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className={`vitals-drawn is-${kind}`}>
      {place === 'top' && foe}
      {waiting && <p className="panel-vitals-empty">Vitals appear when you log in.</p>}
      {children}
      {place === 'bottom' && foe}
    </div>
  );
}

/** The column styles' columns, one for each vital, as Ledger lays
 *  them. */
export function Columns({
  rows,
  inks,
  cell,
}: {
  rows: readonly ShownVital[];
  inks: VitalInks;
  cell: (row: ShownVital) => ReactNode;
}) {
  if (rows.length === 0) return null;
  return (
    <div className="vitals-cols">
      {rows.map((row) => (
        <div key={row.key} {...toneProps(row.tone, inks[row.key], 'vitals-col')}>
          {cell(row)}
        </div>
      ))}
    </div>
  );
}

/** A 2 px line across the footer at `pct`, with the part Show each hit
 *  leaves pale, the mark your opponent takes where the style's own
 *  mark cannot stretch. */
export function LineMark({ pct, hit }: { pct: number | null; hit: HitView | undefined }) {
  const { fill, ghost, draining } = hitFill(pct, hit);
  return (
    <span className="vitals-line-mark">
      {fill !== null && ghost !== null && (
        <HitGhost className="vitals-line-gone" fill={fill} ghost={ghost} draining={draining} />
      )}
      {fill !== null && <span className="vitals-line-fill" style={{ width: `${fill}%` }} />}
    </span>
  );
}

/** The row styles' grid: each vital's label, its mark and its value,
 *  or the mark under the label and value. */
export function MarkRows({
  kind,
  under,
  rows,
  inks,
  mark,
}: {
  kind: string;
  under: boolean;
  rows: readonly ShownVital[];
  inks: VitalInks;
  mark: (row: ShownVital) => ReactNode;
}) {
  if (rows.length === 0) return null;
  return (
    <div className={`vitals-marks is-${kind}${under ? ' is-under' : ''}`}>
      {rows.map((row) => (
        <MarkRow key={row.key} row={row} ink={inks[row.key]}>
          <span className="vitals-mark" aria-hidden="true">
            {mark(row)}
          </span>
        </MarkRow>
      ))}
    </div>
  );
}
