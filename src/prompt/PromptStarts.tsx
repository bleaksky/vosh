import { useEffect, useMemo, useState, type ReactNode } from 'react';
import type { BandEnv } from '../terminal/bandCells';
import { startRows, type StartRow } from './cardRules';
import { useLabelMeasure } from '../lib/useCellWidth';
import { type PromptConfig, type PromptDesign, type PromptPreset } from '../ipc/prompt';
import { promptRenderMany } from '../ipc/promptDesign';
import { parseSgrCells, shownColumns, type Cell } from '../terminal/sgrCells';
import { Button, CheckIcon, ChevronDownIcon, ChevronRightIcon, PlusIcon } from '../ui';
import { CardMenu } from './CardMenu';
import { CellLine } from './PromptCells';

// P4: the designs to start from, each drawn with your live values in its
// real colors, on first use as the card's body and on every later open
// in the Presets menu of the card at rest. P11's body shows while drawing
// is off.

/** A design drawn as a sample: its lines out of a fight, after the lines
 *  it draws only in a fight, which carry the tag. */
export interface Sample {
  lines: { cells: Cell[]; fight: boolean }[];
}

/** The lines of a sample: the rows a fight adds above the rows the
 *  design draws now, tagged, then those rows, with empty rows at the end
 *  left off. */
function sampleOf(live: string, fight: string): Sample {
  const trim = (rows: Cell[][]) => {
    const out = [...rows];
    while (out.length > 0 && shownColumns(out[out.length - 1]) === 0) out.pop();
    return out;
  };
  const now = trim(parseSgrCells(live));
  const fought = trim(parseSgrCells(fight));
  const extra = Math.max(0, fought.length - now.length);
  return {
    lines: [
      ...fought.slice(0, extra).map((cells) => ({ cells, fight: true })),
      ...now.map((cells) => ({ cells, fight: false })),
    ],
  };
}

/** Each design drawn, by its template, with the live values of `session`
 *  or samples. A design in `fought` adds the lines it draws only in a
 *  fight, as Detailed does on P4. */
function useSamples(
  session: number,
  templates: readonly string[],
  fought: ReadonlySet<string>,
  values: 'live' | 'sample',
  refresh: number,
): Map<string, Sample> {
  const [samples, setSamples] = useState<Map<string, Sample>>(new Map());
  const key = templates.join('\u0000') + '\u0001' + [...fought].join('\u0000');
  useEffect(() => {
    let alive = true;
    const drawn = templates.filter((t) => t.length > 0);
    if (drawn.length === 0) return;
    void Promise.all([
      promptRenderMany(
        drawn.map((template) => ({ template, values })),
        session,
      ),
      promptRenderMany(
        drawn.map((template) => ({ template, values, preview: 'fight' as const })),
        session,
      ),
    ])
      .then(([live, fight]) => {
        if (!alive) return;
        const next = new Map<string, Sample>();
        drawn.forEach((template, i) => {
          const inFight = fought.has(template) ? (fight[i]?.ansi ?? '') : '';
          next.set(template, sampleOf(live[i]?.ansi ?? '', inFight));
        });
        setSamples(next);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
    // The key stands for the templates.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, values, refresh, session]);
  return samples;
}

/** The rows of the start list or the Presets menu, the designs of
 *  From another profile, and Start empty. */
export interface StartChoices {
  rows: StartRow[];
  others: StartRow[];
  empty: StartRow | null;
}

interface StartListProps extends StartChoices {
  samples: Map<string, Sample>;
  env: BandEnv;
  cellW: number;
  /** The column a sample is cut to, in px. */
  column: number;
  /** The column a fight line is cut to, which leaves room for its tag. */
  fightColumn: number;
  onPick: (row: StartRow) => void;
  /** It sits inside the Presets menu, so it is a group of its items. */
  inMenu?: boolean;
}

/** The rows of the start list, in the palette row recipe. */
function StartList({
  rows,
  others,
  empty,
  samples,
  env,
  cellW,
  column,
  fightColumn,
  onPick,
  inMenu = false,
}: StartListProps) {
  const [others_, setOthers] = useState<HTMLElement | null>(null);
  return (
    <ul className="pc-starts" role={inMenu ? 'group' : 'menu'} aria-label="Starts">
      {rows.map((row) => (
        <li key={row.id} role="none">
          <button
            type="button"
            role="menuitemradio"
            aria-checked={row.checked}
            className="pc-start"
            onClick={() => onPick(row)}
          >
            {row.checked && <CheckIcon className="pc-start-check" />}
            <span className="pc-start-name">{row.label}</span>
            {(samples.get(row.template)?.lines ?? []).map((line, i) => (
              <span key={i} className="pc-start-sample" aria-hidden="true">
                <CellLine
                  cells={line.cells}
                  env={env}
                  cellW={cellW}
                  column={line.fight ? fightColumn : column}
                />
                {line.fight && <span className="pc-start-tag">in a fight</span>}
              </span>
            ))}
          </button>
        </li>
      ))}
      {others.length > 0 && (
        <li role="none">
          <button
            type="button"
            role="menuitem"
            aria-haspopup="menu"
            aria-expanded={others_ !== null}
            className="pc-start is-single"
            onClick={(e) => setOthers(others_ ? null : e.currentTarget)}
          >
            <span className="pc-start-name">From another profile</span>
            <ChevronRightIcon className="pc-start-chevron" />
          </button>
          {others_ && (
            <CardMenu
              anchor={others_}
              place="beside"
              width={232}
              label="From another profile"
              onClose={() => setOthers(null)}
            >
              {others.map((row) => (
                <li key={row.id} role="none">
                  <button
                    type="button"
                    role="menuitemradio"
                    aria-checked={row.checked}
                    className="ov-menu-item"
                    onClick={() => {
                      setOthers(null);
                      onPick(row);
                    }}
                  >
                    <span className="ov-menu-label">{row.label}</span>
                  </button>
                </li>
              ))}
            </CardMenu>
          )}
        </li>
      )}
      {empty && (
        <>
          <li role="separator" className="pc-starts-sep" />
          <li role="none">
            <button
              type="button"
              role="menuitemradio"
              aria-checked={empty.checked}
              className="pc-start is-single"
              onClick={() => onPick(empty)}
            >
              {empty.checked && <CheckIcon className="pc-start-check" />}
              <span className="pc-start-name">{empty.label}</span>
            </button>
          </li>
        </>
      )}
    </ul>
  );
}

/** The sample column of the start list, 429 px, and of the Presets
 *  menu, 410 px (P4 and P0). A sample wider than its column ends on an
 *  ellipsis inside it. */
const LIST_SAMPLE_PX = 429;
const MENU_SAMPLE_PX = 410;
/** The row's text column in the start list, where a fight line and its
 *  tag may run past the sample column (P4). */
const LIST_ROW_PX = 498;

interface StartsProps {
  /** The session whose prompt the card works on. */
  session: number;
  mode: 'start' | 'rest';
  config: PromptConfig;
  presets: readonly PromptPreset[];
  designs: readonly PromptDesign[];
  /** Rows of its own in place of the presets and designs, such as a
   *  vitals text's Presets (vitalsStartRows). */
  own?: StartChoices | undefined;
  values: 'live' | 'sample';
  refresh: number;
  env: BandEnv;
  cellW: number;
  /** You picked a start, by its id and its design. */
  onPick: (row: StartRow) => void;
  onInsertValue: () => void;
  /** The hint at rest. */
  restHint?: string | undefined;
  /** A line under the hint at rest, such as what the Lament preview
   *  hides (P8c). */
  note?: string | null;
  /** You turned prompts off in the game, so the card at rest says so in
   *  place of its hint (P14). */
  promptsOff?: boolean;
  /** The not matching sentence while no prompt has matched, which the
   *  card at rest says in place of its hint after prompts off. */
  notMatching?: string | null;
  /** What goes between the hint and the list on first use: the Line
   *  triggers that matched your prompt (D6). */
  children?: ReactNode;
}

/** P4's body: the start list on first use, or the card at rest with
 *  Insert value… and the Presets menu. */
export function Starts({
  session,
  mode,
  config,
  presets,
  designs,
  own,
  values,
  refresh,
  env,
  cellW,
  onPick,
  onInsertValue,
  restHint = 'Click any part of your prompt to change it.',
  note = null,
  promptsOff = false,
  notMatching = null,
  children,
}: StartsProps) {
  const list = useMemo(
    () => own ?? startRows(presets, config, designs),
    [own, presets, config, designs],
  );
  const templates = useMemo(() => list.rows.map((r) => r.template), [list]);
  // Detailed shows its fight line with the sample opponent, tagged, so
  // you see what it adds in a fight (P4). The other starts draw your
  // prompt as it is now.
  const fought = useMemo(
    () => new Set(list.rows.filter((r) => r.id === 'detailed').map((r) => r.template)),
    [list],
  );
  const samples = useSamples(session, templates, fought, values, refresh);
  // A fight line leaves room for its tag, 8 px after it.
  const measure = useLabelMeasure(11);
  const tagRoom = 8 + measure('in a fight');
  const [presetsAt, setPresetsAt] = useState<HTMLElement | null>(null);
  const insert = (
    <Button icon={<PlusIcon />} onClick={onInsertValue}>
      Insert value…
    </Button>
  );
  if (mode === 'start') {
    return (
      <div className="pc-body is-list">
        <p className="pc-hint">
          {config.draw
            ? 'Start from one of these, or click any part of your prompt to change it.'
            : 'Start from one of these. Picking one turns on Draw your prompt.'}
        </p>
        {children}
        <StartList
          {...list}
          samples={samples}
          env={env}
          cellW={cellW}
          column={LIST_SAMPLE_PX}
          fightColumn={LIST_ROW_PX - tagRoom}
          onPick={onPick}
        />
        <div className="pc-actions">{insert}</div>
      </div>
    );
  }
  return (
    <div className="pc-body">
      {promptsOff || notMatching ? (
        <p className="pc-hint is-warn" role="status">
          <span className="pc-warn-dot" aria-hidden="true" />
          <span>
            {promptsOff
              ? 'You turned prompts off in the game. Type prompt in the game to turn them back on.'
              : notMatching}
          </span>
        </p>
      ) : (
        <p className="pc-hint">{restHint}</p>
      )}
      {note && <p className="pc-rest-note">{note}</p>}
      <div className="pc-actions">
        {insert}
        <Button
          className="pc-presets-button"
          aria-haspopup="menu"
          aria-expanded={presetsAt !== null}
          onClick={(e) => setPresetsAt(presetsAt ? null : e.currentTarget)}
        >
          <span>Presets</span>
          <ChevronDownIcon />
        </Button>
      </div>
      {presetsAt && (
        <CardMenu
          anchor={presetsAt}
          place="above-start"
          width={468}
          label="Presets"
          onClose={() => setPresetsAt(null)}
        >
          <li role="none">
            <StartList
              {...list}
              samples={samples}
              env={env}
              cellW={cellW}
              column={MENU_SAMPLE_PX}
              fightColumn={MENU_SAMPLE_PX - tagRoom}
              inMenu
              onPick={(row) => {
                setPresetsAt(null);
                onPick(row);
              }}
            />
          </li>
        </CardMenu>
      )}
    </div>
  );
}

interface DrawOffProps {
  /** Who the design is saved for, `Tester` or `Default`. */
  name: string;
  /** Another game, where Vosh still reads the values in your prompt. */
  other: boolean;
  confirming: boolean;
  onForget: () => void;
}

/** P11: drawing is off, so you see the game's own prompt, and Forget
 *  your game's prompt stops Vosh reading it. */
export function DrawOff({ name, other, confirming, onForget }: DrawOffProps) {
  return (
    <div className="pc-body">
      <p className="pc-question">You see the game&apos;s own prompt.</p>
      <p className="pc-copy">
        {other
          ? `Vosh keeps your design for ${name} and still reads the values in your prompt.`
          : `Vosh keeps your design for ${name}.`}
      </p>
      <div className="pc-forget">
        <Button
          variant="danger"
          aria-haspopup="dialog"
          aria-expanded={confirming}
          onClick={onForget}
        >
          Forget your game&apos;s prompt
        </Button>
      </div>
    </div>
  );
}
