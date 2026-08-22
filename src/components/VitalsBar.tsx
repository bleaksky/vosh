import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import {
  DEFAULT_VITALS_CONFIG,
  getUiConfig,
  onGmcpPackage,
  onPromptVars,
  onState,
  subscribeVitalsConfigChanged,
  type VitalsConfig,
} from '../lib/session';
import { useTickState } from '../lib/useTickState';
import { formatMudTime, mudTimeColor, useWorldTime } from '../lib/useWorldTime';
import { useCharStats } from '../lib/useCharStats';
import { useCombat, type CombatState } from '../lib/useCombat';
import { tokenizeTemplate, type TemplateSegment } from '../lib/vitalsTemplate';
import { colorForVital, colorForPercent } from '../lib/vitalsColor';
import { loadFontStack } from '../lib/fontLoader';

interface Vitals {
  hp: number;
  maxhp: number;
  mana: number;
  maxmana: number;
  move: number;
  maxmove: number;
}

interface VitalDeltas {
  hp: number | null;
  mana: number | null;
  move: number | null;
}

const NO_DELTAS: VitalDeltas = { hp: null, mana: null, move: null };

function num(value: unknown, fallback: number): number {
  if (typeof value === 'number' && Number.isFinite(value)) return value;
  if (typeof value === 'string') {
    const n = Number(value);
    if (Number.isFinite(n)) return n;
  }
  return fallback;
}

function pct(current: number, max: number): number {
  if (max <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((current / max) * 100)));
}

// Per-vital color ramp (and the user-override / drain-toggle logic)
// lives in ../lib/vitalsColor so the Settings preview can share the
// same resolution path. See `colorForVital` there for the behavior
// matrix.

// Single-ramp gradient red -> yellow -> green by percent value.
// Mirrors the tintin nprompt @check / @percent color steps so the
// percent text reads as a universal health signal regardless of
// which vital it labels (hp / mn / mv all use the same ramp here).
// Used when VitalsConfig.percent_color === 'gradient'.
// Combat chip — name + hp% track bar + optional condition word.
// Moved here from the status bar so it sits adjacent to the vitals
// (the natural place to glance when something is hitting you).
// Renders nothing when no combat is in progress, so the vitals row
// reads identically when out of combat.
function CombatChip({
  combat,
  config = DEFAULT_VITALS_CONFIG,
}: {
  combat: CombatState;
  config?: VitalsConfig;
  /** Deprecated — kept for callers that still pass it. Use `config`
   *  so the combat bar can pick up bar_style + bar_font together. */
  font?: string;
}) {
  const hp = combat.hp;
  // Combat bar reuses whichever bar_style the user picked for vitals
  // but in a single-hue drain-red palette: bright at full hp, fading
  // through a mid stop to near-black. 12 cells is a compact width
  // that fits inside the chip without dominating the row.
  const fill = hp !== undefined ? combatDrainRed(hp) : combatDrainEmpty(0);
  const cells = 12;
  return (
    <div className="vitals-combat">
      <div className="vitals-combat-line">
        <span className="vitals-combat-swords" aria-hidden="true">
          ⚔
        </span>
        <span className="vitals-combat-name">{combat.name}</span>
        {hp !== undefined ? (
          <>
            {config.bar_style === 'track' ? (
              <TrackBar value={hp} cells={cells} color={fill} font={config.bar_font} />
            ) : config.bar_style === 'ramped' ? (
              <RampedBar
                value={hp}
                cells={cells}
                filledGlyph={config.bar_filled}
                emptyGlyph={config.bar_empty}
                color={fill}
                font={config.bar_font}
              />
            ) : (
              <SolidBar
                value={hp}
                cells={cells}
                filledGlyph={config.bar_filled}
                emptyGlyph={config.bar_empty}
                color={fill}
                font={config.bar_font}
              />
            )}
            <span className="vitals-combat-pct" style={{ color: fill }}>
              {hp}%
            </span>
          </>
        ) : (
          <span className="vitals-combat-unknown">hp unknown</span>
        )}
      </div>
    </div>
  );
}

// Drain ramp for the combat target — bright red at full hp, fading
// through a mid stop to near-black as the opponent dies. Replaces
// the older green/yellow/red threshold ramp; the user wanted a
// single-hue "getting dimmer" curve so the bar reads as "how much
// life left" at a glance.
function combatDrainRed(value: number): string {
  const v = Math.max(0, Math.min(100, value));
  const stops: Array<[number, [number, number, number]]> = [
    [0, [58, 13, 13]],
    [50, [168, 40, 40]],
    [100, [255, 77, 77]],
  ];
  for (let i = 0; i < stops.length - 1; i++) {
    const [a, ac] = stops[i];
    const [b, bc] = stops[i + 1];
    if (v >= a && v <= b) {
      const t = (v - a) / (b - a);
      const r = Math.round(ac[0] + (bc[0] - ac[0]) * t);
      const g = Math.round(ac[1] + (bc[1] - ac[1]) * t);
      const bl = Math.round(ac[2] + (bc[2] - ac[2]) * t);
      return `rgb(${r}, ${g}, ${bl})`;
    }
  }
  return 'rgb(255, 77, 77)';
}

// Empty-trough color for the bar. Always near-black red so the
// trough reads as "missing hp" regardless of the current fill
// brightness.
function combatDrainEmpty(_value: number): string {
  return 'rgb(38, 8, 8)';
}

// Combat pane — standalone panel that renders the combat chip in its
// own zone (top/bottom/left/right) instead of inline next to the
// vitals stack. Returns null when no target is set so the panel
// collapses to zero height out of combat. Pairs with the
// `hideCombat` prop on VitalsBar (App.tsx wires the two together by
// reading panelLayout.placements.combat.zone).
export function CombatPane({ chip = false }: { chip?: boolean } = {}) {
  const combat = useCombat();
  // Read bar_font so the combat target's hp bar uses the same font as
  // the user's vital bars instead of inheriting from the parent panel
  // (whose font may resolve `ch` to a too-small width and collapse
  // the bar to zero).
  const [config, setConfig] = useState<VitalsConfig>(DEFAULT_VITALS_CONFIG);
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    getUiConfig()
      .then((cfg) => {
        if (!cancelled) setConfig(cfg.vitals);
      })
      .catch(() => {});
    subscribeVitalsConfigChanged((next) => {
      if (!cancelled) setConfig(next);
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, []);
  if (!combat) return null;
  return (
    <div className={`combat-pane${chip ? ' combat-pane-chip' : ''}`} aria-label="combat">
      <CombatChip combat={combat} config={config} />
    </div>
  );
}

// CSS-tinted track bar. Renders as a fixed-width div (sized in `ch`
// to match the user's bar_width) with a colored fill that grows
// left-to-right by percent. No glyph dependency — looks smooth at
// any fill percentage and any font, transitions on update. Replaces
// the dropped "ramped" Unicode 1/8-block mode which rendered ugly
// in most monospace fonts.
function TrackBar({
  value,
  cells,
  color,
  font,
  wrapped = false,
}: {
  value: number;
  cells: number;
  color: string;
  font?: string;
  /** When true, this bar is nested inside HistoryWrapper which owns
   *  the cells×ch flex sizing. Skip the inline flex-basis / max-width
   *  so the parent column flex sizes us by cross-axis stretch
   *  instead of treating our cells×ch as our main-axis height. */
  wrapped?: boolean;
}) {
  const pct = Math.max(0, Math.min(100, value));
  // `flex-basis` (not `width`) lets the bar shrink first when the row
  // is narrower than label + bar + readouts. `max-width` caps it at
  // the user's configured cells so a wide panel does not balloon the
  // bar beyond what the user asked for. The inner fill stays a
  // straight `width: ${pct}%`, which reads as a true proportion at
  // whatever the outer bar's actual rendered width turns out to be.
  return (
    <span
      className="vitals-glyphs vitals-glyphs-track"
      style={
        wrapped
          ? { fontFamily: font || undefined }
          : { flexBasis: `${cells}ch`, maxWidth: `${cells}ch`, fontFamily: font || undefined }
      }
      aria-hidden="true"
    >
      <span className="vitals-glyphs-track-fill" style={{ width: `${pct}%`, background: color }} />
    </span>
  );
}

// Solid (glyph) bar with auto-shrink. xterm-style fixed-character
// bars cannot just clip on overflow without lying about the fill
// percentage (a 30/60 bar visible as 20/60 reads as 100% full). So we
// observe the rendered width, measure the actual character cell size,
// and recompute filled / empty counts so the visible bar always
// matches the live percentage at whatever width the row gives us.
function SolidBar({
  value,
  cells,
  filledGlyph,
  emptyGlyph,
  color,
  font,
  wrapped = false,
}: {
  value: number;
  cells: number;
  filledGlyph: string;
  emptyGlyph: string;
  color: string;
  font?: string;
  wrapped?: boolean;
}) {
  const ref = useRef<HTMLSpanElement | null>(null);
  // Start at the configured cell count so the first paint shows the
  // user's intent without a flash of zero glyphs. The ResizeObserver
  // narrows this on the next frame if the row cannot afford the full
  // width.
  const [renderCells, setRenderCells] = useState(cells);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    // Render a single hidden sample of the filled glyph and measure
    // its bounding box. Cached per resize so a font swap propagates
    // through the next observed event. Using a getBoundingClientRect
    // on a real DOM node beats hard-coding "1ch == X px" because the
    // user's font choice can change the glyph's advance width.
    let sample: HTMLSpanElement | null = null;
    const measure = () => {
      if (!ref.current) return;
      if (!sample) {
        sample = document.createElement('span');
        sample.style.visibility = 'hidden';
        sample.style.position = 'absolute';
        sample.style.whiteSpace = 'pre';
        sample.style.letterSpacing = '0';
        sample.style.fontWeight = '700';
        sample.textContent = filledGlyph;
        ref.current.appendChild(sample);
      } else if (sample.parentNode !== ref.current) {
        ref.current.appendChild(sample);
      }
      const charWidth = sample.getBoundingClientRect().width;
      if (charWidth <= 0) return;
      const available = ref.current.clientWidth;
      const fits = Math.max(1, Math.floor(available / charWidth));
      const next = Math.min(cells, fits);
      setRenderCells((prev) => (prev === next ? prev : next));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => {
      observer.disconnect();
      if (sample && sample.parentNode) sample.parentNode.removeChild(sample);
    };
  }, [cells, filledGlyph]);

  const filledCount = Math.round((value / 100) * renderCells);
  const emptyCount = Math.max(0, renderCells - filledCount);
  return (
    <span
      ref={ref}
      className="vitals-glyphs"
      style={
        wrapped
          ? { fontFamily: font || undefined }
          : { maxWidth: `${cells}ch`, fontFamily: font || undefined }
      }
      aria-hidden="true"
    >
      {filledCount > 0 && <span style={{ color }}>{filledGlyph.repeat(filledCount)}</span>}
      {emptyCount > 0 && <span className="vitals-empty">{emptyGlyph.repeat(emptyCount)}</span>}
    </span>
  );
}

// 1/8-step partial block characters for the ramped bar's boundary
// cell. Index 0 means "no partial" (boundary is empty), index 8 means
// "full" (boundary is the filledGlyph itself); indices 1..7 are the
// progressive partial blocks. Lets the bar move at sub-character
// resolution — a 53% bar at width 20 shows 10 full cells + a ▍
// (3/8 partial) for the boundary instead of snapping to 50% or 55%.
const RAMP_PARTIALS = ['', '▏', '▎', '▍', '▌', '▋', '▊', '▉'];

// Ramped bar. Same ResizeObserver-driven cell-fit pattern as SolidBar
// but the boundary cell renders as a partial block keyed off the
// fractional remainder of (value × cells / 100). Looks smooth across
// the full 0..100 range, especially at narrow bar widths where the
// solid variant's 5%-per-cell granularity reads as blocky.
function RampedBar({
  value,
  cells,
  filledGlyph,
  emptyGlyph,
  color,
  font,
  wrapped = false,
}: {
  value: number;
  cells: number;
  filledGlyph: string;
  emptyGlyph: string;
  color: string;
  font?: string;
  wrapped?: boolean;
}) {
  const ref = useRef<HTMLSpanElement | null>(null);
  const [renderCells, setRenderCells] = useState(cells);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    let sample: HTMLSpanElement | null = null;
    const measure = () => {
      if (!ref.current) return;
      if (!sample) {
        sample = document.createElement('span');
        sample.style.visibility = 'hidden';
        sample.style.position = 'absolute';
        sample.style.whiteSpace = 'pre';
        sample.style.letterSpacing = '0';
        sample.style.fontWeight = '700';
        sample.textContent = filledGlyph;
        ref.current.appendChild(sample);
      } else if (sample.parentNode !== ref.current) {
        ref.current.appendChild(sample);
      }
      const charWidth = sample.getBoundingClientRect().width;
      if (charWidth <= 0) return;
      const available = ref.current.clientWidth;
      const fits = Math.max(1, Math.floor(available / charWidth));
      const next = Math.min(cells, fits);
      setRenderCells((prev) => (prev === next ? prev : next));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => {
      observer.disconnect();
      if (sample && sample.parentNode) sample.parentNode.removeChild(sample);
    };
  }, [cells, filledGlyph]);

  const exact = (Math.max(0, Math.min(100, value)) / 100) * renderCells;
  const wholeFilled = Math.floor(exact);
  const fraction = exact - wholeFilled;
  const stepIdx = Math.round(fraction * 8); // 0..8
  let boundary = '';
  if (wholeFilled < renderCells && stepIdx > 0) {
    boundary = stepIdx === 8 ? filledGlyph : RAMP_PARTIALS[stepIdx];
  }
  const emptyCount = Math.max(0, renderCells - wholeFilled - (boundary ? 1 : 0));
  return (
    <span
      ref={ref}
      className="vitals-glyphs"
      style={
        wrapped
          ? { fontFamily: font || undefined }
          : { maxWidth: `${cells}ch`, fontFamily: font || undefined }
      }
      aria-hidden="true"
    >
      {wholeFilled > 0 && <span style={{ color }}>{filledGlyph.repeat(wholeFilled)}</span>}
      {boundary && <span style={{ color }}>{boundary}</span>}
      {emptyCount > 0 && <span className="vitals-empty">{emptyGlyph.repeat(emptyCount)}</span>}
    </span>
  );
}

// HistoryWrapper — `bar_layout: 'with_history'` wraps any bar element
// (SolidBar / TrackBar / RampedBar) in a flex column with a braille
// trend grid below it. The wrapper owns the outer cells×ch sizing
// for the row's flex layout; the inner bar receives a `wrapped` hint
// so it skips its own flex-basis / max-width inline styles (those
// would otherwise set the bar's main-axis height in the column flex,
// blowing the row to 100s of px tall). Grid runs at the bar's font-
// size with -2px letter-spacing, so sparkW = 1.3× cells fits the
// bar's width across the common 12–14px row font range.
// Single-row braille dot bits — one cell carries 2 samples (one per
// dot column) across 4 dot rows of vertical resolution. Pure function
// of two sample values; used by both the live HistoryWrapper and the
// Settings preview to render one cell at a time so flex layout can
// own cell positioning instead of CSS letter-spacing math.
const DOTS_LEFT_TOP_DOWN = [0x01, 0x02, 0x04, 0x40];
const DOTS_RIGHT_TOP_DOWN = [0x08, 0x10, 0x20, 0x80];
const DOTS_PER_COL = 4;
export function brailleCell(v0: number, v1: number): string {
  const lit0 = Math.max(0, Math.min(DOTS_PER_COL, Math.round(v0 * DOTS_PER_COL)));
  const lit1 = Math.max(0, Math.min(DOTS_PER_COL, Math.round(v1 * DOTS_PER_COL)));
  let bits = 0;
  for (let d = 0; d < 4; d++) {
    const fromBottom = DOTS_PER_COL - 1 - d;
    if (fromBottom < lit0) bits |= DOTS_LEFT_TOP_DOWN[d];
    if (fromBottom < lit1) bits |= DOTS_RIGHT_TOP_DOWN[d];
  }
  return String.fromCodePoint(0x2800 | bits);
}

function HistoryWrapper({
  values,
  cells,
  filledGlyph,
  emptyGlyph,
  currentValue,
  color,
  font,
}: {
  values: number[];
  cells: number;
  filledGlyph: string;
  emptyGlyph: string;
  currentValue: number;
  color: string;
  font?: string;
}) {
  // Per-cell flex grid for both bar and trend rows. Each row has
  // exactly `cells` children with `flex: 1 1 0`, so the bar cells
  // and grid cells share identical column boundaries regardless of
  // font metrics or letter-spacing. The bar inside the spark
  // wrapper does NOT use the SolidBar / TrackBar / RampedBar
  // components — those bake in their own ch-based sizing that
  // can't be reconciled with a per-cell-flex grid. Spark mode
  // always renders bar as ▓░ glyphs (or whichever bar_filled /
  // bar_empty the user configured); bar_style branching can come
  // back later if needed.
  const filledCount = Math.max(0, Math.min(cells, Math.round((currentValue / 100) * cells)));
  // Pad history with the oldest known sample (or 0 if empty) so a
  // steady value reads as a fully lit grid rather than empty cells
  // on the left.
  const need = cells * 2;
  const samples = values.slice(-need);
  const pad = samples.length > 0 ? samples[0] : 0;
  while (samples.length < need) samples.unshift(pad);
  return (
    <span
      className="vitals-glyphs vitals-spark"
      style={{
        flexBasis: `${cells}ch`,
        maxWidth: `${cells}ch`,
        fontFamily: font || undefined,
      }}
      aria-hidden="true"
    >
      <span className="vitals-spark-row vitals-spark-bar-row">
        {Array.from({ length: cells }, (_, i) => {
          const filled = i < filledCount;
          return (
            <span key={i} className="vitals-spark-cell" style={filled ? { color } : undefined}>
              {filled ? filledGlyph : emptyGlyph}
            </span>
          );
        })}
      </span>
      <span className="vitals-spark-row vitals-spark-grid-row">
        {Array.from({ length: cells }, (_, i) => (
          <span key={i} className="vitals-spark-cell vitals-spark-grid-cell">
            <span className="vitals-spark-grid-base">⣿</span>
            <span className="vitals-spark-grid-trace" style={{ color }}>
              {brailleCell(samples[i * 2], samples[i * 2 + 1])}
            </span>
          </span>
        ))}
      </span>
    </span>
  );
}

// Pane-head tick readout for the ember layout. Its own leaf so the
// per-second countdown re-renders just this span instead of the
// whole bar (same split as TickSecondsToken in the template path).
// Renders nothing while the tick timer is inactive.
function EmberTick() {
  const { active, tickSecs } = useTickState();
  if (!active) return null;
  return <span className="vitals-ember-tick">tick {tickSecs}s</span>;
}

// Stacked vitals — one row per hp/mana/move. Tick and mud time render
// in the LineChip on the input row's top border; this component no
// longer hosts them. Each row is
// `label · bar (20 cells) · % · cur/max · delta`. Subscribes to
// Char.Vitals + World.Time; World.Time hour-change rebases the
// per-tick delta snapshot.
export function VitalsBar({ hideCombat = false }: { hideCombat?: boolean } = {}) {
  const [gmcpVitals, setVitals] = useState<Vitals | null>(null);
  const [deltas, setDeltas] = useState<VitalDeltas>(NO_DELTAS);
  const [config, setConfig] = useState<VitalsConfig>(DEFAULT_VITALS_CONFIG);
  // Prompt vars — replaces / overlays GMCP values for the
  // template + inline resolvers. Written by user triggers via
  // `mud.set_prompt_var(name, value)` on the Lua side.
  const [promptVars, setPromptVars] = useState<Record<string, string>>({});
  const vitalsSnapRef = useRef<Vitals | null>(null);
  const prevHourRef = useRef<number | string | null>(null);
  // Per-vital history ring buffer used by the spark bar style. Each
  // Char.Vitals event pushes a new percent (0..1) to each ring; the
  // SparkBar slices the last N samples to draw. We use a ref + state
  // counter to update history without re-renders piling up — the
  // existing setVitals re-render is what makes the spark redraw.
  const sparkHistoryRef = useRef<{ hp: number[]; mana: number[]; move: number[] }>({
    hp: [],
    mana: [],
    move: [],
  });

  // Vitals appearance config. Read once on mount then live-updated via
  // vosh://vitals-config-changed so Settings edits land without a
  // relaunch and the bar redraws on the next render.
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    getUiConfig()
      .then((cfg) => {
        if (!cancelled) setConfig(cfg.vitals);
      })
      .catch(() => {});
    subscribeVitalsConfigChanged((next) => {
      if (!cancelled) setConfig(next);
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, []);

  // Inject @font-face for any system font the user names in the bar
  // font stack. macOS WKWebView (since Sonoma) refuses to match user-
  // installed fonts by CSS family name alone, so a plain `font-family:
  // "MonoLisa"` silently falls through to the next font in the stack.
  // loadFontStack walks the stack, skips generics + bundled families,
  // and registers each system family it finds with a runtime @font-face
  // block so the bar's inline style can actually resolve to it.
  useEffect(() => {
    if (config.bar_font) loadFontStack(config.bar_font);
  }, [config.bar_font]);

  useEffect(() => {
    let unsubVitals: (() => void) | undefined;
    let unsubTime: (() => void) | undefined;
    let unsubState: (() => void) | undefined;
    let cancelled = false;

    onGmcpPackage<Record<string, number | undefined>>('Char.Vitals', (data) => {
      const d = data ?? {};
      const next: Vitals = {
        hp: num(d.hp, 0),
        maxhp: num(d.maxhp, 0),
        mana: num(d.mana, 0),
        maxmana: num(d.maxmana, 0),
        move: num(d.move, 0),
        maxmove: num(d.maxmove, 0),
      };
      setVitals(next);
      // Push the current percent into each vital's spark history
      // ring. The ring is capped at 320 samples — sparkW maxes out
      // at 2.5× the user's bar_width (capped at 60 cells), so up to
      // 150 grid cells × 2 samples = 300 samples needed.
      const SPARK_HISTORY_MAX = 320;
      const push = (arr: number[], v: number) => {
        arr.push(v);
        while (arr.length > SPARK_HISTORY_MAX) arr.shift();
      };
      const ring = sparkHistoryRef.current;
      push(ring.hp, next.maxhp > 0 ? next.hp / next.maxhp : 0);
      push(ring.mana, next.maxmana > 0 ? next.mana / next.maxmana : 0);
      push(ring.move, next.maxmove > 0 ? next.move / next.maxmove : 0);
      const snap = vitalsSnapRef.current;
      if (snap === null) {
        vitalsSnapRef.current = next;
        setDeltas(NO_DELTAS);
      } else {
        setDeltas({
          hp: next.hp - snap.hp,
          mana: next.mana - snap.mana,
          move: next.move - snap.move,
        });
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unsubVitals = fn;
    });

    onGmcpPackage<Record<string, unknown>>('World.Time', (data) => {
      if (!data || typeof data !== 'object') return;
      const hour = data.hour as number | string | undefined | null;
      if (hour === undefined || hour === null) return;
      if (prevHourRef.current !== null && prevHourRef.current !== hour) {
        setVitals((curr) => {
          const prevSnap = vitalsSnapRef.current;
          if (curr) {
            if (prevSnap) {
              setDeltas({
                hp: curr.hp - prevSnap.hp,
                mana: curr.mana - prevSnap.mana,
                move: curr.move - prevSnap.move,
              });
            }
            vitalsSnapRef.current = curr;
          }
          return curr;
        });
      }
      prevHourRef.current = hour;
    }).then((fn) => {
      if (cancelled) fn();
      else unsubTime = fn;
    });

    onState((payload) => {
      if (payload.kind === 'disconnected') {
        setVitals(null);
        setDeltas(NO_DELTAS);
        vitalsSnapRef.current = null;
        prevHourRef.current = null;
        setPromptVars({});
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unsubState = fn;
    });

    let unsubPromptVars: (() => void) | undefined;
    onPromptVars((payload) => {
      setPromptVars(payload || {});
    }).then((fn) => {
      if (cancelled) fn();
      else unsubPromptVars = fn;
    });

    return () => {
      cancelled = true;
      unsubVitals?.();
      unsubTime?.();
      unsubState?.();
      unsubPromptVars?.();
    };
  }, []);

  // When the combat panel is in a non-hidden zone (App.tsx passes
  // hideCombat=true in that case), the CombatChip is rendered there
  // instead of inline — null out the local subscription so all four
  // inline `combat && <CombatChip ... />` slots collapse.
  const rawCombat = useCombat();
  const combat = hideCombat ? null : rawCombat;

  // Merge prompt_vars on top of GMCP vitals. Any of the six
  // canonical keys (hp/maxhp/mana/maxmana/move/maxmove) that exist
  // in prompt_vars override the GMCP value, so a `#prompt`-style
  // trigger that calls `mud.set_prompt_var("hp", captures[1])`
  // immediately moves the bar without waiting for a GMCP push.
  // If neither GMCP nor prompt_vars have anything, we render
  // nothing.
  const vitals: Vitals | null = (() => {
    const fromVar = (k: string): number | null => {
      const raw = promptVars[k];
      if (raw === undefined) return null;
      const n = Number(raw);
      return Number.isFinite(n) ? n : null;
    };
    const base: Vitals = gmcpVitals ?? {
      hp: 0,
      maxhp: 0,
      mana: 0,
      maxmana: 0,
      move: 0,
      maxmove: 0,
    };
    const merged: Vitals = {
      hp: fromVar('hp') ?? base.hp,
      maxhp: fromVar('maxhp') ?? base.maxhp,
      mana: fromVar('mana') ?? base.mana,
      maxmana: fromVar('maxmana') ?? base.maxmana,
      move: fromVar('move') ?? base.move,
      maxmove: fromVar('maxmove') ?? base.maxmove,
    };
    if (merged.maxhp === 0 && merged.maxmana === 0 && merged.maxmove === 0 && gmcpVitals === null) {
      return null;
    }
    return merged;
  })();
  if (!vitals) return null;

  const segs: Array<{ label: string; cur: number; max: number; delta: number | null }> = [];
  if (vitals.maxhp > 0)
    segs.push({ label: 'hp', cur: vitals.hp, max: vitals.maxhp, delta: deltas.hp });
  if (vitals.maxmana > 0)
    segs.push({ label: 'mn', cur: vitals.mana, max: vitals.maxmana, delta: deltas.mana });
  if (vitals.maxmove > 0)
    segs.push({ label: 'mv', cur: vitals.move, max: vitals.maxmove, delta: deltas.move });
  if (segs.length === 0) return null;

  // Low HP vignette — when on, render a soft red vignette that
  // pulses at the window periphery whenever hp drops below 30%.
  // Additive — the bar / template / inline layouts keep rendering
  // normally and the overlay sits on top via position:fixed so it
  // does not affect bar layout. Appended at the end of each return
  // branch below.
  const erelei = config.low_hp_vignette ? <LowHpVignetteOverlay segs={segs} /> : null;

  // Template-driven layout takes priority over the built-in
  // stacked / inline layouts so a user who has authored a custom
  // template can replace the entire row shape without disabling
  // any of the underlying toggles.
  if (config.template_enabled) {
    return (
      <>
        <TemplateVitalsRow
          config={config}
          vitals={vitals}
          deltas={deltas}
          combat={combat}
          promptVars={promptVars}
        />
        {erelei}
      </>
    );
  }

  // Ember layout — the sidebar-pane block from the Ember redesign:
  // a pane head (caps "vitals" + live tick countdown while the tick
  // timer runs) over three thin fixed-height track bars with mono
  // cur/max values. Fill colors are fixed per vital (hp success /
  // mn info / mv warn), so the columns, glyph, and width settings
  // do not apply here. Combat chip rides above the rows, same slot
  // the inline layout uses.
  if (config.layout === 'ember') {
    return (
      <>
        <div className="vitals-bar vitals-ember" aria-label="vitals">
          <div className="vitals-ember-head">
            <span className="caps">vitals</span>
            <EmberTick />
          </div>
          {combat && (
            <div className="vitals-row vitals-row-combat-top">
              <CombatChip combat={combat} config={config} />
            </div>
          )}
          <div className="vitals-ember-rows">
            {segs.map((s) => (
              <div key={s.label} className="vitals-ember-row">
                <span className="vitals-ember-label">{s.label}</span>
                <div className="vitals-ember-track">
                  <div
                    className={`vitals-ember-fill vitals-ember-fill-${s.label}`}
                    style={{ width: `${pct(s.cur, s.max)}%` }}
                  />
                </div>
                <span className="vitals-ember-value">
                  {s.cur}/{s.max}
                </span>
              </div>
            ))}
          </div>
        </div>
        {erelei}
      </>
    );
  }

  // Inline layout packs every vital onto a single horizontal row in
  // the tintin nprompt shape:
  //   850(85%)h 230(76%)m 120(60%)v
  if (config.layout === 'inline') {
    // Sub-style switch. `plain` keeps the historical InlineVitalChip
    // text; `underline` adds the colored drain bar beneath each unit;
    // `badge` wraps each vital in a chip with a percent pill (phase 3).
    const renderUnit = (s: (typeof segs)[number], i: number) => {
      if (config.inline_style === 'drain') {
        return <InlineVitalDrain key={s.label} config={config} {...s} />;
      }
      if (config.inline_style === 'badge') {
        return <InlineVitalBadge key={s.label} config={config} {...s} />;
      }
      return <InlineVitalChip key={s.label} config={config} compact={i > 0} {...s} />;
    };
    return (
      <>
        <div
          className={`vitals-bar vitals-bar-inline vitals-inline-${config.inline_style}`}
          aria-label="vitals"
        >
          {combat && (
            <div className="vitals-row vitals-row-combat-top">
              <CombatChip combat={combat} config={config} />
            </div>
          )}
          <div className="vitals-row vitals-row-inline">{segs.map(renderUnit)}</div>
        </div>
        {erelei}
      </>
    );
  }

  return (
    <>
      <div
        className={`vitals-bar${config.show_bar ? '' : ' vitals-bar-no-bar'}`}
        aria-label="vitals"
      >
        {/* Stacked rows + a side combat chip vertically centered to the
            right of the vitals stack. Without the row-flex wrapper,
            combat would land below the rows; here it sits beside them
            centered against the middle (mana) row. */}
        <div className="vitals-stacked-wrap">
          <div className="vitals-stacked-rows">
            {segs.map((s) => (
              <VitalRow
                key={s.label}
                config={config}
                history={
                  s.label === 'hp'
                    ? sparkHistoryRef.current.hp
                    : s.label === 'mn'
                      ? sparkHistoryRef.current.mana
                      : sparkHistoryRef.current.move
                }
                {...s}
              />
            ))}
          </div>
          {combat && (
            <div className="vitals-stacked-combat">
              <CombatChip combat={combat} config={config} />
            </div>
          )}
        </div>
      </div>
      {erelei}
    </>
  );
}

// One vital rendered tintin nprompt-style: `850(85%)h`.
// The single-letter label maps from the internal label (hp/mn/mv)
// to its first character, and sits AFTER the percent group so the
// value reads first and the label disambiguates after. Percent color
// follows config.percent_color (per-vital fill or 0–100 gradient).
// Resolve the percent text color for the underline / badge inline
// styles. `stat` mirrors the per-stat numeric color (so numeric +
// percent read as one block); `drain` keeps the percent quiet
// while things are fine and ramps through warn → orange → red as
// the value drops, so the percent itself becomes the alarm
// signal; `accent` forces the brand pink at every fill.
//
// In `drain` mode the high-health color is text-faint (not the
// stat color) so percent and numeric stay visually distinct
// during the calm. Only when things go wrong does percent start
// to color, and it ramps independently of the stat hue.
function resolvePercentColor(label: string, value: number, config: VitalsConfig): string {
  switch (config.percent_color_mode) {
    case 'stat':
      return colorForVital(label, 100, config);
    case 'accent':
      return 'var(--c-accent)';
    case 'drain':
    default:
      if (value >= 75) return 'var(--c-text-faint)';
      if (value >= 50) return '#e6c384';
      if (value >= 25) return '#d99268';
      return '#e46876';
  }
}

// Renders a percent value either as historical `(NN%)` text or
// wrapped in a small styled chip per `config.pct_chip_style`. Used
// by the plain inline layout (where the percent currently sits in
// parens) and by the `%pct_*` template expansion so the user can
// drop the chip anywhere they place the token in their template.
function PercentChip({
  value,
  color,
  style,
}: {
  value: number;
  color: string;
  style: VitalsConfig['pct_chip_style'];
}) {
  if (style === 'none') {
    return (
      <span className="vitals-inline-pct" style={{ color }}>
        ({value}%)
      </span>
    );
  }
  const styleVars: React.CSSProperties = {
    color,
    ['--pct-color' as string]: color,
    ['--pct-fill' as string]: `${Math.max(0, Math.min(100, value))}%`,
  };
  return (
    <span className={`vitals-pct-chip vitals-pct-chip-${style}`} style={styleVars}>
      {value}%
    </span>
  );
}

// Inline `drain` style. Each vital is a chip with caption upper-
// left, percent upper-right (color-shifted by percent_color_mode),
// and a big numeric value on the main line. Behind the text, a
// drain background fills inside a small inset from the chip
// border — the bar gets room to breathe so it never touches the
// edges. The leading edge of the drain carries a soft color-matched
// glow that pops the "current value" boundary without adding hard
// chrome. Matches mockup C from vosh-vitals-chip-scale.html.
function InlineVitalDrain({
  label,
  cur,
  max,
  delta,
  config,
}: {
  label: string;
  cur: number;
  max: number;
  delta: number | null;
  config: VitalsConfig;
}) {
  const value = pct(cur, max);
  const numericColor = colorForVital(label, 100, config);
  const percentColor = resolvePercentColor(label, value, config);
  const showDelta = config.show_delta && delta !== null && delta !== 0;
  const deltaPositive = (delta ?? 0) > 0;
  const isDanger = value < 20;
  return (
    <span
      className={`vitals-inline-drain${isDanger ? ' is-danger' : ''}`}
      style={
        {
          ['--vital-color']: numericColor,
          ['--vital-fill']: `${Math.max(0, Math.min(100, value))}%`,
        } as React.CSSProperties
      }
    >
      <span className="vitals-inline-drain-bg" aria-hidden="true" />
      <span className="vitals-inline-drain-top">
        <span className="vitals-inline-drain-cap">{label}</span>
        {config.show_percent && (
          <span className="vitals-inline-drain-pct" style={{ color: percentColor }}>
            {value}%
          </span>
        )}
      </span>
      <span className="vitals-inline-drain-val" style={{ color: numericColor }}>
        {config.show_numeric ? (
          <>
            {cur}
            <span className="vitals-inline-drain-max">/{max}</span>
          </>
        ) : (
          `${value}%`
        )}
        {showDelta && (
          <span
            className={`vitals-delta${deltaPositive ? ' vitals-delta-up' : ' vitals-delta-down'}`}
          >
            {deltaPositive ? '+' : ''}
            {delta}
          </span>
        )}
      </span>
    </span>
  );
}

// Inline `badge` style. Main chip uses the same chrome as the
// tick + time chips today (border-strong outline, surface-deep
// fill, 3px radius, inset highlight). Inside: caption + numeric
// value tinted per stat. The percent rides as its own small
// bordered pill that hangs outside the upper-right corner.
function InlineVitalBadge({
  label,
  cur,
  max,
  delta,
  config,
}: {
  label: string;
  cur: number;
  max: number;
  delta: number | null;
  config: VitalsConfig;
}) {
  const value = pct(cur, max);
  const numericColor = colorForVital(label, 100, config);
  const percentColor = resolvePercentColor(label, value, config);
  const showDelta = config.show_delta && delta !== null && delta !== 0;
  const deltaPositive = (delta ?? 0) > 0;
  const isDanger = value < 20;
  return (
    <span
      className={`vitals-inline-badge${isDanger ? ' is-danger' : ''}`}
      style={{ color: numericColor }}
    >
      <span className="vitals-inline-badge-cap">{label}</span>
      {(config.show_numeric || !config.show_percent) && (
        <span className="vitals-inline-badge-val" style={{ color: numericColor }}>
          {cur}
          <span className="vitals-inline-badge-max">/{max}</span>
        </span>
      )}
      {config.show_percent && (
        <span
          className="vitals-inline-badge-pct"
          style={{ color: percentColor, borderColor: isDanger ? 'var(--c-danger)' : undefined }}
        >
          {value}%
        </span>
      )}
      {showDelta && (
        <span
          className={`vitals-delta${deltaPositive ? ' vitals-delta-up' : ' vitals-delta-down'}`}
        >
          {deltaPositive ? '+' : ''}
          {delta}
        </span>
      )}
    </span>
  );
}

function InlineVitalChip({
  label,
  cur,
  max,
  delta,
  config,
}: {
  label: string;
  cur: number;
  max: number;
  delta: number | null;
  config: VitalsConfig;
  compact?: boolean;
}) {
  const value = pct(cur, max);
  const percentColor =
    config.percent_color === 'gradient'
      ? colorForPercent(value)
      : colorForVital(label, value, config);
  const showDelta = config.show_delta && delta !== null && delta !== 0;
  const deltaPositive = (delta ?? 0) > 0;
  // Single-letter label per tintin: hp -> h, mn -> m, mv -> v. Any
  // future vital can supply its own first-letter convention here.
  const letter = label[0] ?? '?';
  return (
    <span className="vitals-inline-chip">
      {config.show_numeric && <span className="vitals-numeric">{cur}</span>}
      {config.show_percent && (
        <PercentChip value={value} color={percentColor} style={config.pct_chip_style} />
      )}
      <span className="vitals-inline-letter">{letter}</span>
      {!config.show_numeric && !config.show_percent && (
        // Degenerate config: at least show the value so the chip
        // does not collapse to just a single letter.
        <span className="vitals-numeric">{cur}</span>
      )}
      {showDelta && (
        <span
          className={`vitals-delta${deltaPositive ? ' vitals-delta-up' : ' vitals-delta-down'}`}
        >
          {deltaPositive ? '+' : ''}
          {delta}
        </span>
      )}
    </span>
  );
}

function VitalRow({
  label,
  cur,
  max,
  delta,
  config,
  history,
}: {
  label: string;
  cur: number;
  max: number;
  delta: number | null;
  config: VitalsConfig;
  history: number[];
}) {
  const value = pct(cur, max);
  const fill = colorForVital(label, value, config);
  const total = Math.max(4, Math.min(60, config.bar_width));
  const showDelta = config.show_delta && delta !== null && delta !== 0;
  const deltaPositive = (delta ?? 0) > 0;

  // Compose bar style × layout. For `bar_layout: 'plain'` we pick a
  // bar component by bar_style (track / ramped / solid). For
  // `with_history` we use the per-cell-flex HistoryWrapper which
  // ALWAYS renders the bar as solid ▓░ glyphs (or whatever the user
  // set bar_filled / bar_empty to) — track/ramped variants don't
  // survive the per-cell layout cleanly and would need their own
  // rendering paths. The trade-off is worth it for guaranteed
  // alignment between the bar row and the trend grid row below.
  const withHistory = config.bar_layout === 'with_history';
  const barElement =
    config.bar_style === 'track' ? (
      <TrackBar value={value} cells={total} color={fill} font={config.bar_font} />
    ) : config.bar_style === 'ramped' ? (
      <RampedBar
        value={value}
        cells={total}
        filledGlyph={config.bar_filled}
        emptyGlyph={config.bar_empty}
        color={fill}
        font={config.bar_font}
      />
    ) : (
      <SolidBar
        value={value}
        cells={total}
        filledGlyph={config.bar_filled}
        emptyGlyph={config.bar_empty}
        color={fill}
        font={config.bar_font}
      />
    );

  return (
    <div className={`vitals-row${withHistory ? ' vitals-row-spark' : ''}`}>
      <span className="vitals-label">{label}</span>
      {config.show_bar &&
        (withHistory ? (
          <HistoryWrapper
            values={history}
            cells={total}
            filledGlyph={config.bar_filled}
            emptyGlyph={config.bar_empty}
            currentValue={value}
            color={fill}
            font={config.bar_font}
          />
        ) : (
          barElement
        ))}
      {config.show_percent && (
        <span
          className="vitals-percent"
          style={{
            color: config.percent_color === 'gradient' ? colorForPercent(value) : fill,
          }}
        >
          {value}%
        </span>
      )}
      {config.show_numeric && (
        <span className="vitals-numeric">
          {cur}/{max}
        </span>
      )}
      {config.show_delta && (
        <span className="vitals-delta-slot">
          {showDelta && (
            <span
              className={`vitals-delta${deltaPositive ? ' vitals-delta-up' : ' vitals-delta-down'}`}
            >
              {deltaPositive ? '+' : ''}
              {delta}
            </span>
          )}
        </span>
      )}
    </div>
  );
}

// Template-driven row. Tokenizes the user's template string and emits
// one React node per segment. Text segments render verbatim; token
// segments resolve to live vital values (with appropriate coloring
// for percent / bar tokens). Unknown tokens fall through as literal
// `%foo` text so a typo is visible instead of silently disappearing.
function TemplateVitalsRow({
  config,
  vitals,
  deltas,
  combat,
  promptVars,
}: {
  config: VitalsConfig;
  vitals: Vitals;
  deltas: VitalDeltas;
  combat: CombatState | null;
  promptVars: Record<string, string>;
}) {
  // Phase 6 perf fix: TemplateVitalsRow no longer subscribes to
  // tick state. The `%tick` token resolves to a `<TickSecondsToken
  // />` leaf that subscribes itself, so the whole template row no
  // longer re-renders four times per second just to advance the
  // countdown — only the tiny leaf does. `%time` already follows
  // the same pattern via `useWorldTime` which only fires on the
  // (rare) World.Time push.
  const worldTime = useWorldTime();
  const mudTime = formatMudTime(worldTime);
  const timeColor = mudTimeColor(worldTime);
  // Full merged snapshot of Char.Vitals + Char.Worth so the template
  // can interpolate ANY field the server pushes (xp, gold, alignment,
  // position, ...) — not just the curated hp/mn/mv tokens.
  const stats = useCharStats();
  // Re-tokenize only when the template string changes, not on every
  // vitals push.
  const segments = useMemo(() => tokenizeTemplate(config.template), [config.template]);
  const gradient = config.percent_color === 'gradient';
  const width = Math.max(4, Math.min(60, config.bar_width));

  // Resolve the token name to a React node. Returns null for tokens
  // whose source value isn't available (e.g. %time before World.Time
  // pushes, %tick when the backend tick is disabled) so the rest of
  // the template renders cleanly without empty parens or stray dashes.
  const renderToken = (name: string): ReactNode => {
    const valueFor = (cur: number, max: number) => Math.round(pct(cur, max));
    switch (name) {
      case 'hp':
        return vitals.hp;
      case 'mhp':
        return vitals.maxhp;
      case 'mn':
        return vitals.mana;
      case 'mmn':
        return vitals.maxmana;
      case 'mv':
        return vitals.move;
      case 'mmv':
        return vitals.maxmove;
      case 'pct_hp':
      case 'pct_mn':
      case 'pct_mv': {
        const label = name.slice(4); // 'hp' | 'mn' | 'mv'
        const cur = label === 'hp' ? vitals.hp : label === 'mn' ? vitals.mana : vitals.move;
        const max =
          label === 'hp' ? vitals.maxhp : label === 'mn' ? vitals.maxmana : vitals.maxmove;
        const v = valueFor(cur, max);
        const color = gradient ? colorForPercent(v) : colorForVital(label, v, config);
        // When a chip style is configured, wrap the token in a
        // chip so the user can place chip-styled percents anywhere
        // in their template. The flex-baseline host span mirrors
        // what the inline plain layout already does via
        // .vitals-inline-chip — without it the bare inline-block
        // chip floats above the template row's baseline because
        // its line-height computes against the chip's smaller font
        // and ends up taller than the surrounding text.
        if (config.pct_chip_style !== 'none') {
          return (
            <span className="vitals-pct-chip-host">
              <PercentChip value={v} color={color} style={config.pct_chip_style} />
            </span>
          );
        }
        return <span style={{ color }}>{v}%</span>;
      }
      case 'dhp':
      case 'dmn':
      case 'dmv': {
        const label = name.slice(1); // 'hp' | 'mn' | 'mv'
        const d = label === 'hp' ? deltas.hp : label === 'mn' ? deltas.mana : deltas.move;
        if (d === null || d === 0) return null;
        const cls = d > 0 ? 'vitals-delta vitals-delta-up' : 'vitals-delta vitals-delta-down';
        return (
          <span className={cls}>
            {d > 0 ? '+' : ''}
            {d}
          </span>
        );
      }
      case 'bar_hp':
      case 'bar_mn':
      case 'bar_mv': {
        const label = name.slice(4);
        const cur = label === 'hp' ? vitals.hp : label === 'mn' ? vitals.mana : vitals.move;
        const max =
          label === 'hp' ? vitals.maxhp : label === 'mn' ? vitals.maxmana : vitals.maxmove;
        const v = valueFor(cur, max);
        const fill = colorForVital(label, v, config);
        if (config.bar_style === 'track') {
          return <TrackBar value={v} cells={width} color={fill} font={config.bar_font} />;
        }
        if (config.bar_style === 'ramped') {
          return (
            <RampedBar
              value={v}
              cells={width}
              filledGlyph={config.bar_filled}
              emptyGlyph={config.bar_empty}
              color={fill}
              font={config.bar_font}
            />
          );
        }
        return (
          <SolidBar
            value={v}
            cells={width}
            filledGlyph={config.bar_filled}
            emptyGlyph={config.bar_empty}
            color={fill}
            font={config.bar_font}
          />
        );
      }
      case 'tick':
        return <TickSecondsToken />;
      case 'time':
        return mudTime !== null ? (
          <span style={timeColor ? { color: timeColor } : undefined}>{mudTime}</span>
        ) : null;
      default: {
        // Resolution priority: prompt_vars (written by user
        // triggers) override GMCP fields, so a `#prompt`-style
        // trigger can repaint `%anything` without needing the
        // server to push a matching package. After that, fall
        // back to the merged Char.Vitals + Char.Worth snapshot so
        // `%xp`, `%gold`, `%align`, `%pos`, etc. still work
        // without curated logic. Unknown tokens render as the red
        // `%name` literal so typos surface instead of dropping
        // silently.
        if (Object.prototype.hasOwnProperty.call(promptVars, name)) {
          return promptVars[name];
        }
        const fallback = stats[name];
        if (fallback === undefined || fallback === null) {
          return <span className="vitals-template-unknown">%{name}</span>;
        }
        return String(fallback);
      }
    }
  };

  // Bar tokens (%bar_hp / %bar_mn / %bar_mv) are fixed-width inline
  // spans. When a template puts them on a line that does not fit, the
  // browser wraps at the bar boundary which strands the trailing
  // tokens (`%pct_hp %dhp`) on their own line. To wrap cleanly, we
  // detect a template with bar tokens, split it on newlines, and
  // render each line as a flex row where the bar absorbs the
  // remaining width and the surrounding text segments hold their
  // natural width. Templates without bars keep the original inline
  // pre-wrap rendering so users who care about exact whitespace are
  // not disturbed.
  const hasBarToken = segments.some(
    (s) =>
      s.kind === 'token' && (s.name === 'bar_hp' || s.name === 'bar_mn' || s.name === 'bar_mv'),
  );

  if (!hasBarToken) {
    return (
      <div className="vitals-bar vitals-bar-template" aria-label="vitals">
        {combat && (
          <div className="vitals-row vitals-row-combat-top">
            <CombatChip combat={combat} config={config} />
          </div>
        )}
        <div className="vitals-row vitals-row-template">
          {segments.map((seg, i) => (
            <TemplateSegmentNode key={i} segment={seg} renderToken={renderToken} />
          ))}
        </div>
      </div>
    );
  }

  const lines = splitTemplateLines(segments);
  return (
    <div className="vitals-bar vitals-bar-template" aria-label="vitals">
      {combat && (
        <div className="vitals-row vitals-row-combat-top">
          <CombatChip combat={combat} />
        </div>
      )}
      {lines.map((lineSegments, i) => (
        <TemplateLineFlexRow key={i} segments={lineSegments} renderToken={renderToken} />
      ))}
    </div>
  );
}

// Tiny leaf that displays just the running tick countdown. Defined
// as its own component so the parent template row can stop
// subscribing to useTickState; only this leaf re-renders on each
// second-advance, instead of every `%hp / %pct_mn / %bar_mv / ...`
// segment in the user's template being walked four times per
// second just because the countdown advanced.
function TickSecondsToken() {
  const { active, tickSecs } = useTickState();
  if (!active) return null;
  return <>{tickSecs}s</>;
}

function TemplateSegmentNode({
  segment,
  renderToken,
}: {
  segment: TemplateSegment;
  renderToken: (name: string) => ReactNode;
}) {
  if (segment.kind === 'text') return <>{segment.text}</>;
  const node = renderToken(segment.name);
  if (node === null || node === undefined) {
    // Resolve to empty string so the row doesn't contain stale parens
    // around a missing value. We deliberately drop adjacent text too?
    // No — keep adjacent text intact; user can use cmd-style escaping
    // if they need parens to also disappear when the value is missing.
    return null;
  }
  return <>{node}</>;
}

/**
 * Split a tokenized template into a list of lines. `\n` inside text
 * segments starts a new line; tokens stay on whichever line their
 * surrounding text put them on. Empty lines (two consecutive `\n`)
 * survive as empty arrays so the rendered output keeps the same
 * vertical spacing the user typed.
 */
function splitTemplateLines(segments: TemplateSegment[]): TemplateSegment[][] {
  const lines: TemplateSegment[][] = [[]];
  for (const seg of segments) {
    if (seg.kind === 'text') {
      const parts = seg.text.split('\n');
      for (let i = 0; i < parts.length; i++) {
        if (i > 0) lines.push([]);
        if (parts[i].length > 0) {
          lines[lines.length - 1].push({ kind: 'text', text: parts[i] });
        }
      }
    } else {
      lines[lines.length - 1].push(seg);
    }
  }
  return lines;
}

/**
 * Render one line of a bar-containing template as a flex row. Text
 * segments and non-bar tokens render in fixed-width flex cells so
 * their natural width is preserved; bar tokens render in a flex-
 * shrinkable cell so they give up width first when the line is
 * narrow. The line's `min-width: 0` lets the bar shrink below its
 * own cell content via the SolidBar / TrackBar `max-width: ${cells}ch`
 * cap rather than overflowing the row.
 */
function TemplateLineFlexRow({
  segments,
  renderToken,
}: {
  segments: TemplateSegment[];
  renderToken: (name: string) => ReactNode;
}) {
  return (
    <div className="vitals-row vitals-row-template-line">
      {segments.map((seg, i) => {
        if (seg.kind === 'text') {
          return (
            <span key={i} className="vitals-template-cell">
              {seg.text}
            </span>
          );
        }
        const node = renderToken(seg.name);
        if (node === null || node === undefined) return null;
        const isBar = seg.name === 'bar_hp' || seg.name === 'bar_mn' || seg.name === 'bar_mv';
        const cls = isBar ? 'vitals-template-bar-cell' : 'vitals-template-cell';
        return (
          <span key={i} className={cls}>
            {node}
          </span>
        );
      })}
    </div>
  );
}

// Low HP vignette — soft red peripheral glow that pulses on the
// window edges when hp drops below 30%. Vignette opacity ramps
// linearly from 0 (at hp=30) to 0.6 (at hp=0). Sits on top of the
// regular bar via position:fixed so it does not affect layout.
function LowHpVignetteOverlay({
  segs,
}: {
  segs: Array<{ label: string; cur: number; max: number; delta: number | null }>;
}) {
  const hpSeg = segs.find((s) => s.label === 'hp');
  const hp = hpSeg ? pct(hpSeg.cur, hpSeg.max) : 100;
  const danger = hp >= 30 ? 0 : ((30 - hp) / 30) * 0.6;
  return (
    <div
      className="low-hp-vignette"
      style={{ '--lhv-danger': danger.toFixed(3) } as React.CSSProperties}
      aria-hidden="true"
    >
      <span className="lhv-pulse" />
    </div>
  );
}
