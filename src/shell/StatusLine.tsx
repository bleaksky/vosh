import { useEffect, useLayoutEffect, useMemo, useRef, useState, type RefObject } from 'react';
import { vitalsTextWatch } from '../ipc/vitals';
import type { ChipStyle, VitalsOptions } from '../ipc/uiConfig';
import { useBandEnv } from '../prompt/useBandEnv';
import { usePlayPalette } from '../theme/fitGameColors';
import { useChipStyle } from '../stores/config/chipStyleStore';
import { useCombat, type CombatOpponent } from '../stores/gmcp/combatStore';
import { useGameTime } from '../stores/config/gameTimeStore';
import { useSelected } from '../stores/session/sessionsStore';
import { useRoundTrip } from '../stores/session/roundTripStore';
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
import { readPanelFace, readPanelTextPx, textWidth, usePanelFaceVersion } from '../panel/panelFace';
import {
  formatVital,
  hiddenVital,
  opponentHealth,
  sameMob,
  vitalRows,
  vitalsOn,
  widestVital,
  VITAL_LABELS,
  type ShownVital,
  type VitalTone,
} from '../panel/vitalsView';
import { TextRuns, type TextColors } from '../panel/VitalsText';
import { vitalsTextPieces } from '../panel/vitalsTextFit';
import { daylightTint, isDaytime } from './daylight';
import { formatGameTime } from './gameTime';
import { StatusClock, type ClockMoons, type ClockTick, type ClockTime } from './StatusClock';
import { FIT_ALL, statusLineFit, type StatusLineFit } from './statusLineFit';
import { roundTripText, roundTripTone, SLOW_MS, WIDEST_ROUND_TRIP } from './roundTrip';
import { statusMoons } from './statusMoons';
import { useVitalsMenu } from '../panel/useVitalsMenu';
import { VisuallyHidden } from '../ui';

// The quiet line under the input band (SPEC 10 G4): your vitals when
// the line carries them, your opponent, your target, then the tick, the
// round trip to the game, then the tick, the game time, and the moons
// together, 20 px apart in the panel face, the Panel font, with tabular
// numbers.
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
// leaves your vitals to the panel, your target shows by name alone. As
// the line runs short the opponent's name, the labels, Values, the moons,
// a fine round trip and the game time give way in that order
// (statusLineFit.ts).
//
// The round trip to the game is the selected session's, which it reads
// every two seconds (Round Trip Readout). Nothing shows before the first
// reading or once the connection ends. It reads in tertiary under
// 300 ms, in the warn tone from 300 ms, and in seconds in the danger
// text tone from a second, and a slow one never gives way
// (roundTrip.ts).
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
// A right click on your vitals, your opponent or your vitals text opens
// the vitals menu (VitalsMenu.tsx), as on the panel footer.
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
  const items = statusItems({ showVitals, vitals, target: target.name, combat, options, text });
  const clock = useClock(connected);
  const roundTrip = useRoundTrip();
  const lineRef = useRef<HTMLDivElement | null>(null);
  const fit = useStatusFit(lineRef, items, clock, connected, roundTrip);
  const vitalsMenu = useVitalsMenu();

  return (
    <div
      ref={lineRef}
      className="shell-statusline"
      role="group"
      aria-label="Status"
      onContextMenu={(e) => {
        if (e.target instanceof Element && e.target.closest(VITALS_ITEMS)) vitalsMenu.open(e);
      }}
    >
      {!connected && <span>{NOT_CONNECTED}</span>}
      <StatusItemsView items={items} fit={fit} />
      {roundTrip !== null && fit.roundTrip && <RoundTripItem ms={roundTrip} />}
      <StatusClock
        style={clock.style}
        tick={clock.tick}
        time={fit.time ? clock.time : null}
        moons={fit.moons ? clock.moons : null}
      />
      {vitalsMenu.menu}
    </div>
  );
}

const NOT_CONNECTED = 'Not connected';

/** The round trip to the game, in its tone, with a plain title. */
export function RoundTripItem({ ms }: { ms: number }) {
  const tone = roundTripTone(ms);
  return (
    <span
      className={`shell-status-rtt${tone === 'fine' ? '' : ` is-${tone}`}`}
      title="Round trip to the game"
    >
      {roundTripText(ms)}
    </span>
  );
}

/** The items a right click opens the vitals menu on: your vitals, your
 *  opponent and your vitals text. */
const VITALS_ITEMS = '.shell-status-vital, .shell-status-foe, .shell-status-text';

/** How the line fits while it carries your quiet form: the widths
 *  statusLineFit weighs, measured in the panel face at your panel size
 *  against the line's room. Everything shows otherwise. */
function useStatusFit(
  lineRef: RefObject<HTMLDivElement | null>,
  items: StatusItems,
  clock: ClockProps,
  connected: boolean,
  roundTrip: number | null,
): StatusLineFit {
  const room = useLineRoom(lineRef);
  const faceVersion = usePanelFaceVersion();
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const font = useMemo(() => `${readPanelTextPx()}px ${readPanelFace()}`, [faceVersion]);
  if (!items.quiet || room === null) return FIT_ALL;
  const measure = (text: string) => textWidth(text, font, faceVersion);
  return statusLineFit({
    room,
    lead: connected ? 0 : measure(NOT_CONNECTED),
    vitals: items.rows.map((row) => ({
      label: measure(VITAL_LABELS[row.key]),
      value: measure(row.widest),
      current: measure(widestVital('current', row.max, row.tone === 'hidden')),
    })),
    foe: items.keepsFoe
      ? Math.max(measure('100%'), items.foe ? measure(opponentHealth(items.foe).value) : 0)
      : null,
    fighting: items.foe !== null,
    target: items.target === null ? 0 : measure('Target') + VALUE_GAP_PX,
    roundTrip:
      roundTrip === null
        ? 0
        : Math.max(measure(WIDEST_ROUND_TRIP), measure(roundTripText(roundTrip))),
    slow: roundTrip !== null && roundTrip >= SLOW_MS,
    ...clockWidths(clock, measure),
  });
}

/** The 6 px a caption, an icon, a label or a name keeps before its
 *  value, the 12 px of a caption icon, and the 14 px moons 4 apart, as
 *  frame.css and StatusClock draw them. */
const VALUE_GAP_PX = 6;
const ICON_PX = 12;
const MOON_PX = 14;
const MOON_GAP_PX = 4;

/** The clock's parts as wide as they draw, the tick at two figures at
 *  least so it holds still as the seconds count. */
function clockWidths(
  clock: ClockProps,
  measure: (text: string) => number,
): { tick: number; time: number; moons: number } {
  const lead = (caption: string) =>
    clock.style === 'icon_value'
      ? ICON_PX + VALUE_GAP_PX
      : clock.style === 'caption_value'
        ? measure(caption) + VALUE_GAP_PX
        : 0;
  const tick = clock.tick
    ? lead('Tick') + Math.max(measure(`${clock.tick.secs}s`), measure('00s'))
    : 0;
  const time = clock.time ? lead('Time') + measure(clock.time.text) : 0;
  const sky = clock.moons?.moons.length ?? 0;
  const moons =
    sky === 0 || !clock.moons
      ? 0
      : (clock.style === 'caption_value' ? measure('Moons') + VALUE_GAP_PX : 0) +
        sky * MOON_PX +
        (sky - 1) * MOON_GAP_PX +
        (clock.moons.alignment ? VALUE_GAP_PX + measure(clock.moons.alignment) : 0);
  return { tick, time, moons };
}

/** The room inside the line's 16 px sides, null before the first
 *  measure. */
function useLineRoom(lineRef: RefObject<HTMLDivElement | null>): number | null {
  const [room, setRoom] = useState<number | null>(null);
  useLayoutEffect(() => {
    const node = lineRef.current;
    if (!node) return;
    const measure = () => {
      const style = getComputedStyle(node);
      const sides = parseFloat(style.paddingLeft) + parseFloat(style.paddingRight);
      setRoom(node.clientWidth - (Number.isFinite(sides) ? sides : 0));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    return () => observer.disconnect();
  }, [lineRef]);
  return room;
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

/** What the line draws of your vitals, your opponent and your target. */
interface StatusItems {
  /** Your vitals text, while the line writes it in the Text style. */
  text: LineText | null;
  rows: ShownVital[];
  /** Your opponent in a fight. */
  foe: CombatOpponent | null;
  /** The line carries your quiet form, so it gives way as it runs
   *  short. */
  quiet: boolean;
  /** It keeps room for your opponent, which you left on. */
  keepsFoe: boolean;
  /** A Target item, for a target on no mob the line already names. */
  target: string | null;
}

/** The items the line draws from plain values. */
function statusItems({
  showVitals,
  vitals,
  target,
  combat,
  options,
  text = null,
}: StatusVitalsProps): StatusItems {
  const writes = showVitals && options.style === 'text';
  const quiet = showVitals && !writes;
  const keepsFoe = quiet && !options.off.includes('opponent');
  const foe = keepsFoe ? combat : null;
  // The fight your text writes, or the opponent item, names the mob, so
  // a target on it adds nothing.
  const named = writes ? (text?.fight ? combat : null) : foe;
  return {
    text: writes && text && text.pieces.length > 0 ? text : null,
    rows:
      showVitals && !writes && vitals
        ? vitalRows(vitals, vitalsOn(options.order, options.off), options)
        : [],
    foe,
    quiet,
    keepsFoe,
    target: target && !(named && sameMob(target, named.name)) ? target : null,
  };
}

/** Your vitals, your opponent and your target, drawn from plain values
 *  so a test can render every case, as `fit` lets them. */
export function StatusVitals({
  fit = FIT_ALL,
  ...props
}: StatusVitalsProps & { fit?: StatusLineFit }) {
  return <StatusItemsView items={statusItems(props)} fit={fit} />;
}

/** A label, a name, or a value the line gives way on, still read by a
 *  screen reader. */
function Hideable({ shown, children }: { shown: boolean; children: string }) {
  return shown ? <>{children}</> : <VisuallyHidden>{children}</VisuallyHidden>;
}

function StatusItemsView({ items, fit }: { items: StatusItems; fit: StatusLineFit }) {
  const { text, rows, foe, target } = items;
  return (
    <>
      {text && (
        <span className="shell-status-text" style={{ color: text.env.fg }}>
          {text.pieces.map((cells, i) => (
            <span key={i} className="shell-status-text-piece">
              <TextRuns cells={cells} env={text.env} />
            </span>
          ))}
        </span>
      )}
      {rows.map((row) => (
        <span key={row.key} className="shell-status-vital">
          <Hideable shown={fit.labels}>{VITAL_LABELS[row.key]}</Hideable>
          <span className={toneClass(row.tone, !fit.labels)}>
            {!fit.current
              ? row.value
              : row.tone === 'hidden'
                ? hiddenVital('current')
                : formatVital('current', row.current, row.max)}
          </span>
        </span>
      ))}
      {foe && <FoeItem combat={foe} name={fit.names} />}
      {target && fit.names && (
        <span className="shell-status-target">
          Target<span className="shell-status-value">{target}</span>
        </span>
      )}
    </>
  );
}

/** Your opponent's name, then its health in the warn tone, or a quiet
 *  `?` while the game withholds it. The name gives way first. */
function FoeItem({ combat, name }: { combat: CombatOpponent; name: boolean }) {
  const health = opponentHealth(combat);
  return (
    <span className="shell-status-foe">
      {name ? (
        <span className="shell-status-name">{combat.name}</span>
      ) : (
        <VisuallyHidden>{combat.name}</VisuallyHidden>
      )}
      <span className={toneClass(health.hidden ? 'hidden' : 'warn', !name)}>{health.value}</span>
    </span>
  );
}

/** A value's classes for its tone, `bare` with no label or name before
 *  it. */
function toneClass(tone: VitalTone, bare = false): string {
  const tones = { quiet: '', danger: ' is-low', warn: ' is-warn', hidden: ' is-hidden' };
  return `shell-status-value${tones[tone]}${bare ? ' is-bare' : ''}`;
}

/** The clock as StatusClock draws it. */
interface ClockProps {
  style: ChipStyle;
  tick: ClockTick | null;
  time: ClockTime | null;
  moons: ClockMoons | null;
}

/** Reads the tick, the way it counts, the game time on its clock, the
 *  moons, and the theme for StatusClock. The daylight tint and the moons
 *  take the play palette, fitted while Fit game colors is on. The moons
 *  show only while connected. */
function useClock(connected: boolean): ClockProps {
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
  return {
    style,
    tick: shown
      ? {
          secs: shown.secs,
          count: shown.count,
          warn: tick.warn,
          overdue: tick.overdue,
          interval: tick.intervalSecs,
        }
      : null,
    time: text ? { text, tint, daytime: isDaytime(world.time), hour } : null,
    moons,
  };
}
