import { useEffect, useRef, useState, type ReactNode } from 'react';
import {
  DEFAULT_VITALS_CONFIG,
  getUiConfig,
  onGmcpPackage,
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
function CombatChip({ combat }: { combat: CombatState }) {
  const hp = combat.hp;
  const fill = hp !== undefined ? combatHpColor(hp) : '#7aa89f';
  return (
    <div className="vitals-combat">
      <div className="vitals-combat-head">
        <span className="vitals-combat-swords" aria-hidden="true">
          ⚔
        </span>
        <span className="vitals-combat-name">{combat.name}</span>
      </div>
      {hp !== undefined ? (
        <div className="vitals-combat-bar-row">
          <TrackBar value={hp} cells={12} color={fill} />
          <span className="vitals-combat-pct" style={{ color: fill }}>
            {hp}%
          </span>
        </div>
      ) : (
        <div className="vitals-combat-unknown">hp unknown</div>
      )}
      {combat.condition && <div className="vitals-combat-condition">{combat.condition}</div>}
    </div>
  );
}

// Color ramp for the combat target's hp%. Greens at high, red at
// low — same ramp the old status-bar combat seg used so the move
// from one panel to the other doesn't change the visual.
function combatHpColor(value: number): string {
  if (value >= 80) return '#87a987';
  if (value >= 60) return '#e6c384';
  if (value >= 40) return '#d99a6c';
  if (value >= 20) return '#e46876';
  return '#7d1d1d';
}

// CSS-tinted track bar. Renders as a fixed-width div (sized in `ch`
// to match the user's bar_width) with a colored fill that grows
// left-to-right by percent. No glyph dependency — looks smooth at
// any fill percentage and any font, transitions on update. Replaces
// the dropped "ramped" Unicode 1/8-block mode which rendered ugly
// in most monospace fonts.
function TrackBar({ value, cells, color }: { value: number; cells: number; color: string }) {
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
      style={{ flexBasis: `${cells}ch`, maxWidth: `${cells}ch` }}
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
}: {
  value: number;
  cells: number;
  filledGlyph: string;
  emptyGlyph: string;
  color: string;
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
    <span ref={ref} className="vitals-glyphs" style={{ maxWidth: `${cells}ch` }} aria-hidden="true">
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
}: {
  value: number;
  cells: number;
  filledGlyph: string;
  emptyGlyph: string;
  color: string;
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
    <span ref={ref} className="vitals-glyphs" style={{ maxWidth: `${cells}ch` }} aria-hidden="true">
      {wholeFilled > 0 && <span style={{ color }}>{filledGlyph.repeat(wholeFilled)}</span>}
      {boundary && <span style={{ color }}>{boundary}</span>}
      {emptyCount > 0 && <span className="vitals-empty">{emptyGlyph.repeat(emptyCount)}</span>}
    </span>
  );
}

// Stacked vitals — one row per hp/mana/move. Tick and mud time render
// in the LineChip on the input row's top border; this component no
// longer hosts them. Each row is
// `label · bar (20 cells) · % · cur/max · delta`. Subscribes to
// Char.Vitals + World.Time; World.Time hour-change rebases the
// per-tick delta snapshot.
export function VitalsBar() {
  const [vitals, setVitals] = useState<Vitals | null>(null);
  const [deltas, setDeltas] = useState<VitalDeltas>(NO_DELTAS);
  const [config, setConfig] = useState<VitalsConfig>(DEFAULT_VITALS_CONFIG);
  const vitalsSnapRef = useRef<Vitals | null>(null);
  const prevHourRef = useRef<number | string | null>(null);

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
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unsubState = fn;
    });

    return () => {
      cancelled = true;
      unsubVitals?.();
      unsubTime?.();
      unsubState?.();
    };
  }, []);

  const combat = useCombat();

  if (!vitals) return null;

  const segs: Array<{ label: string; cur: number; max: number; delta: number | null }> = [];
  if (vitals.maxhp > 0)
    segs.push({ label: 'hp', cur: vitals.hp, max: vitals.maxhp, delta: deltas.hp });
  if (vitals.maxmana > 0)
    segs.push({ label: 'mn', cur: vitals.mana, max: vitals.maxmana, delta: deltas.mana });
  if (vitals.maxmove > 0)
    segs.push({ label: 'mv', cur: vitals.move, max: vitals.maxmove, delta: deltas.move });
  if (segs.length === 0) return null;

  // Template-driven layout takes priority over the built-in
  // stacked / inline layouts so a user who has authored a custom
  // template can replace the entire row shape without disabling
  // any of the underlying toggles.
  if (config.template_enabled) {
    return <TemplateVitalsRow config={config} vitals={vitals} deltas={deltas} combat={combat} />;
  }

  // Inline layout packs every vital onto a single horizontal row in
  // the tintin nprompt shape:
  //   850(85%)h 230(76%)m 120(60%)v
  if (config.layout === 'inline') {
    return (
      <div className="vitals-bar vitals-bar-inline" aria-label="vitals">
        {combat && (
          <div className="vitals-row vitals-row-combat-top">
            <CombatChip combat={combat} />
          </div>
        )}
        <div className="vitals-row vitals-row-inline">
          {segs.map((s, i) => (
            <InlineVitalChip key={s.label} config={config} compact={i > 0} {...s} />
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className={`vitals-bar${config.show_bar ? '' : ' vitals-bar-no-bar'}`} aria-label="vitals">
      {/* Stacked rows + a side combat chip vertically centered to the
          right of the vitals stack. Without the row-flex wrapper,
          combat would land below the rows; here it sits beside them
          centered against the middle (mana) row. */}
      <div className="vitals-stacked-wrap">
        <div className="vitals-stacked-rows">
          {segs.map((s) => (
            <VitalRow key={s.label} config={config} {...s} />
          ))}
        </div>
        {combat && (
          <div className="vitals-stacked-combat">
            <CombatChip combat={combat} />
          </div>
        )}
      </div>
    </div>
  );
}

// One vital rendered tintin nprompt-style: `850(85%)h`.
// The single-letter label maps from the internal label (hp/mn/mv)
// to its first character, and sits AFTER the percent group so the
// value reads first and the label disambiguates after. Percent color
// follows config.percent_color (per-vital fill or 0–100 gradient).
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
        <span className="vitals-inline-pct" style={{ color: percentColor }}>
          ({value}%)
        </span>
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
}: {
  label: string;
  cur: number;
  max: number;
  delta: number | null;
  config: VitalsConfig;
}) {
  const value = pct(cur, max);
  const fill = colorForVital(label, value, config);
  const total = Math.max(4, Math.min(60, config.bar_width));
  const showDelta = config.show_delta && delta !== null && delta !== 0;
  const deltaPositive = (delta ?? 0) > 0;

  return (
    <div className="vitals-row">
      <span className="vitals-label">{label}</span>
      {config.show_bar &&
        (config.bar_style === 'track' ? (
          <TrackBar value={value} cells={total} color={fill} />
        ) : config.bar_style === 'ramped' ? (
          <RampedBar
            value={value}
            cells={total}
            filledGlyph={config.bar_filled}
            emptyGlyph={config.bar_empty}
            color={fill}
          />
        ) : (
          <SolidBar
            value={value}
            cells={total}
            filledGlyph={config.bar_filled}
            emptyGlyph={config.bar_empty}
            color={fill}
          />
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
}: {
  config: VitalsConfig;
  vitals: Vitals;
  deltas: VitalDeltas;
  combat: CombatState | null;
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
  const segments = tokenizeTemplate(config.template);
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
          return <TrackBar value={v} cells={width} color={fill} />;
        }
        if (config.bar_style === 'ramped') {
          return (
            <RampedBar
              value={v}
              cells={width}
              filledGlyph={config.bar_filled}
              emptyGlyph={config.bar_empty}
              color={fill}
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
        // Pass-through: any %name that matches a field in the merged
        // Char.Vitals + Char.Worth snapshot renders that field's
        // value. So `%xp`, `%gold`, `%align`, `%pos`, etc. all just
        // work without curated logic. If nothing matches, we render
        // the literal `%name` so the user sees their typo instead of
        // a silent drop.
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
            <CombatChip combat={combat} />
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
