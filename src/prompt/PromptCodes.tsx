import { useEffect, useId, useRef, useState } from 'react';
import type { BandEnv } from '../terminal/bandCells';
import {
  clockTime,
  entryCopy,
  lastSeenLine,
  legendColumns,
  localStamp,
  migratedNote,
  prefixNote,
} from './cardRules';
import {
  onGamePromptSeen,
  promptCaptureCheck,
  promptCompile,
  promptLastSeen,
  type GamePromptSeenPayload,
  type PromptCapture,
  type PromptCaptureCheck,
  type PromptCaptureSource,
  type PromptCheckRead,
  type PromptCompileReport,
  type PromptLegendRow,
  type PromptLineTrigger,
} from '../ipc/prompt';
import { useTauriEvent } from '../ipc/useTauriEvent';
import { errorText } from '../lib/text';
import { pushToast } from '../stores/toasts';
import { Button, Field } from '../ui';
import { CandidateBox, MatchRow } from './PromptCandidate';

// The capture steps on The Forsaken Lands: one where you tell Vosh your
// prompt setting when the game sent none this session, and one where
// Vosh reads your codes and shows what it reads in your newest prompt,
// with a warning when codes run together and a second line for a prompt
// from a fight.

/** The codes Vosh reads, and where they came from. */
export interface CodesRequest {
  prompt: string;
  fprompt: string;
  /** You typed or pasted them as you type them in the game. */
  typed: boolean;
  source: PromptCaptureSource;
  /** RFC 3339 local time. */
  seenAt: string;
}

interface CodesEntryProps {
  /** The session whose prompt the card works on. */
  session: number;
  /** The codes the profile holds, for Change codes…. */
  initial: { prompt: string; fprompt: string } | null;
  onRead: (codes: CodesRequest) => void;
  onPoint: () => void;
  /** The game sent Char.Prompt while the step was open. */
  onGameSent: () => void;
}

/** Your prompt setting, as Vosh saw it when you typed prompt, from
 *  your log, or as you type or paste it. The game's answers fill the
 *  fields while the step is open. */
export function CodesEntry({ session, initial, onRead, onPoint, onGameSent }: CodesEntryProps) {
  const promptId = useId();
  const fightId = useId();
  const [prompt, setPrompt] = useState(initial?.prompt ?? '');
  const [fprompt, setFprompt] = useState(initial?.fprompt ?? '');
  const [seen, setSeen] = useState<string | null>(null);
  const origin = useRef<{ source: PromptCaptureSource; at: string }>({
    source: 'typed',
    at: localStamp(new Date()),
  });

  useEffect(() => {
    if (initial) return;
    let alive = true;
    void promptLastSeen(session)
      .then((last) => {
        if (!alive || !last?.prompt || last.source === 'gmcp') return;
        setPrompt(last.prompt);
        setFprompt(last.fprompt ?? '');
        setSeen(lastSeenLine(last, new Date()));
        origin.current = { source: last.source, at: last.at ?? localStamp(new Date()) };
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
    // The step reads where the codes came from once, as it opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  // What the game says of your prompt settings, in the card's session
  // only.
  useTauriEvent(
    (cb: (seen: [GamePromptSeenPayload, number]) => void) =>
      onGamePromptSeen((payload, from) => cb([payload, from])),
    ([payload, from]) => {
      if (from !== session) return;
      if (payload.kind === 'gmcp') {
        onGameSent();
        return;
      }
      const now = new Date();
      if (payload.kind === 'prompt') {
        setPrompt(payload.text);
        setSeen(`Vosh saw it when you typed prompt at ${clockTime(now)}.`);
        origin.current = { source: 'session', at: localStamp(now) };
      } else if (payload.kind === 'fprompt') {
        setFprompt(payload.text);
      }
    },
  );

  // What you type is yours, though the copy keeps saying where the codes
  // came from, so the card does not change its question under you.
  const typed = (set: (v: string) => void) => (value: string) => {
    set(value);
    origin.current = { source: 'typed', at: localStamp(new Date()) };
  };
  const copy = entryCopy(seen, initial !== null);
  const ready = prompt.trim().length > 0;
  return (
    <>
      <div className="pc-body">
        <p className="pc-question">{copy.title}</p>
        <p className="pc-copy">{copy.body}</p>
        <div className="pc-field-row is-first">
          <label htmlFor={promptId}>Prompt</label>
          <Field
            id={promptId}
            mono
            width={440}
            value={prompt}
            placeholder="%n%P%C<%hhp %mm %vmv>"
            onChange={typed(setPrompt)}
          />
        </div>
        <div className="pc-field-row">
          <label htmlFor={fightId}>Fight prompt</label>
          <Field
            id={fightId}
            mono
            width={440}
            value={fprompt}
            placeholder="None set"
            onChange={typed(setFprompt)}
          />
        </div>
        <p className="pc-note">
          The game uses your fight prompt while you fight, once you set one with fprompt.
        </p>
      </div>
      <div className="pc-rule" aria-hidden="true" />
      <div className="pc-foot">
        <Button onClick={onPoint}>Point at the line instead</Button>
        <span className="pc-spacer" />
        <Button
          variant="primary"
          disabled={!ready}
          onClick={() =>
            onRead({
              prompt,
              fprompt,
              typed: origin.current.source === 'typed',
              source: origin.current.source,
              seenAt: origin.current.at,
            })
          }
        >
          Read these codes
        </Button>
      </div>
    </>
  );
}

interface CodesReadProps {
  /** The session whose prompt the card works on. */
  session: number;
  request: CodesRequest;
  /** Where the codes came from, on the new build. */
  sourceLine: string | null;
  /** The capture the profile holds now. */
  capture: PromptCapture;
  secondary: { label: string; onClick: () => void };
  onUse: (report: PromptCompileReport) => void;
  /** Bumps whenever a prompt arrives, so the match line counts again. */
  refresh: number;
  env: BandEnv;
  cellW: number;
  measure: (label: string) => number;
  /** Hears the newest prompt the codes read, whose values the card
   *  marks on your prompt. The stepper never moves those marks. */
  onNewest?: (read: PromptCheckRead | null) => void;
}

/** What Vosh reads from your codes, shown on your newest prompt with
 *  the codes they come from. */
export function CodesRead({
  session,
  request,
  sourceLine,
  capture,
  secondary,
  onUse,
  refresh,
  env,
  cellW,
  measure,
  onNewest,
}: CodesReadProps) {
  const [report, setReport] = useState<PromptCompileReport | null>(null);
  const [check, setCheck] = useState<PromptCaptureCheck | null>(null);
  const [index, setIndex] = useState(0);

  useEffect(() => {
    let alive = true;
    void promptCompile(
      {
        kind: 'aabahran',
        prompt: request.prompt,
        fprompt: request.fprompt,
        typed: request.typed,
      },
      session,
    )
      .then((next) => {
        if (alive) setReport(next);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [request.prompt, request.fprompt, request.typed, session]);

  useEffect(() => {
    if (!report?.ok) {
      setCheck(null);
      return;
    }
    let alive = true;
    void promptCaptureCheck(
      {
        kind: 'aabahran',
        prompt: report.prompt,
        fprompt: report.fprompt,
        follow_game: true,
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
  }, [report, refresh, session]);

  const read = check?.reads[index] ?? null;
  const newest = check?.reads[0] ?? null;
  useEffect(() => {
    onNewest?.(newest);
  }, [newest, onNewest]);
  const runTogether = report?.warnings.find((w) => w.kind === 'run_together')?.message ?? null;
  const legend = report?.legend ?? [];
  const grid = legend.filter((row) => row.warning === null);
  const warned = legend.filter((row) => row.warning !== null);
  const [left, right] = legendColumns(grid);
  const tagOf = (row: PromptLegendRow) =>
    row.tag ?? (row.fight && !read?.fight ? 'in a fight' : null);
  const notes = [sourceLine, prefixNote(read), migratedNote(capture)].filter(
    (n): n is string => n !== null,
  );
  const fixed = (report?.fixes.length ?? 0) > 0;
  const boxLabel = read
    ? index === 0
      ? 'Your newest prompt'
      : `Prompt ${index + 1} of ${check?.reads.length ?? 0}${read.fight ? ', from a fight' : ''}`
    : '';

  return (
    <>
      <div className="pc-body">
        <p className="pc-question">Vosh read your prompt.</p>
        {report?.shows && <p className="pc-copy">{report.shows}</p>}
        {report?.error && (
          <div className="pc-match">
            <p className="pc-match-text is-warn is-wrap">
              <span className="pc-warn-dot" aria-hidden="true" />
              <span>{report.error.message}</span>
            </p>
          </div>
        )}
        {read && (
          <CandidateBox read={read} env={env} cellW={cellW} measure={measure} label={boxLabel} />
        )}
        {report?.ok && (
          <MatchRow check={check} index={index} onStep={setIndex} warning={runTogether} />
        )}
        {report?.fixes.map((command) => (
          <CommandBox key={command} command={command} />
        ))}
        {grid.length > 0 && (
          <div className="pc-legend">
            {[left, right].map((column, c) => (
              <div key={c}>
                {column.map((row) => (
                  <LegendRow key={`${row.which}-${row.span[0]}`} row={row} tag={tagOf(row)} />
                ))}
              </div>
            ))}
          </div>
        )}
        {warned.map((row) => (
          <div key={`${row.which}-${row.span[0]}-${row.label}`} className="pc-legend-warning">
            <LegendRow row={row} tag={tagOf(row)} />
            <p>
              <span className="pc-warn-dot" aria-hidden="true" />
              <span>{row.warning}</span>
            </p>
          </div>
        ))}
        {notes.length > 0 && (
          <div className={`pc-notes${fixed ? ' is-after-fix' : ''}`}>
            {notes.map((note) => (
              <p key={note} className="pc-note">
                {note}
              </p>
            ))}
          </div>
        )}
        {report?.fix_note && <p className="pc-fix-note">{report.fix_note}</p>}
      </div>
      <div className="pc-rule" aria-hidden="true" />
      <div className="pc-foot">
        <Button onClick={secondary.onClick}>{secondary.label}</Button>
        <span className="pc-spacer" />
        <Button
          variant="primary"
          disabled={!report?.ok}
          onClick={() => {
            if (report?.ok) onUse(report);
          }}
        >
          Use these codes
        </Button>
      </div>
    </>
  );
}

function LegendRow({ row, tag }: { row: PromptLegendRow; tag: string | null }) {
  return (
    <div className="pc-legend-row">
      {row.code.length > 0 && (
        <code className={`pc-code${row.warn ? ' is-warn' : ''}`}>{row.code}</code>
      )}
      <span className="pc-legend-label">{row.label}</span>
      {tag && <span className="pc-legend-tag">{tag}</span>}
    </div>
  );
}

/** The setting that fixes codes that run together, with Copy. Vosh never
 *  sends it. Settings draws it 600 wide under its block. */
export function CommandBox({ command, className }: { command: string; className?: string }) {
  return (
    <div className={className ? `pc-command ${className}` : 'pc-command'}>
      <span className="pc-command-text" aria-label="Command to type in the game">
        {command}
      </span>
      <Button
        onClick={() => {
          void navigator.clipboard
            .writeText(command)
            .then(() => pushToast({ kind: 'success', message: 'Copied', timeoutMs: 1600 }))
            .catch(() => {});
        }}
      >
        Copy
      </Button>
    </div>
  );
}

interface LineTriggersProps {
  triggers: readonly PromptLineTrigger[];
  /** Set one to match Prompts. It resolves once the trigger moved. */
  onMove: (name: string) => Promise<void>;
}

/** The Line triggers row: once a profile reads your prompt, Line
 *  triggers no longer see it, so the first capture a profile saves
 *  names the enabled Line triggers that matched your recent prompts,
 *  each with Move to Prompts. A trigger a highlight preset installed
 *  changes only with its preset, so it has no button. */
export function LineTriggers({ triggers, onMove }: LineTriggersProps) {
  const [busy, setBusy] = useState<string | null>(null);
  if (triggers.length === 0) return null;
  return (
    <div className="pc-d6">
      <p className="pc-d6-text">
        <span className="pc-warn-dot" aria-hidden="true" />
        <span>
          These triggers matched your prompt as a line. Vosh now sends your prompt only to Prompts
          triggers.
        </span>
      </p>
      <ul className="pc-d6-list">
        {triggers.map((t) => (
          <li key={t.name} className="pc-d6-row">
            <span className="pc-d6-what">
              <span className="pc-d6-name">{t.name}</span>
              <span className="pc-d6-pattern">{t.pattern}</span>
            </span>
            {!t.preset && (
              <Button
                disabled={busy !== null}
                onClick={() => {
                  setBusy(t.name);
                  void onMove(t.name)
                    .catch((e: unknown) =>
                      pushToast({
                        kind: 'error',
                        message: errorText(e),
                      }),
                    )
                    .finally(() => setBusy(null));
                }}
              >
                Move to Prompts
              </Button>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}
