import { useEffect, useMemo, useState } from 'react';
import type { BandEnv } from '../terminal/bandCells';
import {
  BOX_TEXT_X,
  cellsBefore,
  nameChoices,
  namesFor,
  numberButtons,
  placeNameButtons,
  type NameChoice,
} from './cardRules';
import {
  promptCandidates,
  promptCaptureCheck,
  promptCaptureFromLine,
  type PromptCandidateGroup,
  type PromptCaptureCheck,
  type PromptCheckRead,
  type PromptCompileReport,
} from '../ipc/prompt';
import { numberRuns } from './promptScreen';
import { parseSgrCells } from '../terminal/sgrCells';
import { Button, CheckIcon, ChevronDownIcon, Field } from '../ui';
import { MenuSeparator } from '../ui/MenuSurface';
import { focusUnderPointer } from '../ui/menuAim';
import { CardMenu } from './CardMenu';
import { MatchRow } from './PromptCandidate';
import { CellLine } from './PromptCells';

// P15, other games: B2's question, which line is your prompt, then A2's
// naming of its numbers inside the card. Vosh saves an anchored pattern
// with its settle flag to the profile, never to the catalog.

/** A line you pointed at: its ring entry as the game sent it. */
export interface PointedLine {
  id: number;
  raw: string;
  plain: string;
}

interface PointPickProps {
  /** The session whose prompt the card works on. */
  session: number;
  /** The group to propose first. */
  start: number;
  onRead: (line: PointedLine, group: number) => void;
  /** The line it proposes now, which the terminal highlights. */
  onShow?: (plain: string | null) => void;
}

/** B2: the line that came right before your commands most often, which
 *  Vosh proposes as your prompt. */
export function PointPick({ session, start, onRead, onShow }: PointPickProps) {
  const [groups, setGroups] = useState<PromptCandidateGroup[] | null>(null);
  const [at, setAt] = useState(start);
  useEffect(() => {
    let alive = true;
    void promptCandidates(session)
      .then((next) => {
        if (alive) setGroups(next);
      })
      .catch(() => setGroups([]));
    return () => {
      alive = false;
    };
  }, [session]);
  const count = groups?.length ?? 0;
  const group = count > 0 ? groups![at % count] : null;
  const proposed = group?.entries[0]?.plain ?? null;
  useEffect(() => {
    onShow?.(proposed);
  }, [proposed, onShow]);
  const seen = group
    ? group.count > 1
      ? `The same line came after each of your last ${group.count} commands.`
      : 'This line came right before your last command.'
    : null;
  return (
    <>
      <div className="pc-body">
        <p className="pc-question">Is the highlighted line your prompt?</p>
        <p className="pc-copy">
          Vosh reads the numbers in it, hides the line, and draws your own prompt in its place.
        </p>
        {groups && (
          <p className="pc-seen">
            {seen && <CheckIcon className="pc-match-check" />}
            <span>
              {seen ??
                'Vosh has not seen your prompt since you connected. Send a command and Vosh checks again.'}
            </span>
          </p>
        )}
      </div>
      <div className="pc-rule" aria-hidden="true" />
      <div className="pc-foot is-end">
        <Button disabled={count < 2} onClick={() => setAt((i) => (i + 1) % Math.max(1, count))}>
          Pick another line
        </Button>
        <Button
          variant="primary"
          disabled={!group}
          onClick={() => {
            if (group) {
              const { id, raw, plain } = group.entries[0];
              onRead({ id, raw, plain }, at % count);
            }
          }}
        >
          Read this line
        </Button>
      </div>
    </>
  );
}

interface PointNameProps {
  /** The session whose prompt the card works on. */
  session: number;
  /** The line you pointed at. */
  line: PointedLine;
  onUse: (report: PromptCompileReport) => void;
  onPickAnother: () => void;
  refresh: number;
  env: BandEnv;
  cellW: number;
  /** The width of a label in the buttons' 11 px type. */
  measure: (label: string) => number;
  /** The line the box shows and which of its numbers carry a name, by
   *  their place among its numbers, so the terminal marks them too. */
  onShow?: (shown: { line: string; named: boolean[] } | null) => void;
}

/** The byte offset `at` of `line` as a character index. */
function charIndex(line: string, at: number): number {
  const bytes = new TextEncoder().encode(line);
  return new TextDecoder().decode(bytes.slice(0, at)).length;
}

/** A2: the line with each number marked and a menu under each pair that
 *  names the value it reads. */
export function PointName({
  session,
  line,
  onUse,
  onPickAnother,
  refresh,
  env,
  cellW,
  measure,
  onShow,
}: PointNameProps) {
  const id = line.id;
  const [names, setNames] = useState<string[] | null>(null);
  const [report, setReport] = useState<PromptCompileReport | null>(null);
  const [check, setCheck] = useState<PromptCaptureCheck | null>(null);
  const [index, setIndex] = useState(0);
  const [menu, setMenu] = useState<{ number: number; anchor: HTMLElement } | null>(null);
  // Other name… turns into a field for a name of your own.
  const [other, setOther] = useState<string | null>(null);

  // The entry the box shows: the one you pointed at, or one the stepper
  // reached among the prompts the line's pattern reads.
  const shown: Pick<PromptCheckRead, 'id' | 'raw' | 'plain'> = check?.reads[index] ?? line;
  const shownId = shown.id;

  useEffect(() => {
    let alive = true;
    void promptCaptureFromLine(shownId, names ?? undefined, session)
      .then((next) => {
        if (alive) setReport(next);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [shownId, names, session]);

  const shape = report?.ok ? report.shapes[0] : null;
  useEffect(() => {
    if (!shape || !report) return;
    let alive = true;
    void promptCaptureCheck(
      {
        kind: 'regex',
        lines: shape.lines,
        settle: shape.settle,
        names: report.names,
      },
      session,
    )
      .then((next) => {
        if (!alive) return;
        setCheck(next);
        setIndex((i) => Math.min(i, Math.max(0, next.reads.length - 1)));
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
    // The pattern decides the check, not the numbers' names.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [shape?.lines.join('\n'), shape?.settle, report?.names, refresh, session]);

  const raw = shown.raw;
  const plain = shown.plain.split('\n').pop() ?? '';
  const rawLine = useMemo(() => {
    const rows = parseSgrCells(raw);
    return rows[rows.length - 1] ?? [];
  }, [raw]);
  const numbers = useMemo(() => report?.numbers ?? [], [report]);
  const marks = numbers
    .filter((n) => n.name.length > 0)
    .map((n) => ({
      from: cellsBefore(plain, charIndex(plain, n.span[0])),
      to: cellsBefore(plain, charIndex(plain, n.span[1])),
      warn: false,
    }));
  // Each number of the line, named or not, by its place among them.
  const named = useMemo(() => {
    const spans = numbers
      .filter((n) => n.name.length > 0)
      .map((n) => [charIndex(plain, n.span[0]), charIndex(plain, n.span[1])]);
    return numberRuns(plain).map((run) =>
      spans.some(([from, to]) => run.start < to && from < run.end),
    );
  }, [numbers, plain]);
  const namedKey = named.join(',');
  useEffect(() => {
    onShow?.(report ? { line: plain, named } : null);
    // The names decide the marks, not the array they come in.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [plain, namedKey, report === null, onShow]);
  const buttons = numberButtons(numbers);
  // Each button under its number, on a row of its own when it would draw
  // over the one before it.
  const placed = placeNameButtons(
    buttons.map((button) => ({
      index: button.index,
      col: cellsBefore(plain, charIndex(plain, numbers[button.index].span[0])),
      label: button.label,
    })),
    cellW,
    measure,
  );
  const choose = (number: number, name: string) => {
    setNames(namesFor(numbers, number, name));
    setMenu(null);
    setOther(null);
  };
  const current = menu ? numbers[menu.number] : null;
  const groupsOf = nameChoices(report?.gmcp_names ?? []);

  return (
    <>
      <div className="pc-body">
        <p className="pc-question" id={`pc-naming-${id}`}>
          Is this the line your game shows before each command?
        </p>
        <p className="pc-copy">
          Vosh named each number from the letters after it. Change any name that is wrong.
        </p>
        {
          <div
            className="pc-box"
            aria-labelledby={`pc-naming-${id}`}
            style={{ height: placed.height }}
          >
            <CellLine
              className="pc-box-line"
              style={{ top: 12, left: BOX_TEXT_X }}
              cells={rawLine}
              env={env}
              cellW={cellW}
              marks={marks}
            />
            {buttons.map((button, i) => {
              const at = placed.buttons[i];
              const open = menu?.number === button.index;
              return (
                <button
                  key={button.index}
                  type="button"
                  className="pc-name-button"
                  aria-haspopup="menu"
                  aria-expanded={open}
                  aria-label={button.aria}
                  style={{ left: at.left, top: at.top }}
                  onClick={(e) =>
                    setMenu(open ? null : { number: button.index, anchor: e.currentTarget })
                  }
                >
                  <span>{button.label}</span>
                  <ChevronDownIcon size={12} />
                </button>
              );
            })}
          </div>
        }
        <MatchRow check={check} index={index} onStep={setIndex} />
      </div>
      <div className="pc-rule" aria-hidden="true" />
      <div className="pc-foot is-end">
        <Button onClick={onPickAnother}>Pick another line</Button>
        <Button
          variant="primary"
          disabled={!report?.ok}
          onClick={() => {
            if (report?.ok) onUse(report);
          }}
        >
          Use these names
        </Button>
      </div>
      {menu && current && (
        <CardMenu
          anchor={menu.anchor}
          place="below-start"
          label={`Name for ${current.text}`}
          onClose={() => {
            setMenu(null);
            setOther(null);
          }}
        >
          {groupsOf.map((group, g) => (
            <NameGroup
              key={g}
              first={g === 0}
              choices={group}
              current={current.name}
              onChoose={(name) => choose(menu.number, name)}
            />
          ))}
          <MenuSeparator />
          {other === null ? (
            <li role="none">
              <button
                type="button"
                role="menuitem"
                className="menu-item pc-name-item"
                onPointerMove={focusUnderPointer}
                onClick={() => setOther('')}
              >
                <span>Other name…</span>
              </button>
            </li>
          ) : (
            <li role="none" className="pc-name-field">
              <Field
                mono
                width="100%"
                value={other}
                autoFocus
                placeholder="A name of your own"
                aria-label="Other name"
                onChange={setOther}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && other.trim().length > 0) {
                    e.preventDefault();
                    choose(menu.number, other.trim());
                  }
                }}
              />
            </li>
          )}
          <MenuSeparator />
          <li role="none">
            <button
              type="button"
              role="menuitem"
              className="menu-item pc-name-item"
              onPointerMove={focusUnderPointer}
              onClick={() => choose(menu.number, '')}
            >
              <span>Leave out</span>
            </button>
          </li>
        </CardMenu>
      )}
    </>
  );
}

function NameGroup({
  first,
  choices,
  current,
  onChoose,
}: {
  first: boolean;
  choices: NameChoice[];
  current: string;
  onChoose: (name: string) => void;
}) {
  return (
    <>
      {!first && <MenuSeparator />}
      {choices.map((choice) => {
        const checked = choice.name === current;
        return (
          <li key={choice.name} role="none">
            <button
              type="button"
              role="menuitemradio"
              aria-checked={checked}
              aria-label={choice.package ? `${choice.name} from ${choice.package}` : undefined}
              className="menu-item pc-name-item"
              onPointerMove={focusUnderPointer}
              onClick={() => onChoose(choice.name)}
            >
              {checked && <CheckIcon className="pc-start-check" />}
              <span className={choice.package ? 'pc-name-mono' : undefined}>{choice.label}</span>
              {choice.package && <span className="pc-name-package">{choice.package}</span>}
            </button>
          </li>
        );
      })}
    </>
  );
}
