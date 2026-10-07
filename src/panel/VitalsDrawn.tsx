import type { ReactNode } from 'react';
import type { VitalsOpponent } from '../ipc/uiConfig';
import type { CombatOpponent } from '../stores/gmcp/combatStore';
import { MarkValue } from './VitalsMarks';
import {
  opponentHealth,
  widestOpponentHealth,
  type OpponentHealth,
  type ShownVital,
  type VitalInks,
} from './vitalsView';

// What the eight styles of the More Vitals Styles review share. Your
// opponent draws its name and its health on one line across the footer
// with the style's own mark under them in warn, 10 above your vitals or
// 10 under them (Q25). The row styles set each vital's label, mark and
// value on a pane row, as Gauges does, and drop each mark under its
// label and value on a narrow panel (Q24). The column styles stand
// your vitals in columns, as Ledger does, and never stack.

/** What every new style draws from. */
export interface DrawnVitalsProps {
  rows: readonly ShownVital[];
  /** Your vitals are on but have not come yet, so a line says so. */
  waiting: boolean;
  combat: CombatOpponent | null;
  place: VitalsOpponent;
  inks: VitalInks;
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
