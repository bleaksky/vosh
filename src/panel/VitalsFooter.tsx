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
import type { VitalsDensity, VitalsOptions } from '../ipc/uiConfig';
import { useCombat, type CombatOpponent } from '../stores/gmcp/combatStore';
import { useVitalsOptions } from '../stores/config/vitalsOptionsStore';
import { useVitals, type Vitals, type VitalKey } from '../stores/gmcp/vitalsStore';
import {
  formatVital,
  hiddenVital,
  meterFill,
  vitalsFooterHeight,
  vitalsGeometry,
  vitalTone,
  widestVital,
  type VitalsGeometry,
  type VitalTone,
} from './vitalsView';
import { panelWidthOf, usePanelLayout } from './panelLayoutStore';
import { textPx, usePaneText } from './paneTextSize';
import { vitalsLineFit, type VitalsLineFit } from './vitalsLine';

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
// While the game hides your vitals (Char.Vitals with the hidden flag,
// under lamented tears) each one reads `?` in its Values form, in
// tertiary, over an empty meter, and nothing warns. The opponent's
// health reads `?` the same way while Char.Combat withholds it.
//
// While your pinned prompt hides your vitals, the footer keeps only the
// opponent row, so a fight still shows its health on the right. Out of
// a fight it draws nothing.

const ROWS: { key: VitalKey; label: string; max: 'maxhp' | 'maxmana' | 'maxmove' }[] = [
  { key: 'hp', label: 'Health', max: 'maxhp' },
  { key: 'mana', label: 'Mana', max: 'maxmana' },
  { key: 'move', label: 'Moves', max: 'maxmove' },
];

/** The vitals the MUD sends. Health always shows. While the game hides
 *  your vitals it sends every max as 0, so all three show. */
function shownRows(vitals: Vitals) {
  if (vitals.hidden) return ROWS;
  return ROWS.filter((r) => r.key === 'hp' || vitals[r.max] > 0);
}

/** `opponentOnly` keeps only the opponent row, for while your pinned
 *  prompt hides your vitals. */
export function VitalsFooter({ opponentOnly = false }: { opponentOnly?: boolean } = {}) {
  const vitals = useVitals();
  const combat = useCombat();
  const options = useVitalsOptions();
  // The footer draws One line, and Rows for every other style.
  const density: VitalsDensity = options.style === 'line' ? 'line' : 'rows';
  const sectionRef = useRef<HTMLElement | null>(null);
  const width = useFooterWidth(sectionRef, density === 'line');
  const { size } = usePaneText();
  // The vitals draw in the panel face at your panel size, so they
  // measure in it, again each time it changes or a face loads.
  const faceVersion = usePanelFaceVersion();
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const face = useMemo(() => readPanelFace(), [faceVersion]);
  // Fit by each vital at its max, the widest its value reads, so the
  // line does not jump between forms as a value loses a digit in a
  // fight. Only a new max, a new Values form, a new panel width, or a
  // new face moves it.
  const fit =
    density === 'line' && vitals !== null
      ? vitalsLineFit(
          width,
          shownRows(vitals).map((r) => ({
            label: textWidth(r.label, `400 ${size}px ${face}`, faceVersion),
            value: textWidth(
              widestVital(options.values, vitals[r.max], vitals.hidden),
              `500 ${size}px ${face}`,
              faceVersion,
            ),
          })),
        )
      : 'rows';

  return (
    <VitalsBlock
      sectionRef={sectionRef}
      vitals={vitals}
      combat={combat}
      density={density}
      fit={fit}
      options={options}
      opponentOnly={opponentOnly}
    />
  );
}

export interface VitalsBlockProps {
  vitals: Vitals | null;
  combat: CombatOpponent | null;
  density: VitalsDensity;
  /** How One line fits the panel. Rows ignores it. */
  fit: VitalsLineFit;
  options: VitalsOptions;
  sectionRef?: Ref<HTMLElement>;
  /** Only the opponent row, and nothing out of a fight. */
  opponentOnly?: boolean;
}

/** The footer drawn from plain values, so every combination of the
 *  Vitals rows renders in a test. */
export function VitalsBlock({
  vitals,
  combat,
  density,
  fit,
  options,
  sectionRef,
  opponentOnly = false,
}: VitalsBlockProps) {
  const { size } = usePaneText();
  if (opponentOnly && !combat) return null;
  const line = !opponentOnly && density === 'line' && fit !== 'rows';
  const geometry = geometryAt(vitalsGeometry(options.meter), size);
  const meter = geometry.meter > 0;
  const rows =
    vitals === null
      ? []
      : shownRows(vitals).map((r) =>
          vitals.hidden
            ? {
                key: r.key,
                label: r.label,
                value: hiddenVital(options.values),
                pct: null,
                tone: 'hidden' as const,
              }
            : {
                key: r.key,
                label: r.label,
                value: formatVital(options.values, vitals[r.key], vitals[r.max]),
                pct: meterFill(vitals[r.key], vitals[r.max]),
                tone: vitalTone(
                  vitals[r.key],
                  vitals[r.max],
                  vitals.low[r.key],
                  options.warn_thirds,
                ),
              },
        );

  return (
    <section
      ref={sectionRef}
      className={`panel-vitals${line ? ' is-one-line' : ''}`}
      // The footer holds one row for One line and three for Rows while
      // it waits for your vitals, so logging in moves nothing.
      style={footerStyle(geometry, opponentOnly || density === 'line' ? 1 : 3)}
      aria-label={opponentOnly ? 'Opponent' : 'Vitals'}
    >
      {combat &&
        (combat.hidden ? (
          <VitalRow
            className="panel-vitals-row-combat panel-vitals-row-hidden"
            label={combat.name}
            value="?"
            pct={null}
            meter={meter}
          />
        ) : (
          <VitalRow
            className="panel-vitals-row-combat"
            label={combat.name}
            value={combat.hp_pct !== null ? `${combat.hp_pct}%` : (combat.condition ?? '')}
            pct={combat.hp_pct}
            meter={meter}
          />
        ))}
      {opponentOnly ? null : vitals === null ? (
        <div className="panel-vitals-row">
          <p className="panel-vitals-empty">Vitals appear when you log in.</p>
        </div>
      ) : line ? (
        <div className="panel-vitals-row panel-vitals-oneline">
          {rows.map((r) => (
            <VitalItem
              key={r.key}
              label={r.label}
              showLabel={fit === 'labels'}
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
            label={r.label}
            value={r.value}
            pct={r.pct}
            meter={meter}
          />
        ))
      )}
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

function VitalRow({
  label,
  value,
  pct,
  meter,
  className,
}: {
  label: string;
  value: string;
  pct: number | null;
  meter: boolean;
  className?: string | undefined;
}) {
  return (
    <div className={`panel-vitals-row${className ? ` ${className}` : ''}`}>
      <div className="panel-vitals-line">
        <span className="panel-vitals-label">{label}</span>
        <span className="panel-vitals-value">{value}</span>
      </div>
      {meter && <Meter pct={pct} />}
    </div>
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
}: {
  label: string;
  showLabel: boolean;
  value: string;
  pct: number | null;
  tone: VitalTone;
  meter: boolean;
}) {
  const toned = toneClass(tone);
  return (
    <div className={`panel-vitals-item${showLabel ? '' : ' is-bare'}${toned ? ` ${toned}` : ''}`}>
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
