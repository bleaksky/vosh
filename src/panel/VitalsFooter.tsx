import {
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type Ref,
  type RefObject,
} from 'react';
import { readPanelFace, usePanelFaceVersion } from './panelFace';
import type { VitalsOptions, VitalsPlace, VitalsStyle, VitalsValues } from '../ipc/uiConfig';
import { useCombat, type CombatOpponent } from '../stores/gmcp/combatStore';
import { useVitalsOptions } from '../stores/config/vitalsOptionsStore';
import { useVitals, type Vitals } from '../stores/gmcp/vitalsStore';
import {
  opponentHealth,
  shownRows,
  widestOpponentHealth,
  vitalsFooterHeight,
  vitalsOn,
  vitalsGeometry,
  vitalInks,
  VITAL_LABELS,
  type ShownVital,
  type VitalInks,
  type VitalsGeometry,
  type VitalTone,
} from './vitalsView';
import { usePlayPalette } from '../theme/fitGameColors';
import { themeTokens } from '../theme/themes';
import { useActiveTheme } from '../theme/useActiveTheme';
import { panelWidthOf, usePanelLayout } from './panelLayoutStore';
import { textPx, usePaneText } from './paneTextSize';
import { vitalsLineFit, type VitalsLineFit } from './vitalsLine';
import {
  ledgerFigure,
  ledgerFit,
  ledgerHeight,
  type LedgerFit,
  type MeasureText,
} from './vitalsLedgerFit';
import { VitalsLedger } from './VitalsLedger';
import { VitalsGauges } from './VitalsGauges';
import { VitalsPips } from './VitalsPips';
import { gaugesFit, marksHeight, pipsFit, type GaugesFit, type PipsFit } from './vitalsMarksFit';
import { VitalsText, type TextColors } from './VitalsText';

// Vitals pinned under the panes (SPEC 5, G3). Each vital is a label,
// the value, and a meter that stays tertiary at rest and turns danger
// when the vital runs low. In a fight the opponent gets a row on top
// with its health in warn. Nothing pulses.
//
// Settings, Layout, Vitals shapes it (VitalsOptions.dc.html). Density
// picks Rows, one row per vital, or One line, Health, Mana, and Moves
// side by side, each with its label at the left, its value at the
// right, and its meter under both. One line drops the labels only when
// they no longer fit beside the values, under about 360 pt for four
// digit health, and a panel too narrow for even the values stacks them
// in rows (vitalsLine.ts). Values writes each number as current and
// max, the current alone, or percent. Meter draws the 2 px line, the
// 4 px bar, or none at the panes' 22 px pitch. The rows, the text and
// the space round them scale with your panel size, as the panes do,
// and the meter keeps its px. Warn before you run low
// turns a vital warn under two thirds and danger under one third. The
// rules live in vitalsView.ts.
//
// Customize vitals sets which vitals show and their order, and puts
// your opponent's row on top or at the bottom, or drops it. The footer
// holds the height of the vitals that show, and room for every vital
// you left on only while it waits for your vitals at login.
//
// While the game hides your vitals (Char.Vitals with the hidden flag,
// under lamented tears) each one reads `?` in its Values form, in
// tertiary, over an empty meter, and nothing warns. The opponent's
// health reads `?` the same way while Char.Combat withholds it or
// sends neither a percent nor a condition.
//
// While your pinned prompt hides your vitals, or you turned all three
// off, the footer keeps only the opponent row, so a fight still shows
// its health on the right. Out of a fight it draws nothing and the
// panes take its room.

// A color you pick for a vital under Customize vitals is a slot of the
// play palette, so it follows Color vision, lifted to 3:1 on the panel.
// It colors the vital's label and its meter, never the number, and low
// and warn still turn the meter and the value.

// Text writes your vitals with your prompt's codes (VitalsText.tsx),
// in the panel only. With your vitals in the status line it draws Rows
// here until the status line draws them.

// Ledger draws columns of figures under the pane label caps
// (VitalsLedger.tsx). Meter sets the line under each column there.
// Gauges and Pips draw a pill or discs between each label and value
// (VitalsGauges.tsx, VitalsPips.tsx), their own marks, so Meter goes
// quiet for them.

/** `opponentOnly` keeps only the opponent row, for while your pinned
 *  prompt hides your vitals. `textColors` are the terminal settings the
 *  Text style draws its colors with. */
export function VitalsFooter({
  opponentOnly = false,
  textColors = NO_TEXT_COLORS,
}: { opponentOnly?: boolean; textColors?: TextColors | undefined } = {}) {
  const vitals = useVitals();
  const combat = useCombat();
  const options = useVitalsOptions();
  const theme = useActiveTheme();
  const palette = usePlayPalette();
  const inks = useMemo(
    () => vitalInks(options.colors, palette, themeTokens(theme)),
    [options.colors, palette, theme],
  );
  const style = drawnStyle(options.style, options.place);
  const sectionRef = useRef<HTMLElement | null>(null);
  const width = useFooterWidth(sectionRef, style !== 'rows');
  const { size } = usePaneText();
  // The vitals draw in the panel face at your panel size, so they
  // measure in it, again each time it changes or a face loads.
  const faceVersion = usePanelFaceVersion();
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const face = useMemo(() => readPanelFace(), [faceVersion]);
  if (style === 'text') {
    return (
      <VitalsText width={width} hostRef={sectionRef} colors={textColors} fightOnly={opponentOnly} />
    );
  }
  // Fit by each vital at its max, the widest its value reads, so the
  // footer does not jump between forms as a value loses a digit in a
  // fight. Only a new max, a new Values form, a new panel width, or a
  // new face moves it.
  const measure: MeasureText = (text, px, weight) =>
    textWidth(text, `${weight} ${px}px ${face}`, faceVersion);
  // Only the vitals the footer draws, none while it keeps the opponent
  // alone.
  const rows =
    vitals === null || opponentOnly
      ? []
      : shownRows(vitals, vitalsOn(options.order, options.off), options);
  // Gauges and Pips measure your opponent's health with your values.
  const foe = combat && !options.off.includes('opponent') ? combat : null;
  const fit = fitOf(style, width, size, rows, foe, options.values, measure);

  return (
    <VitalsBlock
      sectionRef={sectionRef}
      vitals={vitals}
      combat={combat}
      fit={fit}
      options={options}
      inks={inks}
      opponentOnly={opponentOnly}
    />
  );
}

/** The style the footer draws, with how it fits the panel's width. */
export type VitalsFit =
  | { style: 'rows' }
  | { style: 'line'; fit: VitalsLineFit }
  | { style: 'ledger'; fit: LedgerFit }
  | { style: 'gauges'; fit: GaugesFit }
  | { style: 'pips'; fit: PipsFit };

/** The footer styles drawn so far. */
type DrawnStyle = VitalsFit['style'] | 'text';

/** Text draws in the panel only, and Rows stands in for it elsewhere. */
function drawnStyle(style: VitalsStyle, place: VitalsPlace): DrawnStyle {
  return style === 'text' && place !== 'panel' ? 'rows' : style;
}

/** The terminal's colors as they start, for a footer handed none. */
const NO_TEXT_COLORS: TextColors = { themeTerminalColors: false, brightBold: false };

/** How `style` fits a footer `width` px wide at panel size `size`. */
function fitOf(
  style: VitalsFit['style'],
  width: number,
  size: number,
  rows: readonly ShownVital[],
  combat: CombatOpponent | null,
  values: VitalsValues,
  measure: MeasureText,
): VitalsFit {
  if (style === 'line') {
    const items = rows.map((row) => ({
      label: measure(VITAL_LABELS[row.key], size, 400),
      value: measure(row.widest, size, 500),
    }));
    return { style, fit: rows.length === 0 ? 'rows' : vitalsLineFit(width, items) };
  }
  if (style === 'ledger') {
    const widest = rows.map((row) => ledgerFigure(values, row.max, row.max, row.tone === 'hidden'));
    return { style, fit: ledgerFit(width, size, widest, measure) };
  }
  if (style === 'gauges' || style === 'pips') {
    const labels = rows.map((row) => VITAL_LABELS[row.key]);
    const values = rows.map((row) => row.widest);
    if (combat) values.push(widestOpponentHealth(opponentHealth(combat)));
    return style === 'gauges'
      ? { style, fit: gaugesFit(width, size, labels, values, measure) }
      : { style, fit: pipsFit(width, size, labels, values, measure) };
  }
  return { style };
}

export interface VitalsBlockProps {
  vitals: Vitals | null;
  combat: CombatOpponent | null;
  /** The style the footer draws and how it fits the panel. */
  fit: VitalsFit;
  options: VitalsOptions;
  /** The color of each vital you gave one, lifted (vitalInks). */
  inks?: VitalInks;
  sectionRef?: Ref<HTMLElement>;
  /** Only the opponent row, and nothing out of a fight, as when every
   *  vital is off. */
  opponentOnly?: boolean;
}

/** The footer drawn from plain values, so every combination of the
 *  Vitals rows renders in a test. */
export function VitalsBlock({
  vitals,
  combat,
  fit,
  options,
  inks = {},
  sectionRef,
  opponentOnly = false,
}: VitalsBlockProps) {
  const { size } = usePaneText();
  const on = opponentOnly ? [] : vitalsOn(options.order, options.off);
  const foe = options.off.includes('opponent') ? null : combat;
  const rows = vitals === null ? [] : shownRows(vitals, on, options);
  // With no vital to draw, out of a fight, the panes take the room.
  const mine = vitals === null ? on.length : rows.length;
  if (mine === 0 && !foe) return null;
  const geometry = geometryAt(vitalsGeometry(options.meter), size);
  const label = mine === 0 ? 'Opponent' : 'Vitals';
  const waiting = vitals === null && mine > 0;
  if (fit.style === 'ledger') {
    return (
      <section
        ref={sectionRef}
        className="panel-vitals panel-vitals-ledger"
        style={ledgerStyle(geometry, waiting ? ledgerHeight(size, geometry.meter) : 0)}
        aria-label={label}
      >
        <VitalsLedger
          rows={rows}
          waiting={waiting}
          combat={foe}
          place={options.opponent}
          values={options.values}
          fit={fit.fit}
          size={size}
          meter={geometry.meter > 0}
          inks={inks}
        />
      </section>
    );
  }
  if (fit.style === 'gauges' || fit.style === 'pips') {
    const marked = {
      rows,
      waiting,
      combat: foe,
      place: options.opponent,
      inks,
    };
    return (
      <section
        ref={sectionRef}
        className={`panel-vitals panel-vitals-marks${fit.fit === 'under' ? ' is-under' : ''}`}
        style={waitingStyle(waiting ? marksHeight(size, mine) : 0)}
        aria-label={label}
      >
        {fit.style === 'gauges' ? (
          <VitalsGauges {...marked} fit={fit.fit} />
        ) : (
          <VitalsPips {...marked} fit={fit.fit} />
        )}
      </section>
    );
  }
  const line = mine > 0 && fit.style === 'line' && fit.fit !== 'rows';
  const meter = geometry.meter > 0;
  // Rows holds the vitals that show, and every vital you left on while
  // it waits for your vitals, so logging in moves nothing. One line
  // and the opponent alone hold one row.
  const held = mine === 0 || fit.style === 'line' ? 1 : mine;
  const opponent = foe && <OpponentRow combat={foe} meter={meter} />;

  return (
    <section
      ref={sectionRef}
      className={`panel-vitals${line ? ' is-one-line' : ''}`}
      style={footerStyle(geometry, held)}
      aria-label={label}
    >
      {options.opponent === 'top' && opponent}
      {mine === 0 ? null : vitals === null ? (
        <div className="panel-vitals-row">
          <p className="panel-vitals-empty">Vitals appear when you log in.</p>
        </div>
      ) : line ? (
        <div className="panel-vitals-row panel-vitals-oneline">
          {rows.map((r) => (
            <VitalItem
              key={r.key}
              label={VITAL_LABELS[r.key]}
              ink={inks[r.key]}
              showLabel={fit.fit === 'labels'}
              value={r.value}
              pct={r.pct}
              tone={r.tone}
              meter={meter}
            />
          ))}
        </div>
      ) : (
        rows.map((r) => (
          <VitalRow
            key={r.key}
            className={toneClass(r.tone)}
            label={VITAL_LABELS[r.key]}
            ink={inks[r.key]}
            value={r.value}
            pct={r.pct}
            meter={meter}
          />
        ))
      )}
      {options.opponent === 'bottom' && opponent}
    </section>
  );
}

/** `geometry` at panel size `size` px: the rows, the space above the
 *  text, the gap to the meter and the footer's padding scale as the
 *  text does, and the meter keeps its px. */
function geometryAt(geometry: VitalsGeometry, size: number): VitalsGeometry {
  return {
    ...geometry,
    row: textPx(geometry.row, size),
    rowTop: textPx(geometry.rowTop, size),
    meterGap: textPx(geometry.meterGap, size),
    padTop: textPx(geometry.padTop, size),
    padBottom: textPx(geometry.padBottom, size),
  };
}

/** Ledger's meter as custom properties panel.css reads, and the height
 *  it holds while it waits for your vitals, 0 once they show. */
function ledgerStyle(geometry: VitalsGeometry, waiting: number): CSSProperties {
  return {
    '--vitals-meter': `${geometry.meter}px`,
    '--vitals-meter-radius': `${geometry.meterRadius}px`,
    ...waitingStyle(waiting),
  } as CSSProperties;
}

/** The height a footer holds while it waits for your vitals, none once
 *  they show. */
function waitingStyle(waiting: number): CSSProperties | undefined {
  return waiting > 0 ? ({ '--vitals-min-height': `${waiting}px` } as CSSProperties) : undefined;
}

/** The geometry as custom properties panel.css reads. */
function footerStyle(geometry: VitalsGeometry, rows: number): CSSProperties {
  return {
    '--vitals-pad-top': `${geometry.padTop}px`,
    '--vitals-pad-bottom': `${geometry.padBottom}px`,
    '--vitals-min-height': `${vitalsFooterHeight(geometry, rows)}px`,
    '--vitals-row': `${geometry.row}px`,
    '--vitals-row-top': `${geometry.rowTop}px`,
    '--vitals-meter': `${geometry.meter}px`,
    '--vitals-meter-gap': `${geometry.meterGap}px`,
    '--vitals-meter-radius': `${geometry.meterRadius}px`,
  } as CSSProperties;
}

function toneClass(tone: VitalTone): string | undefined {
  if (tone === 'danger') return 'panel-vitals-row-low';
  if (tone === 'warn') return 'panel-vitals-row-warn';
  if (tone === 'hidden') return 'panel-vitals-row-hidden';
  return undefined;
}

/** The footer's drawn width while One line is chosen. It sits under
 *  the saved panel width when the window is too narrow for it, and the
 *  saved width stands in until the first measure. */
function useFooterWidth(el: RefObject<HTMLElement | null>, active: boolean): number {
  const saved = panelWidthOf(usePanelLayout());
  const [width, setWidth] = useState<number | null>(null);
  useLayoutEffect(() => {
    const node = el.current;
    if (!active || !node) return;
    const measure = () => setWidth(node.getBoundingClientRect().width);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    return () => observer.disconnect();
  }, [el, active]);
  return width ?? saved;
}

let measureCanvas: HTMLCanvasElement | null = null;
// Widths by font and text. The labels and maxes rarely change, so a
// vitals update reads these instead of measuring again.
const widths = new Map<string, number>();

/** How wide `text` draws in `font`. Values use tabular numbers, where
 *  every digit is as wide as a zero, so digits measure as zeros. A
 *  width taken before a face loaded is its fallback's, so `faceVersion`
 *  keys each width to the faces loaded when it was taken. */
function textWidth(text: string, font: string, faceVersion: number): number {
  const shape = text.replace(/[0-9]/g, '0');
  const key = `${faceVersion}|${font}|${shape}`;
  const known = widths.get(key);
  if (known !== undefined) return known;
  measureCanvas ??= document.createElement('canvas');
  const ctx = measureCanvas.getContext('2d');
  if (!ctx) return shape.length * 7;
  ctx.font = font;
  const width = Math.ceil(ctx.measureText(shape).width);
  if (widths.size > 64) widths.clear();
  widths.set(key, width);
  return width;
}

/** A vital's color on its row or One line item, as panel.css reads it. */
function inkProps(ink: string | undefined, className: string) {
  return ink
    ? {
        className: `${className} panel-vitals-swatch`,
        style: { '--vital-ink': ink } as CSSProperties,
      }
    : { className };
}

function VitalRow({
  label,
  value,
  pct,
  meter,
  className,
  ink,
}: {
  label: string;
  value: string;
  pct: number | null;
  meter: boolean;
  className?: string | undefined;
  ink?: string | undefined;
}) {
  return (
    <div {...inkProps(ink, `panel-vitals-row${className ? ` ${className}` : ''}`)}>
      <div className="panel-vitals-line">
        <span className="panel-vitals-label">{label}</span>
        <span className="panel-vitals-value">{value}</span>
      </div>
      {meter && <Meter pct={pct} />}
    </div>
  );
}

/** Your opponent's name and health, in warn, or a quiet `?` while the
 *  game withholds the health. */
function OpponentRow({ combat, meter }: { combat: CombatOpponent; meter: boolean }) {
  const health = opponentHealth(combat);
  return (
    <VitalRow
      className={`panel-vitals-row-combat${health.hidden ? ' panel-vitals-row-hidden' : ''}`}
      label={combat.name}
      value={health.value}
      pct={health.pct}
      meter={meter}
    />
  );
}

/** One vital on the One line row. Without its visible label, the label
 *  still names the value for a screen reader, and the value sits at the
 *  right end of its meter. */
function VitalItem({
  label,
  showLabel,
  value,
  pct,
  tone,
  meter,
  ink,
}: {
  label: string;
  ink: string | undefined;
  showLabel: boolean;
  value: string;
  pct: number | null;
  tone: VitalTone;
  meter: boolean;
}) {
  const toned = toneClass(tone);
  return (
    <div
      {...inkProps(
        ink,
        `panel-vitals-item${showLabel ? '' : ' is-bare'}${toned ? ` ${toned}` : ''}`,
      )}
    >
      <div className="panel-vitals-line">
        <span className={showLabel ? 'panel-vitals-label' : 'panel-vitals-label-hidden'}>
          {label}
        </span>
        <span className="panel-vitals-value">{value}</span>
      </div>
      {meter && <Meter pct={pct} />}
    </div>
  );
}

function Meter({ pct }: { pct: number | null }) {
  return (
    <div className="panel-vitals-meter" aria-hidden="true">
      {pct !== null && <div className="panel-vitals-fill" style={{ width: `${pct}%` }} />}
    </div>
  );
}
