import { useMemo, type ReactNode } from 'react';
import type { BandEnv } from '../terminal/bandCells';
import {
  BOX_TEXT_X,
  boxHeight,
  matchLines,
  matchTone,
  placeLabels,
  readMarks,
  readRows,
} from './cardRules';
import type { PromptCaptureCheck, PromptCheckRead } from '../ipc/prompt';
import { CheckIcon, ChevronDownIcon, ChevronUpIcon, IconButton } from '../ui';
import { CellLine } from './PromptCells';

// The parts the capture steps share: the candidate box that shows a
// prompt as the game sent it with each value marked and named under it
// (P3, P3b, P3c), and the match line with the stepper beside it.

interface CandidateBoxProps {
  read: PromptCheckRead;
  env: BandEnv;
  cellW: number;
  measure: (label: string) => number;
  label: string;
}

/** A prompt as the game sent it, every value Vosh reads in the selection
 *  token and a run it cannot split in the warn ring, each named under its
 *  first character. */
export function CandidateBox({ read, env, cellW, measure, label }: CandidateBoxProps) {
  const placed = useMemo(() => placeLabels(read, cellW, measure), [read, cellW, measure]);
  const rows = useMemo(() => readRows(read), [read]);
  const marks = useMemo(() => readMarks(read), [read]);
  return (
    <div className="pc-box" role="group" aria-label={label} style={{ height: boxHeight(placed) }}>
      {rows.map((cells, i) => (
        <CellLine
          key={i}
          className="pc-box-line"
          style={{ top: placed.lineTops[i], left: BOX_TEXT_X }}
          cells={cells}
          env={env}
          cellW={cellW}
          marks={marks[i]}
        />
      ))}
      {placed.labels.map((name) => (
        <span
          key={name.mark}
          className={`pc-box-name${name.warn ? ' is-warn' : ''}`}
          style={{ left: name.left, top: name.top }}
        >
          {name.label}
        </span>
      ))}
    </div>
  );
}

interface StepperProps {
  /** The prompt shown, 0 for the newest. */
  index: number;
  count: number;
  onStep: (index: number) => void;
}

/** A2's stepper: which prompt the box shows, and the way to a newer or
 *  an older one. Like the find bar it comes from, it goes round from the
 *  newest to the oldest. It walks the box only and never scrolls the
 *  terminal (D19). */
export function Stepper({ index, count, onStep }: StepperProps) {
  return (
    <div className="pc-stepper">
      <span className="pc-stepper-count">
        {index + 1} of {count}
      </span>
      <IconButton
        label="Newer prompt"
        icon={<ChevronUpIcon />}
        onClick={() => onStep((index - 1 + count) % count)}
      />
      <IconButton
        label="Older prompt"
        icon={<ChevronDownIcon />}
        onClick={() => onStep((index + 1) % count)}
      />
    </div>
  );
}

interface MatchRowProps {
  check: PromptCaptureCheck | null;
  index: number;
  onStep: (index: number) => void;
  /** A warning that takes the match line's place, as on P3b. */
  warning?: string | null;
}

/** The match line, a check for a clean match and a warn dot for a poor
 *  one, with the stepper while there are prompts to step through. A
 *  warning about codes that run together takes its place. */
export function MatchRow({ check, index, onStep, warning }: MatchRowProps) {
  const count = check?.reads.length ?? 0;
  const stepper = count > 0 ? <Stepper index={index} count={count} onStep={onStep} /> : null;
  let text: ReactNode;
  if (warning) {
    text = (
      <p className="pc-match-text is-warn is-wrap">
        <span className="pc-warn-dot dot is-warn" aria-hidden="true" />
        <span>{warning}</span>
      </p>
    );
  } else if (check) {
    const tone = matchTone(check);
    text = (
      <div className={`pc-match-text${tone === 'warn' ? ' is-warn' : ''}`}>
        {tone === 'ok' && <CheckIcon className="pc-match-check" />}
        {tone === 'warn' && <span className="pc-warn-dot dot is-warn" aria-hidden="true" />}
        <p>
          {matchLines(check).map((sentence, i) => (
            <span key={i}>
              {i > 0 && <br />}
              {sentence}
            </span>
          ))}
        </p>
      </div>
    );
  } else {
    return null;
  }
  return (
    <div className="pc-match">
      {text}
      {stepper}
    </div>
  );
}
