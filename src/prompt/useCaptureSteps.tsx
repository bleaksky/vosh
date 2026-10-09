// The prompt card's capture steps, which tell Vosh how to read your
// prompt: the codes the game sent, your setting when the game sent
// none, or the line another game prints and the names of its numbers. A
// capture they save takes the card on to the designs to start from.

import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react';
import {
  codeReaderStep,
  codesSourceLine,
  firstCapture,
  localStamp,
  openingStep,
  savedCapture,
  withCapture,
  type CardStep,
} from './cardRules';
import { numberMarks, wholeMarks, type ScreenAsk } from './promptScreen';
import { NOWHERE, rawMarks, type Pointing } from './promptPieces';
import {
  promptCandidates,
  promptLineTriggers,
  type PromptCapture,
  type PromptCheckRead,
  type PromptCompileReport,
  type PromptConfig,
  type PromptLineTrigger,
  type PromptState,
} from '../ipc/prompt';
import type { GamePromptSeen } from '../stores/gmcp/gamePromptStore';
import type { BandEnv } from '../terminal/bandCells';
import { CodesEntry, CodesRead, type CodesRequest } from './PromptCodes';
import { PointName, PointPick, type PointedLine } from './PromptPoint';

/** What the steps read from the card and change in it. */
interface StepsCard {
  /** The session whose prompt the card works on. */
  session: number;
  step: CardStep | null;
  setStep: (step: CardStep) => void;
  state: PromptState | null;
  /** The profile's host is one Vosh knows. */
  knownHost: boolean;
  /** The game's latest Char.Prompt. */
  game: GamePromptSeen | null;
  config: PromptConfig | null;
  save: (next: PromptConfig) => void;
  /** Forget clears the part you picked, and closes the dialog that asks. */
  setPointing: (pointing: Pointing) => void;
  setConfirmForget: (open: boolean) => void;
  /** Counts each new prompt state. */
  refresh: number;
  env: BandEnv;
  cellW: number;
  measure: (label: string) => number;
}

/** The capture steps of the card on `step`. The hook tells the card
 *  whether the Forsaken Lands rules hold, whether the game sent its codes
 *  and whether you chose the code reader, and gives it the body of the
 *  step it is on, the marks that step puts on the game's line and the
 *  Line triggers your first capture matched. The card calls resetSteps
 *  as it opens again for another profile, changeCodes and
 *  chooseCodeReader from More, and forget once you confirm Forget. */
export function useCaptureSteps({
  session,
  step,
  setStep,
  state,
  knownHost,
  game,
  config,
  save,
  setPointing,
  setConfirmForget,
  refresh,
  env,
  cellW,
  measure,
}: StepsCard) {
  // The code reader for another game, once you choose it in More.
  const [codesChosen, setCodesChosen] = useState(false);
  const [entryCodes, setEntryCodes] = useState<{ prompt: string; fprompt: string } | null>(null);
  const [request, setRequest] = useState<CodesRequest | null>(null);
  const [pointed, setPointed] = useState<PointedLine | null>(null);
  const [pickFrom, setPickFrom] = useState(0);
  const [newest, setNewest] = useState<PromptCheckRead | null>(null);
  // The game's newest line, which the setting step marks whole while no
  // row shows it.
  const [newestLine, setNewestLine] = useState<string | null>(null);
  // The line Vosh proposes as your prompt, and the one you name, as the
  // terminal marks them.
  const [pointShown, setPointShown] = useState<ScreenAsk | null>(null);
  const [lineTriggers, setLineTriggers] = useState<PromptLineTrigger[]>([]);

  const forsaken =
    codesChosen || (state?.forsaken ?? false) || knownHost || config?.capture.kind === 'aabahran';
  const gameSent = (state?.new_build ?? false) || game !== null;

  // What you gave the steps goes as the card opens again for another
  // profile. One identity, so the card's open effect can list it and
  // still run only as the card mounts.
  const resetSteps = useCallback(() => {
    setRequest(null);
    setEntryCodes(null);
    setCodesChosen(false);
    setPointed(null);
    setLineTriggers([]);
  }, []);

  /** Change codes… in More: the setting step with the codes the profile holds. */
  const changeCodes = () => {
    setEntryCodes(
      config?.capture.kind === 'aabahran'
        ? { prompt: config.capture.prompt, fprompt: config.capture.fprompt }
        : null,
    );
    setStep('codes-entry');
  };

  /** Use Forsaken Lands prompt codes… in More, on another host. */
  const chooseCodeReader = () => {
    setCodesChosen(true);
    setEntryCodes(null);
    setRequest(null);
    setStep(codeReaderStep(gameSent && game !== null));
  };

  // The codes Vosh reads: the ones the game sent on the new build, or
  // the ones you told it.
  const codes: CodesRequest | null =
    gameSent && game && !request
      ? {
          prompt: game.prompt,
          fprompt: game.fprompt,
          typed: false,
          source: 'gmcp',
          seenAt: localStamp(new Date(game.receivedAt)),
        }
      : request;

  /** Save a capture, and the first time this profile reads your prompt,
   *  name the Line triggers that matched it, since they no longer see
   *  it. */
  const saveCapture = (next: PromptCapture) => {
    if (!config) return;
    const first = firstCapture(config.capture);
    save(withCapture(config, next));
    setStep('start');
    if (first) {
      void promptLineTriggers(next, session)
        .then(setLineTriggers)
        .catch(() => setLineTriggers([]));
    }
  };

  const useCodes = (report: PromptCompileReport) => {
    if (!codes) return;
    saveCapture(savedCapture(report, codes.source, codes.seenAt));
  };

  const useNames = (report: PromptCompileReport) => {
    const shape = report.shapes[0];
    if (!shape) return;
    saveCapture({
      kind: 'regex',
      lines: shape.lines,
      settle: shape.settle,
      names: report.names,
      seen_at: localStamp(new Date()),
      source: 'session',
    });
  };

  const forget = () => {
    setConfirmForget(false);
    if (!config) return;
    const next: PromptConfig = { ...config, capture: { kind: 'none' } };
    save(next);
    setRequest(null);
    setPointing(NOWHERE);
    setStep(openingStep({ capture: next.capture, forsaken, gameSent }));
  };

  // The marks on your prompt: your design's parts while the card draws
  // it, or the values Vosh reads on the game's own line while it reads
  // your codes.
  const openRow = state?.open_row ?? null;
  // With no row open, as before the profile reads a prompt, the marks
  // find the game's line on screen.
  const noRow = openRow === null;
  useEffect(() => {
    if (step !== 'codes-entry' || !noRow) return;
    let alive = true;
    void promptCandidates(session)
      .then((groups) => {
        const entries = groups.flatMap((g) => g.entries);
        const latest = entries.reduce<(typeof entries)[number] | null>(
          (a, b) => (a === null || b.id > a.id ? b : a),
          null,
        );
        if (alive) setNewestLine(latest?.plain ?? null);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [step, noRow, refresh, session]);
  const screen = useMemo<ScreenAsk | null>(() => {
    if (step === 'point' || step === 'name') return pointShown;
    if (!noRow) return null;
    if (step === 'codes-entry' && newestLine) {
      return { lines: newestLine.split('\n'), mode: 'tail', marks: wholeMarks };
    }
    if (step === 'codes' && newest) {
      return {
        lines: newest.plain.split('\n'),
        mode: 'tail',
        marks: (shown) => rawMarks({ raw_lines: shown, raw_from: 0 }, newest, false),
      };
    }
    return null;
  }, [step, noRow, newestLine, newest, pointShown]);
  const showPick = useCallback(
    (plain: string | null) =>
      setPointShown(plain ? { lines: [plain], mode: 'shape', marks: wholeMarks } : null),
    [],
  );
  const showNames = useCallback(
    (shown: { line: string; named: boolean[] } | null) =>
      setPointShown(
        shown
          ? {
              lines: [shown.line],
              mode: 'shape',
              marks: (found) => numberMarks(found[0] ?? '', shown.named),
            }
          : null,
      ),
    [],
  );

  let body: ReactNode = null;
  if (config && step) {
    switch (step) {
      case 'codes-entry':
        body = (
          <CodesEntry
            key="codes-entry"
            session={session}
            initial={entryCodes}
            onRead={(next) => {
              setRequest(next);
              setStep('codes');
            }}
            onPoint={() => setStep('point')}
            onGameSent={() => {
              setRequest(null);
              setStep('codes');
            }}
          />
        );
        break;
      case 'codes':
        body = codes ? (
          <CodesRead
            session={session}
            request={codes}
            sourceLine={codes.source === 'gmcp' ? codesSourceLine(game) : null}
            capture={config.capture}
            secondary={
              codes.source === 'gmcp'
                ? { label: 'Point at the line instead', onClick: () => setStep('point') }
                : {
                    label: 'Change codes',
                    onClick: () => {
                      setEntryCodes({ prompt: codes.prompt, fprompt: codes.fprompt });
                      setStep('codes-entry');
                    },
                  }
            }
            onUse={useCodes}
            onNewest={setNewest}
            refresh={refresh}
            env={env}
            cellW={cellW}
            measure={measure}
          />
        ) : null;
        break;
      case 'point':
        body = (
          <PointPick
            session={session}
            start={pickFrom}
            onRead={(line, group) => {
              setPointed(line);
              setPickFrom(group);
              setStep('name');
            }}
            onShow={showPick}
          />
        );
        break;
      case 'name':
        body = pointed ? (
          <PointName
            session={session}
            line={pointed}
            onUse={useNames}
            onPickAnother={() => {
              setPickFrom((g) => g + 1);
              setStep('point');
            }}
            refresh={refresh}
            env={env}
            cellW={cellW}
            measure={measure}
            onShow={showNames}
          />
        ) : null;
        break;
    }
  }

  const raw =
    step === 'codes-entry' && openRow
      ? rawMarks(openRow, null, true)
      : step === 'codes' && openRow
        ? rawMarks(openRow, newest, false)
        : null;

  return {
    forsaken,
    gameSent,
    codesChosen,
    openRow,
    screen,
    raw,
    body,
    lineTriggers,
    setLineTriggers,
    resetSteps,
    changeCodes,
    chooseCodeReader,
    forget,
  };
}
