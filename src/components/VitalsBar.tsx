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

// Each vital has its own healthy-color identity (hp green, mn
// blue, mv warm-orange) but every ramp converges on red as the
// bar drains, so the eye picks up both "which vital" and "how
// urgent" from the color alone. Intermediate stops avoid muddy
// straight-line interpolation through unflattering midpoints.
type Stop = [pct: number, rgb: [number, number, number]];

const VITAL_RAMPS: Record<string, Stop[]> = {
  // green -> yellow -> orange -> red
  hp: [
    [0, [228, 104, 118]],
    [25, [217, 154, 108]],
    [55, [230, 195, 132]],
    [100, [135, 169, 135]],
  ],
  // blue -> teal -> purple -> red
  mn: [
    [0, [228, 104, 118]],
    [30, [192, 130, 168]],
    [65, [134, 153, 188]],
    [100, [127, 180, 202]],
  ],
  // orange -> deep red
  mv: [
    [0, [228, 104, 118]],
    [60, [220, 130, 100]],
    [100, [217, 154, 108]],
  ],
};

// Single-ramp gradient red -> yellow -> green by percent value.
// Mirrors the tintin nprompt @check / @percent color steps so the
// percent text reads as a universal health signal regardless of
// which vital it labels (hp / mn / mv all use the same ramp here).
// Used when VitalsConfig.percent_color === 'gradient'.
function colorForPercent(value: number): string {
  const v = Math.max(0, Math.min(100, value));
  if (v < 20) return '#dc4444';
  if (v < 40) return '#dc8a44';
  if (v < 60) return '#dccd44';
  if (v < 80) return '#a8dc44';
  return '#5fdc6a';
}

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
  return (
    <span
      className="vitals-glyphs vitals-glyphs-track"
      style={{ width: `${cells}ch` }}
      aria-hidden="true"
    >
      <span className="vitals-glyphs-track-fill" style={{ width: `${pct}%`, background: color }} />
    </span>
  );
}

function colorForVital(label: string, value: number): string {
  const ramp = VITAL_RAMPS[label] ?? VITAL_RAMPS.hp;
  const v = Math.max(0, Math.min(100, value));
  let lower = ramp[0];
  let upper = ramp[ramp.length - 1];
  for (let i = 0; i < ramp.length - 1; i++) {
    if (v >= ramp[i][0] && v <= ramp[i + 1][0]) {
      lower = ramp[i];
      upper = ramp[i + 1];
      break;
    }
  }
  const range = upper[0] - lower[0];
  const t = range > 0 ? (v - lower[0]) / range : 0;
  const lerp = (a: number, b: number) => Math.round(a + (b - a) * t);
  const r = lerp(lower[1][0], upper[1][0]);
  const g = lerp(lower[1][1], upper[1][1]);
  const b = lerp(lower[1][2], upper[1][2]);
  return `rgb(${r}, ${g}, ${b})`;
}

// Stacked vitals — one row per hp/mana/move. The tick countdown
// is a separate panel (TickPanel below) so the user can hide vitals
// without losing the tick or vice versa. Each row is
// `label · bar (20 cells) · % · cur/max · delta`. Subscribes to
// Char.Vitals + World.Time; World.Time hour-change rebases the
// per-tick delta snapshot.
export function VitalsBar({
  embedTick = false,
  embedTime = false,
}: { embedTick?: boolean; embedTime?: boolean } = {}) {
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
    return (
      <TemplateVitalsRow
        config={config}
        vitals={vitals}
        deltas={deltas}
        combat={combat}
        embedTick={embedTick}
        embedTime={embedTime}
      />
    );
  }

  // Inline layout packs every vital onto a single horizontal row in
  // the tintin nprompt shape:
  //   850(85%)h 230(76%)m 120(60%)v - (12s) - 3PM
  // Single-letter label AFTER the percent, no space between value
  // and paren. The tick and MUD-time chips appear at the end as
  // `- (...)` clusters when enabled.
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
          {embedTick && (
            <span className="vitals-inline-tail">
              <span className="vitals-inline-tail-sep">-</span>
              <InlineTick className="vitals-inline-tick" />
            </span>
          )}
          {embedTime && (
            <span className="vitals-inline-tail">
              <span className="vitals-inline-tail-sep">-</span>
              <InlineMudTime className="vitals-inline-time" />
            </span>
          )}
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
      {(embedTick || embedTime) && <VitalsTailRow embedTick={embedTick} embedTime={embedTime} />}
    </div>
  );
}

// Compact, label-less tail row at the bottom of the stacked vitals
// layout. Mirrors tintin's nprompt `- (12s) - 3PM` cluster: just the
// tick countdown and/or MUD time, right-aligned, dim. Renders
// nothing when both sources are inactive so the bar does not
// reserve an empty slot.
function VitalsTailRow({ embedTick, embedTime }: { embedTick: boolean; embedTime: boolean }) {
  const { active, tickSecs } = useTickState();
  const worldTime = useWorldTime();
  const mudTime = formatMudTime(worldTime);
  const timeColor = mudTimeColor(worldTime);
  const showTick = embedTick && active;
  const showTime = embedTime && mudTime !== null;
  if (!showTick && !showTime) return null;
  return (
    <div className="vitals-row-tail" aria-hidden="true">
      {showTick && <span className="vitals-row-tail-chip">({tickSecs}s)</span>}
      {showTick && showTime && <span className="vitals-row-tail-sep">·</span>}
      {showTime && (
        <span
          className="vitals-row-tail-chip vitals-row-tail-time"
          style={timeColor ? { color: timeColor } : undefined}
        >
          {mudTime}
        </span>
      )}
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
    config.percent_color === 'gradient' ? colorForPercent(value) : colorForVital(label, value);
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

// Inline MUD time chip. Reads the GMCP World.Time push and renders
// the current in-game hour (formatted 3PM / 12AM). Used by any host
// panel that the user picks via the `time` panel's `in:*` zone
// (vitals / roomstrip / affects / statusbar). Returns null until the
// server pushes a parseable time.
export function InlineMudTime({ className }: { className?: string }) {
  const worldTime = useWorldTime();
  const formatted = formatMudTime(worldTime);
  const color = mudTimeColor(worldTime);
  if (!formatted) return null;
  return (
    <span
      className={`inline-mud-time${className ? ` ${className}` : ''}`}
      style={color ? { color } : undefined}
      aria-label="MUD time"
    >
      {formatted}
    </span>
  );
}

// Inline tick chip rendered at the right edge of a host panel
// (vitals, roomstrip, affects, statusbar) when the user picks one
// of the `in:*` zones for the tick. Returns null when inactive so
// hosts can drop it in unconditionally.
export function InlineTick({ className }: { className?: string }) {
  const { active, tickSecs, warn } = useTickState();
  if (!active) return null;
  return (
    <span className={`inline-tick${className ? ` ${className}` : ''}`} aria-label="tick">
      <span className="vitals-label">tick</span>
      <span className={`vitals-tick-value${warn ? ' is-warn' : ''}`}>{tickSecs}s</span>
    </span>
  );
}

// Stand-alone MUD time panel. Mirrors TickPanel: lives in the panel
// registry as its own movable element so the user can place the
// in-game time anywhere a panel can go (or embed it inline). Wall-
// clock stays in the bottom-right StatusBar slot regardless of
// where this panel is placed.
export function TimePanel() {
  const worldTime = useWorldTime();
  const formatted = formatMudTime(worldTime);
  const color = mudTimeColor(worldTime);
  if (!formatted) return null;
  return (
    <div className="vitals-bar time-panel" aria-label="mud time">
      <div className="vitals-row">
        <span className="vitals-label">time</span>
        <span className="inline-mud-time" style={color ? { color } : undefined}>
          {formatted}
        </span>
      </div>
    </div>
  );
}

// Stand-alone tick countdown panel. Reuses the vitals-bar grid so it
// renders as a single "tick · 12s" row consistent with the hp/mn/mv
// rows. Lives in the panel registry as its own movable element so the
// user can hide vitals without losing the tick.
export function TickPanel() {
  const { active, tickSecs, warn } = useTickState();
  if (!active) return null;
  return (
    <div className="vitals-bar tick-panel" aria-label="tick">
      <TickRow tickSecs={tickSecs} warn={warn} />
    </div>
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
  const fill = colorForVital(label, value);
  const total = Math.max(4, Math.min(60, config.bar_width));
  const track = config.bar_style === 'track';
  const filledCount = Math.round((value / 100) * total);
  const emptyCount = total - filledCount;
  const showDelta = config.show_delta && delta !== null && delta !== 0;
  const deltaPositive = (delta ?? 0) > 0;

  return (
    <div className="vitals-row">
      <span className="vitals-label">{label}</span>
      {config.show_bar &&
        (track ? (
          <TrackBar value={value} cells={total} color={fill} />
        ) : (
          <span className="vitals-glyphs" aria-hidden="true">
            {filledCount > 0 && (
              <span style={{ color: fill }}>{config.bar_filled.repeat(filledCount)}</span>
            )}
            {emptyCount > 0 && (
              <span className="vitals-empty">{config.bar_empty.repeat(emptyCount)}</span>
            )}
          </span>
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

function TickRow({ tickSecs, warn }: { tickSecs: number; warn: boolean }) {
  return (
    <div className="vitals-row">
      <span className="vitals-label">tick</span>
      <span className={`vitals-tick-value${warn ? ' is-warn' : ''}`}>{tickSecs}s</span>
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
  embedTick,
  embedTime,
}: {
  config: VitalsConfig;
  vitals: Vitals;
  deltas: VitalDeltas;
  combat: CombatState | null;
  embedTick: boolean;
  embedTime: boolean;
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
        const color = gradient ? colorForPercent(v) : colorForVital(label, v);
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
        const fill = colorForVital(label, v);
        if (config.bar_style === 'track') {
          return <TrackBar value={v} cells={width} color={fill} />;
        }
        const filled = Math.round((v / 100) * width);
        const empty = width - filled;
        return (
          <span className="vitals-glyphs" aria-hidden="true">
            <span style={{ color: fill }}>{config.bar_filled.repeat(filled)}</span>
            <span className="vitals-empty">{config.bar_empty.repeat(empty)}</span>
          </span>
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
        {embedTick && <InlineTick className="vitals-inline-tick" />}
        {embedTime && <InlineMudTime className="vitals-inline-time" />}
      </div>
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
