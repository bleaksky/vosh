import { useRef, type CSSProperties, type ReactNode } from 'react';
import type { CombatOpponent } from '../stores/gmcp/combatStore';
import {
  formatVital,
  opponentHealth,
  VITAL_LABELS,
  type ShownVital,
  type VitalInks,
  type VitalTone,
} from '../panel/vitalsView';
import { VisuallyHidden } from '../ui';
import { SunPathIcon } from './icons';
import { MoonPhaseIcon } from './MoonPhaseIcon';
import type { ClockMoons, ClockTick, ClockTime } from './StatusClock';
import { roundTripText, roundTripTone, type RoundTripTone } from './roundTrip';
import { partsGone, tickShare, useGiveWay, BAR_GIVE_WAY, type BarStyle } from './statusBarFit';

// The Strip, Dashboard and Meters styles of the status bar, drawn from
// plain values so a test can render every case. StatusLine reads the
// stores and hands them in. Compact stays in StatusLine.
//
// Every style shows the same things. Not connected while you are not,
// then your character and the room while the bar leaves your vitals to
// the panel, or your vitals, your opponent and your target while it
// carries them, or your vitals text in the Text style. Then the tick,
// the game time, the moons and the round trip to the game. They differ
// in how they draw.
//
// Strip sits on the raised ground, 32 px tall. Each vital reads its
// label, a gauge and its value, and the tick a bar that fills toward
// the next one. The round trip shows as signal bars.
//
// Dashboard is 44 px tall, cut into segments by hairlines. Each
// segment holds a caption over its value, and your vitals and your
// opponent draw a thin bar under theirs.
//
// Meters fills the whole bar, 32 px tall. Your vitals, your opponent
// and the tick each take a zone that fills with a tint as wide as the
// value, with a 2 px edge along the bottom. The room takes the zones
// while the bar leaves your vitals to the panel.
//
// Each vital takes the color you picked under Customize vitals, or the
// play palette's red, blue and green, lifted on the ground as the panel
// lifts them. Low Health reads in the danger tone and your opponent in
// the warn tone. The tick turns the warn tone in its last seconds. Each
// style lets parts go as it runs short, in the order statusBarFit.ts
// lists.

export interface BarItems {
  /** Your vitals text, drawn by the caller, while the bar writes it in
   *  the Text style. */
  text: ReactNode;
  rows: readonly ShownVital[];
  /** Your opponent in a fight. */
  foe: CombatOpponent | null;
  /** A target on no mob the bar already names. */
  target: string | null;
}

export interface BarProps {
  style: BarStyle;
  connected: boolean;
  /** Your character, or null before you log in. */
  character: string | null;
  /** Where you are, shown while the bar leaves your vitals to the
   *  panel, or null. */
  room: { name: string; area: string | null } | null;
  items: BarItems;
  /** The color of each vital's label, gauge and zone. */
  inks: Required<VitalInks>;
  tick: ClockTick | null;
  time: ClockTime | null;
  moons: ClockMoons | null;
  /** The round trip to the game in ms, or null before the first
   *  reading. */
  roundTrip: number | null;
  /** Changes when the font the bar draws in changes, so it fits again. */
  faceVersion?: number;
}

export function StatusBar(props: BarProps) {
  const ref = useRef<HTMLSpanElement | null>(null);
  const level = useGiveWay(ref, BAR_GIVE_WAY[props.style].length, barKey(props));
  const gone = partsGone(props.style, level);
  const Style = STYLES[props.style];
  return (
    <span ref={ref} className={`shell-bar is-${props.style}`}>
      <Style {...props} gone={gone} />
    </span>
  );
}

/** What the bar draws, apart from the values that change each second. */
function barKey(p: BarProps): string {
  return [
    p.style,
    p.connected,
    p.character,
    p.room?.name,
    p.room?.area,
    p.items.text ? 'text' : '',
    p.items.rows.map((row) => `${row.key}${row.widest}`).join(),
    p.items.foe?.name,
    p.items.target,
    p.tick ? 'tick' : '',
    p.time?.text.length,
    p.moons?.moons.length,
    p.moons?.alignment,
    p.roundTrip === null ? '' : roundTripTone(p.roundTrip),
    p.faceVersion,
  ].join('|');
}

type StyleProps = BarProps & { gone: ReadonlySet<string> };

const STYLES: Record<BarStyle, (props: StyleProps) => ReactNode> = {
  strip: Strip,
  dashboard: Dashboard,
  meters: Meters,
};

const NOT_CONNECTED = 'Not connected';

/** A label the bar let go, still read by a screen reader. */
function Shown({ shown, children }: { shown: boolean; children: ReactNode }) {
  return shown ? <>{children}</> : <VisuallyHidden>{children}</VisuallyHidden>;
}

/** A vital's value in its tone's class. */
function toneClass(tone: VitalTone): string {
  return tone === 'quiet' ? '' : ` is-${tone === 'danger' ? 'low' : tone}`;
}

/** The value of a vital or the `?` of a hidden one. */
function vitalTitle(row: ShownVital): string {
  return row.tone === 'hidden'
    ? `${VITAL_LABELS[row.key]} hidden`
    : `${VITAL_LABELS[row.key]} ${formatVital('current-max', row.current, row.max)}`;
}

/** The tick as it reads: `14s`, or `−5s` below zero, and its label. */
function tickParts(tick: ClockTick): {
  label: string;
  text: string;
  spoken: string | undefined;
  share: number;
} {
  const count = tick.count ?? 'up';
  const text = tick.secs >= 0 ? `${tick.secs}s` : `−${-tick.secs}s`;
  return {
    label: count === 'up' ? 'Tick' : 'Next tick',
    text,
    spoken: tick.secs >= 0 ? undefined : `minus ${-tick.secs}s`,
    share: tickShare(tick.secs, tick.interval, count) ?? 0,
  };
}

function TickValue({ text, spoken }: { text: string; spoken: string | undefined }) {
  return spoken === undefined ? (
    <>{text}</>
  ) : (
    <>
      <span aria-hidden="true">{text}</span>
      <VisuallyHidden>{spoken}</VisuallyHidden>
    </>
  );
}

function tickClass(tick: ClockTick): string {
  return `${tick.warn ? ' is-warn' : ''}${tick.warn && tick.overdue ? ' is-overdue' : ''}`;
}

/** The moons at 14 px, 4 apart, then the sky's word in the warn tone. */
function MoonsRow({ moons }: { moons: ClockMoons }) {
  return (
    <span className="shell-bar-moons" title={moons.moons.map((moon) => moon.label).join(', ')}>
      <VisuallyHidden>Moons</VisuallyHidden>
      {moons.moons.map((moon) => (
        <MoonPhaseIcon
          key={moon.name}
          phase={moon.phase}
          color={moon.color}
          label={moon.label}
          onLight={moons.onLight === true}
        />
      ))}
      {moons.alignment && <span className="shell-bar-alignment">{moons.alignment}</span>}
    </span>
  );
}

/** The game time after the sun on its path, in its daylight tint. */
function TimeRow({ time, word }: { time: ClockTime; word: boolean }) {
  const day = time.daytime === null ? null : time.daytime ? 'day' : 'night';
  return (
    <span className="shell-bar-time" title="Game time">
      <SunPathIcon hour={time.hour} daytime={time.daytime} />
      <span className="shell-bar-value" style={time.tint ? { color: time.tint } : undefined}>
        {time.text}
      </span>
      {day && (
        <Shown shown={word}>
          <span className="shell-bar-dim">{day}</span>
        </Shown>
      )}
    </span>
  );
}

function rttClass(tone: RoundTripTone): string {
  return tone === 'fine' ? '' : ` is-${tone}`;
}

/** The round trip as a dot, or as signal bars, three while fine, two
 *  from 300 ms and one from a second, then the reading. */
function RoundTripRow({ ms, bars = false }: { ms: number; bars?: boolean }) {
  const tone = roundTripTone(ms);
  const lit = tone === 'fine' ? 3 : tone === 'warn' ? 2 : 1;
  return (
    <span className={`shell-bar-rtt${rttClass(tone)}`} title="Round trip to the game">
      {bars ? (
        <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
          {[0, 1, 2].map((i) => (
            <rect
              key={i}
              x={1 + i * 3.8}
              y={8 - i * 3}
              width="2.4"
              height={3 + i * 3}
              rx="0.6"
              className={i < lit ? 'is-lit' : undefined}
            />
          ))}
        </svg>
      ) : (
        <svg width="8" height="8" viewBox="0 0 8 8" aria-hidden="true">
          <circle cx="4" cy="4" r="3" />
        </svg>
      )}
      <span>{roundTripText(ms)}</span>
    </span>
  );
}

/** Your opponent's health as it reads, and its share for a bar. */
function foeHealth(foe: CombatOpponent): { value: string; hidden: boolean; pct: number } {
  const health = opponentHealth(foe);
  return { value: health.value, hidden: health.hidden, pct: health.pct ?? 0 };
}

function inkStyle(ink: string): CSSProperties {
  return { '--bar-ink': ink } as CSSProperties;
}

/* ── Strip ──────────────────────────────────────────────────────────── */

function Strip(p: StyleProps) {
  const { items, gone } = p;
  const tick = p.tick ? tickParts(p.tick) : null;
  const gauge = gone.has('narrow') ? 28 : 44;
  return (
    <>
      {!p.connected && <span className="shell-bar-item">{NOT_CONNECTED}</span>}
      {p.character && !gone.has('name') && (
        <span className="shell-bar-item shell-bar-who" title={p.character}>
          <span className="shell-bar-dot" aria-hidden="true" />
          <span className="shell-bar-value is-strong">{p.character}</span>
        </span>
      )}
      {p.room && (
        <span className="shell-bar-item shell-bar-room" title={roomTitle(p.room)}>
          <span className="shell-bar-value">{p.room.name}</span>
          {p.room.area && (
            <Shown shown={!gone.has('area')}>
              <span className="shell-bar-dim">{p.room.area}</span>
            </Shown>
          )}
        </span>
      )}
      {items.text && (
        <>
          <Seam />
          {items.text}
        </>
      )}
      {items.rows.map((row) => (
        <span key={row.key} className="shell-bar-group">
          <Seam />
          <span
            className="shell-bar-item shell-bar-vital"
            style={inkStyle(p.inks[row.key])}
            title={vitalTitle(row)}
          >
            <Shown shown={!gone.has('labels')}>{VITAL_LABELS[row.key]}</Shown>
            {!gone.has('gauges') && (
              <span className="shell-bar-gauge" style={{ width: gauge }} aria-hidden="true">
                <span style={{ width: `${row.pct ?? 0}%` }} />
              </span>
            )}
            <span className={`shell-bar-value${toneClass(row.tone)}`}>{row.value}</span>
          </span>
        </span>
      ))}
      {items.foe && (
        <span className="shell-bar-group shell-bar-foe-group">
          <Seam />
          <FoeInline foe={items.foe} name={!gone.has('foeName')} />
        </span>
      )}
      {items.target && <TargetInline name={items.target} />}
      <span className="shell-bar-spring" />
      {p.tick && tick && (
        <span className={`shell-bar-item shell-bar-tick${tickClass(p.tick)}`} title={tick.label}>
          <Shown shown={!gone.has('labels')}>{tick.label}</Shown>
          {!gone.has('gauges') && (
            <span className="shell-bar-gauge is-tick" aria-hidden="true">
              <span style={{ width: `${tick.share * 100}%` }} />
            </span>
          )}
          <span className="shell-bar-value shell-bar-secs">
            <TickValue text={tick.text} spoken={tick.spoken} />
          </span>
        </span>
      )}
      {p.time && !gone.has('time') && (
        <>
          <Seam />
          <span className="shell-bar-item">
            <TimeRow time={p.time} word={!gone.has('dayWord')} />
          </span>
        </>
      )}
      {p.moons && p.moons.moons.length > 0 && !gone.has('moons') && (
        <>
          <Seam />
          <span className="shell-bar-item">
            <MoonsRow moons={p.moons} />
          </span>
        </>
      )}
      {p.roundTrip !== null && keepsRoundTrip(p.roundTrip, gone) && (
        <>
          <Seam />
          <span className="shell-bar-item">
            <RoundTripRow ms={p.roundTrip} bars />
          </span>
        </>
      )}
    </>
  );
}

function Seam() {
  return <span className="shell-bar-seam" aria-hidden="true" />;
}

function roomTitle(room: { name: string; area: string | null }): string {
  return room.area ? `${room.name}, ${room.area}` : room.name;
}

/** A slow round trip never gives way. */
function keepsRoundTrip(ms: number, gone: ReadonlySet<string>): boolean {
  return !gone.has('roundTrip') || roundTripTone(ms) !== 'fine';
}

function FoeInline({ foe, name }: { foe: CombatOpponent; name: boolean }) {
  const health = foeHealth(foe);
  return (
    <span className="shell-bar-item shell-bar-foe" title={foe.name}>
      <Shown shown={name}>
        <span className="shell-bar-name">{foe.name}</span>
      </Shown>
      <span className={`shell-bar-value${health.hidden ? ' is-hidden' : ' is-warn'}`}>
        {health.value}
      </span>
    </span>
  );
}

function TargetInline({ name }: { name: string }) {
  return (
    <span className="shell-bar-item shell-bar-target" title={`Target ${name}`}>
      Target<span className="shell-bar-value shell-bar-name">{name}</span>
    </span>
  );
}

/* ── Dashboard ──────────────────────────────────────────────────────── */

function Segment({
  caption,
  children,
  className = '',
  title,
  style,
  captionShown = true,
}: {
  caption: string;
  children: ReactNode;
  className?: string;
  title?: string;
  style?: CSSProperties;
  captionShown?: boolean;
}) {
  return (
    <span className={`shell-bar-seg${className}`} title={title} style={style}>
      <span className="shell-bar-cap">
        <Shown shown={captionShown}>{caption}</Shown>
      </span>
      <span className="shell-bar-val">{children}</span>
    </span>
  );
}

function Dashboard(p: StyleProps) {
  const { items, gone } = p;
  const tick = p.tick ? tickParts(p.tick) : null;
  const day = p.time?.daytime === null || !p.time ? 'Time' : p.time.daytime ? 'Day' : 'Night';
  return (
    <>
      {!p.connected && <Segment caption="Status">{NOT_CONNECTED}</Segment>}
      {p.character && !gone.has('name') && (
        <Segment caption="Playing" title={p.character}>
          <span className="shell-bar-dot" aria-hidden="true" />
          {p.character}
        </Segment>
      )}
      {p.room && (
        <Segment
          caption={p.room.area ?? 'Room'}
          captionShown={!gone.has('area')}
          className=" is-shrink"
          title={roomTitle(p.room)}
        >
          <span className="shell-bar-name">{p.room.name}</span>
        </Segment>
      )}
      {items.text && <span className="shell-bar-seg is-shrink">{items.text}</span>}
      {items.rows.map((row) => (
        <Segment
          key={row.key}
          caption={VITAL_LABELS[row.key]}
          className=" shell-bar-vital is-metered"
          title={vitalTitle(row)}
          style={inkStyle(p.inks[row.key])}
        >
          <span className={`shell-bar-value${toneClass(row.tone)}`}>{row.value}</span>
          <span className="shell-bar-under" aria-hidden="true">
            <span style={{ width: `${row.pct ?? 0}%` }} />
          </span>
        </Segment>
      ))}
      {items.foe && <FoeSegment foe={items.foe} name={!gone.has('foeName')} />}
      {items.target && (
        <Segment caption="Target" className=" is-shrink shell-bar-target" title={items.target}>
          <span className="shell-bar-name">{items.target}</span>
        </Segment>
      )}
      <span className="shell-bar-spring" />
      {p.tick && tick && (
        <Segment
          caption={tick.label}
          className={` is-right shell-bar-tick${tickClass(p.tick)}`}
          title={tick.label}
        >
          <TickRing share={tick.share} />
          <span className="shell-bar-value shell-bar-secs">
            <TickValue text={tick.text} spoken={tick.spoken} />
          </span>
        </Segment>
      )}
      {p.time && !gone.has('time') && (
        <Segment caption={day} className=" is-right" title="Game time">
          <span
            className="shell-bar-value"
            style={p.time.tint ? { color: p.time.tint } : undefined}
          >
            {p.time.text}
          </span>
        </Segment>
      )}
      {p.moons && p.moons.moons.length > 0 && !gone.has('moons') && (
        <Segment caption="Moons" className=" is-right">
          <MoonsRow moons={p.moons} />
        </Segment>
      )}
      {p.roundTrip !== null && keepsRoundTrip(p.roundTrip, gone) && (
        <Segment
          caption="Round trip"
          className={` is-right shell-bar-rtt${rttClass(roundTripTone(p.roundTrip))}`}
          title="Round trip to the game"
        >
          {roundTripText(p.roundTrip)}
        </Segment>
      )}
    </>
  );
}

function FoeSegment({ foe, name }: { foe: CombatOpponent; name: boolean }) {
  const health = foeHealth(foe);
  return (
    <span className="shell-bar-seg shell-bar-foe is-metered is-shrink" title={foe.name}>
      <span className="shell-bar-cap shell-bar-name">
        <Shown shown={name}>{foe.name}</Shown>
      </span>
      <span className="shell-bar-val">
        <span className={`shell-bar-value${health.hidden ? ' is-hidden' : ' is-warn'}`}>
          {health.value}
        </span>
        <span className="shell-bar-under is-foe" aria-hidden="true">
          <span style={{ width: `${health.pct}%` }} />
        </span>
      </span>
    </span>
  );
}

/** A 14 px ring that fills clockwise with the share of the tick gone. */
function TickRing({ share }: { share: number }) {
  const r = 5.5;
  const around = 2 * Math.PI * r;
  return (
    <svg className="shell-bar-ring" width="14" height="14" viewBox="0 0 14 14" aria-hidden="true">
      <circle cx="7" cy="7" r={r} className="is-track" />
      <circle
        cx="7"
        cy="7"
        r={r}
        strokeDasharray={`${around * share} ${around}`}
        transform="rotate(-90 7 7)"
      />
    </svg>
  );
}

/* ── Meters ─────────────────────────────────────────────────────────── */

function Zone({
  className,
  label,
  labelShown = true,
  value,
  valueClass = '',
  fill,
  ink,
  title,
}: {
  className: string;
  label: ReactNode;
  labelShown?: boolean;
  value: ReactNode;
  valueClass?: string;
  /** The tint's width in percent, or null for a zone with no meter. */
  fill: number | null;
  ink?: string;
  title?: string;
}) {
  return (
    <span
      className={`shell-bar-zone ${className}`}
      style={ink ? inkStyle(ink) : undefined}
      title={title}
    >
      {fill !== null && (
        <span className="shell-bar-fill" style={{ width: `${fill}%` }} aria-hidden="true" />
      )}
      <span className="shell-bar-label">
        <Shown shown={labelShown}>{label}</Shown>
      </span>
      <span className={`shell-bar-value${valueClass}`}>{value}</span>
    </span>
  );
}

function Meters(p: StyleProps) {
  const { items, gone } = p;
  const tick = p.tick ? tickParts(p.tick) : null;
  const labels = !gone.has('labels');
  const time = p.time && !gone.has('time') ? <TimeRow time={p.time} word={false} /> : null;
  const moons =
    p.moons && p.moons.moons.length > 0 && !gone.has('moons') ? <MoonsRow moons={p.moons} /> : null;
  const roundTrip =
    p.roundTrip !== null && keepsRoundTrip(p.roundTrip, gone) ? (
      <RoundTripRow ms={p.roundTrip} />
    ) : null;
  return (
    <>
      {!p.connected && <Zone className="is-note" label={NOT_CONNECTED} value={null} fill={null} />}
      {p.room && (
        <Zone
          className="shell-bar-room is-wide"
          label={p.room.name}
          value={p.room.area}
          valueClass=" is-dim"
          fill={null}
          title={roomTitle(p.room)}
        />
      )}
      {items.text && <span className="shell-bar-zone is-wide is-text">{items.text}</span>}
      {items.rows.map((row) => (
        <Zone
          key={row.key}
          className={`shell-bar-vital${row.tone === 'danger' ? ' is-low' : ''}`}
          label={VITAL_LABELS[row.key]}
          labelShown={labels}
          value={row.value}
          valueClass={toneClass(row.tone)}
          fill={row.pct ?? 0}
          ink={p.inks[row.key]}
          title={vitalTitle(row)}
        />
      ))}
      {items.foe && <FoeZone foe={items.foe} name={!gone.has('foeName')} />}
      {items.target && (
        <Zone
          className="shell-bar-target is-shrink"
          label="Target"
          labelShown={labels}
          value={<span className="shell-bar-name">{items.target}</span>}
          fill={null}
          title={`Target ${items.target}`}
        />
      )}
      {p.tick && tick && (
        <Zone
          className={`shell-bar-tick${tickClass(p.tick)}`}
          label={tick.label}
          labelShown={labels}
          value={<TickValue text={tick.text} spoken={tick.spoken} />}
          valueClass=" shell-bar-secs"
          fill={tick.share * 100}
          title={tick.label}
        />
      )}
      {(time || moons || roundTrip) && (
        <span className="shell-bar-zone is-cluster">
          {time}
          {moons}
          {roundTrip}
        </span>
      )}
    </>
  );
}

function FoeZone({ foe, name }: { foe: CombatOpponent; name: boolean }) {
  const health = foeHealth(foe);
  return (
    <Zone
      className="shell-bar-foe"
      label={<span className="shell-bar-name">{foe.name}</span>}
      labelShown={name}
      value={health.value}
      valueClass={health.hidden ? ' is-hidden' : ' is-warn'}
      fill={health.pct}
      title={foe.name}
    />
  );
}
