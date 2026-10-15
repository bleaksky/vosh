import { useRef } from 'react';
import type { StatsStyle } from '../../ipc/uiConfig';
import { useGiveWay } from '../../lib/useGiveWay';
import { useStatsStyle } from '../../stores/config/statsStyleStore';
import {
  ARMOR_KEYS,
  SAVE_KEYS,
  STAT_KEYS,
  useStats,
  type Stats,
  type StatValue,
} from '../../stores/gmcp/statsStore';
import { VisuallyHidden } from '../../ui';
import { PaneHeader } from '../PaneHeader';
import { usePanelFaceVersion } from '../panelFace';
import { usePaneText } from '../paneTextSize';
import {
  ARMOR_NAMES,
  SAVE_NAMES,
  STAT_NAMES,
  STAT_SHORT,
  armorTitle,
  armorTone,
  saveTitle,
  saveTone,
  signed,
  statFill,
  statTitle,
  statTone,
  toneClass,
  trainedAt,
} from './statsView';

// The numbers on your score sheet, from Char.Stats, in the style you
// pick from the pane menu or in Settings, Layout, Stats.
//
// Score sheet lays them out in two columns as `score` does, each stat
// written the game's way, trained then current in brackets. Rows gives
// each number a row, like the Group pane, with a 48 px meter. Ledger
// draws big figures with a caption over each, like the Ledger vitals.
// All three color a stat above what you trained green and one below it
// yellow, armor that counts under 80% yellow and under 50% red, and a
// save above zero red, as the score sheet does. A stat meter fills from
// 3, the lowest a stat goes, to its max, with a tick where you trained
// it. `^1` marks points past the max, which count for nothing.
//
// Each style gives way as the pane narrows (useGiveWay), before the
// frame paints, until nothing is cut. Rows drops its meters and writes
// each stat in three letters, then drops the max and the percents after
// the values. Ledger sets the five stats in three columns, then drops
// their max, then takes two columns, then one. Score sheet stacks its
// two columns into one by itself, then shows each stat's current value
// alone and drops the percents. Whatever goes still reads on the
// pointer, from each row's title.

export function StatsPane() {
  const stats = useStats();
  const style = useStatsStyle();
  return (
    <>
      <PaneHeader />
      {stats === null ? (
        <div className="pane-body">
          <p className="pane-empty">Stats appear when you log in.</p>
        </div>
      ) : (
        <StatsBody stats={stats} style={style} />
      )}
    </>
  );
}

/** The pane's body for `stats` in `style`, so each style renders in a
 *  test. */
export function StatsBody({ stats, style }: { stats: Stats; style: StatsStyle }) {
  if (style === 'rows') return <Rows stats={stats} />;
  if (style === 'ledger') return <Ledger stats={stats} />;
  return <Sheet stats={stats} />;
}

/** What a style draws that can change its width: each number's digits,
 *  the panel text size and the panel face. */
function useWidthKey(stats: Stats): string {
  const { size } = usePaneText();
  const face = usePanelFaceVersion();
  const nums = [
    ...STAT_KEYS.map((k) => {
      const s = stats.stats[k];
      return s ? `${s.cur},${s.max},${s.over}` : '';
    }),
    ...ARMOR_KEYS.map((k) => `${stats.armor[k]?.ac ?? ''},${stats.armor[k]?.pct ?? ''}`),
    ...SAVE_KEYS.map((k) => String(stats.saves[k] ?? '')),
    stats.hitroll,
    stats.hitrollPct,
    stats.damroll,
  ];
  return `${size}|${face}|${nums.join('|')}`;
}

/** Whether any name or figure in `node` is cut short. */
function anyCut(node: HTMLElement): boolean {
  return [...node.querySelectorAll<HTMLElement>('[data-fit]')].some(
    (el) => el.scrollWidth > el.clientWidth,
  );
}

/** The armor classes the packet carries, in the score sheet's order. */
function armorRows(stats: Stats) {
  return ARMOR_KEYS.flatMap((k) => {
    const a = stats.armor[k];
    return a ? [{ k, ...a }] : [];
  });
}

/** The saves the packet carries, in the score sheet's order. */
function saveRows(stats: Stats) {
  return SAVE_KEYS.flatMap((k) => {
    const v = stats.saves[k];
    return v === null ? [] : [{ k, v }];
  });
}

/** Hitroll as the pointer reads it. */
function hitrollTitle(hitroll: number, pct: number | null): string | undefined {
  return pct === null ? undefined : `Hitroll ${hitroll}, ${pct}% of it counts`;
}

function Luck({ bar, of }: { bar: number; of: number }) {
  return (
    <span className="pane-stats-luck" role="img" aria-label={`Luck ${bar} of ${of}`}>
      {Array.from({ length: of }, (_, i) => (
        <i key={i} className={i < bar ? 'is-lit' : undefined} />
      ))}
    </span>
  );
}

/** Points past the max, the way the score sheet marks them. */
function Over({ over, className }: { over: number; className: string }) {
  if (over <= 0) return null;
  return (
    <span className={className}>
      <span aria-hidden="true">^{over}</span>
      <VisuallyHidden>{`, ${over} past the max`}</VisuallyHidden>
    </span>
  );
}

function StatMeter({ s }: { s: StatValue }) {
  return (
    <span className="pane-stats-meter" aria-hidden="true">
      <span
        className={`pane-stats-fill${toneClass(statTone(s))}`}
        style={{ width: `${statFill(s)}%` }}
      />
      <span className="pane-stats-mark" style={{ left: `${trainedAt(s)}%` }} />
    </span>
  );
}

function ShareMeter({ pct, tone }: { pct: number; tone: string }) {
  return (
    <span className="pane-stats-meter" aria-hidden="true">
      <span className={`pane-stats-fill${tone}`} style={{ width: `${pct}%` }} />
    </span>
  );
}

/* ── Rows ──────────────────────────────────────────────────────────── */

function Rows({ stats }: { stats: Stats }) {
  const ref = useRef<HTMLDivElement | null>(null);
  const level = useGiveWay(ref, 2, useWidthKey(stats), anyCut);
  const bare = level > 0;
  const list = `pane-stats-rows${bare ? ' is-bare' : ''}${level > 1 ? ' is-tight' : ''}`;
  const { luck, hitroll, hitrollPct, damroll } = stats;
  const armor = armorRows(stats);
  const saves = saveRows(stats);
  return (
    <div ref={ref} className="pane-body pane-stats-body">
      <ul className={list}>
        {STAT_KEYS.map((k) => {
          const s = stats.stats[k];
          if (!s) return null;
          return (
            <li key={k} className="pane-stats-row" title={statTitle(k, s)}>
              <span className="pane-stats-row-name" data-fit="">
                {bare ? STAT_SHORT[k] : STAT_NAMES[k]}
                <Over over={s.over} className="pane-stats-row-note is-warn" />
              </span>
              <StatMeter s={s} />
              <span className={`pane-stats-row-value${toneClass(statTone(s))}`}>{s.cur}</span>
              <span className="pane-stats-row-aux">/ {s.max}</span>
            </li>
          );
        })}
        {luck && (
          <li className="pane-stats-row">
            <span className="pane-stats-row-name" data-fit="">
              Luck
            </span>
            <span className="pane-stats-row-luck">
              <Luck {...luck} />
            </span>
          </li>
        )}
      </ul>
      {(hitroll !== null || damroll !== null) && (
        <>
          <h3 className="pane-stats-caption">Combat</h3>
          <ul className={list}>
            {hitroll !== null && (
              <li className="pane-stats-row" title={hitrollTitle(hitroll, hitrollPct)}>
                <span className="pane-stats-row-name" data-fit="">
                  Hitroll
                </span>
                {hitrollPct === null ? <span /> : <ShareMeter pct={hitrollPct} tone="" />}
                <span className="pane-stats-row-value">{hitroll}</span>
                <span className="pane-stats-row-aux">
                  {hitrollPct === null ? '' : `${hitrollPct}%`}
                </span>
              </li>
            )}
            {damroll !== null && (
              <li className="pane-stats-row is-span">
                <span className="pane-stats-row-name" data-fit="">
                  Damroll
                </span>
                <span className="pane-stats-row-value">{damroll}</span>
              </li>
            )}
          </ul>
        </>
      )}
      {armor.length > 0 && (
        <>
          <h3 className="pane-stats-caption">Armor</h3>
          <ul className={list}>
            {armor.map(({ k, ac, pct }) => {
              const tone = toneClass(armorTone(pct));
              return (
                <li key={k} className="pane-stats-row" title={armorTitle(k, ac, pct)}>
                  <span className="pane-stats-row-name" data-fit="">
                    {ARMOR_NAMES[k]}
                  </span>
                  {pct === null ? <span /> : <ShareMeter pct={pct} tone={tone} />}
                  <span className="pane-stats-row-value">{signed(ac)}</span>
                  <span className={`pane-stats-row-aux${tone}`}>
                    {pct === null ? '' : `${pct}%`}
                  </span>
                </li>
              );
            })}
          </ul>
        </>
      )}
      {saves.length > 0 && (
        <>
          <h3 className="pane-stats-caption">Saves</h3>
          <ul className={list}>
            {saves.map(({ k, v }) => (
              <li key={k} className="pane-stats-row is-span" title={saveTitle(k, v)}>
                <span className="pane-stats-row-name" data-fit="">
                  {SAVE_NAMES[k]}
                </span>
                <span className={`pane-stats-row-value${toneClass(saveTone(v))}`}>{signed(v)}</span>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}

/* ── Ledger ────────────────────────────────────────────────────────── */

/** Ledger's fit: the five stats across with their max, then in three
 *  columns, then in three without their max, then in two, then one
 *  under another with their max again. */
const LEDGER_FITS = ['full', 'wrap', 'bare', 'pair', 'single'] as const;

function Cell({
  label,
  over = 0,
  figure,
  small,
  smallClass = '',
  tone = '',
  line,
  title,
}: {
  label: string;
  over?: number;
  /** A number, or a mark such as the luck bar, which sits in the
   *  figure's line on its own. */
  figure: number | string | React.ReactElement;
  small?: React.ReactNode;
  smallClass?: string;
  tone?: string;
  line?: React.ReactNode;
  title?: string | undefined;
}) {
  return (
    <div className="pane-stats-cell" title={title}>
      <span className="pane-stats-cell-label">
        <span data-fit="">{label}</span>
        <Over over={over} className="pane-stats-cell-over" />
      </span>
      <span className="pane-stats-cell-figure" data-fit="">
        {typeof figure === 'object' ? (
          figure
        ) : (
          <span className={`pane-stats-cell-current${tone}`}>{figure}</span>
        )}
        {small !== undefined && (
          <span className={`pane-stats-cell-small${smallClass}`}>{small}</span>
        )}
      </span>
      {line}
    </div>
  );
}

function Ledger({ stats }: { stats: Stats }) {
  const ref = useRef<HTMLDivElement | null>(null);
  const level = useGiveWay(ref, LEDGER_FITS.length - 1, useWidthKey(stats), anyCut);
  const fit = LEDGER_FITS[level];
  const { luck, hitroll, hitrollPct, damroll } = stats;
  const armor = armorRows(stats);
  const saves = saveRows(stats);
  return (
    <div ref={ref} className="pane-body pane-stats-body">
      <div className={`pane-stats-ledger is-stats is-${fit}`}>
        {STAT_KEYS.map((k) => {
          const s = stats.stats[k];
          if (!s) return null;
          const tone = toneClass(statTone(s));
          return (
            <Cell
              key={k}
              label={STAT_SHORT[k]}
              over={s.over}
              figure={s.cur}
              small={`/ ${s.max}`}
              smallClass=" is-max"
              tone={tone}
              title={statTitle(k, s)}
              line={
                <span className="pane-stats-line" aria-hidden="true">
                  <span className={`pane-stats-fill${tone}`} style={{ width: `${statFill(s)}%` }} />
                  <span className="pane-stats-mark" style={{ left: `${trainedAt(s)}%` }} />
                </span>
              }
            />
          );
        })}
      </div>
      {(luck || hitroll !== null || damroll !== null) && (
        <div className="pane-stats-ledger is-combat">
          {luck && <Cell label="Luck" figure={<Luck {...luck} />} />}
          {hitroll !== null && (
            <Cell
              label="Hitroll"
              figure={hitroll}
              small={hitrollPct === null ? undefined : `${hitrollPct}%`}
              title={hitrollTitle(hitroll, hitrollPct)}
            />
          )}
          {damroll !== null && <Cell label="Damroll" figure={damroll} />}
        </div>
      )}
      {armor.length > 0 && (
        <>
          <h3 className="pane-stats-caption">Armor</h3>
          <div className="pane-stats-ledger is-armor">
            {armor.map(({ k, ac, pct }) => {
              const tone = toneClass(armorTone(pct));
              return (
                <Cell
                  key={k}
                  label={ARMOR_NAMES[k]}
                  figure={signed(ac)}
                  small={pct === null ? undefined : `${pct}%`}
                  smallClass={tone}
                  title={armorTitle(k, ac, pct)}
                  line={
                    pct === null ? undefined : (
                      <span className="pane-stats-line" aria-hidden="true">
                        <span className={`pane-stats-fill${tone}`} style={{ width: `${pct}%` }} />
                      </span>
                    )
                  }
                />
              );
            })}
          </div>
        </>
      )}
      {saves.length > 0 && (
        <>
          <h3 className="pane-stats-caption">Saves</h3>
          <div className="pane-stats-ledger is-saves">
            {saves.map(({ k, v }) => (
              <Cell
                key={k}
                label={SAVE_NAMES[k]}
                figure={signed(v)}
                tone={toneClass(saveTone(v))}
                title={saveTitle(k, v)}
              />
            ))}
          </div>
        </>
      )}
    </div>
  );
}

/* ── Score sheet ───────────────────────────────────────────────────── */

function Sheet({ stats }: { stats: Stats }) {
  const ref = useRef<HTMLDivElement | null>(null);
  const bare = useGiveWay(ref, 1, useWidthKey(stats), anyCut) > 0;
  const { luck, hitroll, hitrollPct, damroll } = stats;
  const armor = armorRows(stats);
  const saves = saveRows(stats);
  return (
    <div ref={ref} className={`pane-body pane-stats-body${bare ? ' is-bare' : ''}`}>
      <div className="pane-stats-sheet" data-fit="">
        <ul className="pane-stats-sheet-list">
          {STAT_KEYS.map((k) => {
            const s = stats.stats[k];
            if (!s) return null;
            return (
              <li key={k} className="pane-stats-sheet-row" title={statTitle(k, s)} data-fit="">
                <span className="pane-stats-sheet-name" data-fit="">
                  {STAT_SHORT[k]}
                </span>
                {bare ? (
                  <span className={`pane-stats-sheet-value${toneClass(statTone(s))}`}>{s.cur}</span>
                ) : (
                  <span className="pane-stats-sheet-value">
                    {s.perm}
                    <span className="pane-stats-sheet-paren">
                      {' ('}
                      <span className={`pane-stats-sheet-cur${toneClass(statTone(s))}`}>
                        {s.cur}
                      </span>
                      {')'}
                    </span>
                  </span>
                )}
                <span className="pane-stats-sheet-over">
                  <Over over={s.over} className="is-warn" />
                </span>
              </li>
            );
          })}
        </ul>
        <ul className="pane-stats-sheet-list">
          {hitroll !== null && (
            <li className="pane-stats-sheet-row" title={hitrollTitle(hitroll, hitrollPct)}>
              <span className="pane-stats-sheet-name" data-fit="">
                Hitroll
              </span>
              <span className="pane-stats-sheet-value">{hitroll}</span>
              <span className="pane-stats-sheet-pct">
                {hitrollPct === null ? '' : `${hitrollPct}%`}
              </span>
            </li>
          )}
          {damroll !== null && (
            <li className="pane-stats-sheet-row" data-fit="">
              <span className="pane-stats-sheet-name" data-fit="">
                Damroll
              </span>
              <span className="pane-stats-sheet-value">{damroll}</span>
              <span className="pane-stats-sheet-pct" />
            </li>
          )}
          {luck && (
            <li className="pane-stats-sheet-row" data-fit="">
              <span className="pane-stats-sheet-name" data-fit="">
                Luck
              </span>
              <Luck {...luck} />
              <span className="pane-stats-sheet-pct" />
            </li>
          )}
        </ul>
      </div>
      {(armor.length > 0 || saves.length > 0) && (
        <div className="pane-stats-sheet" data-fit="">
          {armor.length > 0 && (
            <ul className="pane-stats-sheet-list">
              <li>
                <h3 className="pane-stats-caption">Armor</h3>
              </li>
              {armor.map(({ k, ac, pct }) => (
                <li
                  key={k}
                  className="pane-stats-sheet-row"
                  title={armorTitle(k, ac, pct)}
                  data-fit=""
                >
                  <span className="pane-stats-sheet-name" data-fit="">
                    {ARMOR_NAMES[k]}
                  </span>
                  <span className="pane-stats-sheet-value">{signed(ac)}</span>
                  <span className={`pane-stats-sheet-pct${toneClass(armorTone(pct))}`}>
                    {pct === null ? '' : `${pct}%`}
                  </span>
                </li>
              ))}
            </ul>
          )}
          {saves.length > 0 && (
            <ul className="pane-stats-sheet-list">
              <li>
                <h3 className="pane-stats-caption">Saves</h3>
              </li>
              {saves.map(({ k, v }) => (
                <li key={k} className="pane-stats-sheet-row" title={saveTitle(k, v)} data-fit="">
                  <span className="pane-stats-sheet-name" data-fit="">
                    {SAVE_NAMES[k]}
                  </span>
                  <span className={`pane-stats-sheet-value${toneClass(saveTone(v))}`}>
                    {signed(v)}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  );
}
