import { useEffect, useMemo } from 'react';
import { vitalsTextWatch } from '../ipc/vitals';
import type { VitalsOptions } from '../ipc/uiConfig';
import { useBandEnv } from '../prompt/useBandEnv';
import { usePlayPalette } from '../theme/fitGameColors';
import { useChipStyle } from '../stores/config/chipStyleStore';
import { useCombat, type CombatOpponent } from '../stores/gmcp/combatStore';
import { useGameTime } from '../stores/config/gameTimeStore';
import { useSelected } from '../stores/session/sessionsStore';
import { useTarget } from '../stores/session/targetStore';
import { useTickCount } from '../stores/config/tickCountStore';
import { shownTick, useTick } from '../stores/session/tickStore';
import { useVitalsOptions } from '../stores/config/vitalsOptionsStore';
import { useVitals, type Vitals } from '../stores/gmcp/vitalsStore';
import { useVitalsText } from '../stores/session/vitalsTextStore';
import { useWorld } from '../stores/gmcp/worldStore';
import type { BandEnv } from '../terminal/bandCells';
import type { Cell } from '../terminal/sgrCells';
import { nativeSurfaceEnabled } from '../terminal/terminalRenderer';
import { themeTokens } from '../theme/themes';
import { useActiveTheme } from '../theme/useActiveTheme';
import {
  opponentHealth,
  sameMob,
  vitalRows,
  vitalsOn,
  VITAL_LABELS,
  type VitalTone,
} from '../panel/vitalsView';
import { TextRuns, type TextColors } from '../panel/VitalsText';
import { vitalsTextPieces } from '../panel/vitalsTextFit';
import { daylightTint, isDaytime } from './daylight';
import { formatGameTime } from './gameTime';
import { StatusClock } from './StatusClock';
import { statusMoons } from './statusMoons';

// The quiet line under the input band (SPEC 10 G4): your vitals when
// the line carries them, your opponent, your target, then the tick, the
// game time, and the moons together, 20 px apart in the panel face, the
// Panel font, with tabular numbers.
//
// The line carries your vitals with the panel hidden, or with Show your
// vitals in on Status line, which takes them out of the panel (Vitals
// Styles Q6). Every drawn style but Text shows one quiet form here,
// labels and values with no mark and no color. It follows the order,
// the vitals you turned off, Values and Warn before you run low from
// Customize vitals, keeps every vital you leave on with or without a
// max, and ignores Hide vitals while your prompt is pinned. In a fight
// your opponent follows your vitals with its health in the warn tone,
// wherever Customize vitals places its row in the panel. A target you
// set on the same mob joins that item, and a target on another mob
// keeps its own Target item after it. Out of a fight, or while the line
// leaves your vitals to the panel, your target shows by name alone.
//
// Text writes your vitals text here on one line, in the terminal face,
// with a 20 px gap for each new line and each %{right}, and ends in an
// ellipsis where the line runs out. The line watches the text itself
// only while the panel draws no footer, since the footer watches it
// otherwise.
//
// While the game hides your vitals (lamented tears) each one reads `?`
// in its Values form in tertiary and never warns. Your opponent's
// health reads a quiet `?` while Char.Combat withholds it.
//
// The tick, the game time, and the moons share one item, the way the
// old input row chip kept the tick and the time, 8 px apart inside it.
// The tick counts up from the last tick and turns the warn tone on a
// soft warn ground in the last seconds you set in the tick config. The
// time reads on the 24 or 12 hour clock you pick in Settings and takes
// a daylight tint from your theme. The moons in the sky show
// as phase icons in their own colors, with a word for an eclipse, the
// triad, or a near alignment. The chip style in Settings shows each
// value alone, after a caption, or after an icon.

/** The line splits each row of your text at its push, so the push
 *  needs no room and the session renders the text one cell wide. */
const LINE_COLS = 1;

interface Props {
  connected: boolean;
  /** The line carries your vitals, since the panel is hidden or Show
   *  your vitals in is Status line. */
  showVitals: boolean;
  /** The terminal settings the Text style draws its colors with. */
  textColors: TextColors;
}

export function StatusLine({ connected, showVitals, textColors }: Props) {
  const target = useTarget();
  const vitals = useVitals();
  const combat = useCombat();
  const options = useVitalsOptions();
  const writes = showVitals && options.style === 'text';
  const text = useLineText(writes, writes && options.place === 'status', textColors);

  return (
    <div className="shell-statusline" role="group" aria-label="Status">
      {!connected && <span>Not connected</span>}
      <StatusVitals
        showVitals={showVitals}
        vitals={vitals}
        target={target.name}
        combat={combat}
        options={options}
        text={text}
      />
      <ClockItem connected={connected} />
    </div>
  );
}

/** Your vitals text as the line writes it, while `active`, and a watch
 *  on it while `watch`, for when no footer watches it. */
function useLineText(active: boolean, watch: boolean, colors: TextColors): LineText | null {
  const session = useSelected();
  const rendered = useVitalsText();
  useEffect(() => {
    if (!watch) return;
    void vitalsTextWatch(LINE_COLS, session).catch(() => undefined);
    return () => {
      void vitalsTextWatch(null, session).catch(() => undefined);
    };
  }, [watch, session]);
  const env = useBandEnv(
    colors.themeTerminalColors,
    colors.brightBold,
    nativeSurfaceEnabled() ? 'native' : 'xterm',
  );
  const pieces = useMemo(
    () => (active && rendered ? vitalsTextPieces(rendered) : null),
    [active, rendered],
  );
  return pieces && rendered ? { pieces, fight: rendered.fight.some(Boolean), env } : null;
}

/** Your vitals text on the line: its pieces, whether a row reads your
 *  fight, and the colors it draws in. */
export interface LineText {
  pieces: Cell[][];
  fight: boolean;
  env: BandEnv;
}

export interface StatusVitalsProps {
  /** The line carries your vitals. */
  showVitals: boolean;
  vitals: Vitals | null;
  /** The target you set, or null. */
  target: string | null;
  /** The Char.Combat opponent, or null out of a fight. */
  combat: CombatOpponent | null;
  options: VitalsOptions;
  /** Your vitals text, while the line writes it in the Text style. */
  text?: LineText | null;
}

/** Your vitals, your opponent and your target, drawn from plain values
 *  so a test can render every case. */
export function StatusVitals({
  showVitals,
  vitals,
  target,
  combat,
  options,
  text = null,
}: StatusVitalsProps) {
  const writes = showVitals && options.style === 'text';
  const foe = showVitals && !writes && !options.off.includes('opponent') ? combat : null;
  // The fight your text writes, or the opponent item, names the mob, so
  // a target on it adds nothing.
  const named = writes ? (text?.fight ? combat : null) : foe;
  const ownTarget = target && !(named && sameMob(target, named.name)) ? target : null;
  const rows =
    showVitals && !writes && vitals
      ? vitalRows(vitals, vitalsOn(options.order, options.off), options)
      : [];
  return (
    <>
      {writes && text && text.pieces.length > 0 && (
        <span className="shell-status-text" style={{ color: text.env.fg }}>
          {text.pieces.map((cells, i) => (
            <span key={i} className="shell-status-text-piece">
              <TextRuns cells={cells} env={text.env} />
            </span>
          ))}
        </span>
      )}
      {rows.map((row) => (
        <span key={row.key}>
          {VITAL_LABELS[row.key]}
          <span className={toneClass(row.tone)}>{row.value}</span>
        </span>
      ))}
      {foe && <FoeItem combat={foe} />}
      {ownTarget && (
        <span className="shell-status-target">
          Target<span className="shell-status-value">{ownTarget}</span>
        </span>
      )}
    </>
  );
}

/** Your opponent's name, then its health in the warn tone, or a quiet
 *  `?` while the game withholds it. The name gives way first. */
function FoeItem({ combat }: { combat: CombatOpponent }) {
  const health = opponentHealth(combat);
  return (
    <span className="shell-status-foe">
      <span className="shell-status-name">{combat.name}</span>
      <span className={toneClass(health.hidden ? 'hidden' : 'warn')}>{health.value}</span>
    </span>
  );
}

function toneClass(tone: VitalTone): string {
  if (tone === 'danger') return 'shell-status-value is-low';
  if (tone === 'warn') return 'shell-status-value is-warn';
  if (tone === 'hidden') return 'shell-status-value is-hidden';
  return 'shell-status-value';
}

/** Reads the tick, the way it counts, the game time on its clock, the
 *  moons, and the theme for StatusClock. The daylight tint and the moons
 *  take the play palette, fitted while Fit game colors is on. The moons
 *  show only while connected. */
function ClockItem({ connected }: { connected: boolean }) {
  const style = useChipStyle();
  const tick = useTick();
  const shown = shownTick(tick, useTickCount());
  const world = useWorld();
  const theme = useActiveTheme();
  const palette = usePlayPalette();
  const text = formatGameTime(world.time, useGameTime());
  const hour = world.time?.hour ?? null;
  const tokens = useMemo(() => themeTokens(theme), [theme]);
  const tint = useMemo(() => daylightTint(hour, palette, tokens), [hour, palette, tokens]);
  const moons = useMemo(
    () => (connected ? statusMoons(world.moons, palette, tokens) : null),
    [connected, world.moons, palette, tokens],
  );
  return (
    <StatusClock
      style={style}
      tick={
        shown && {
          secs: shown.secs,
          count: shown.count,
          warn: tick.warn,
          overdue: tick.overdue,
          interval: tick.intervalSecs,
        }
      }
      time={text ? { text, tint, daytime: isDaytime(world.time), hour } : null}
      moons={moons}
    />
  );
}
