// The prompt card's body past the capture steps, as it starts and at
// rest: the draw off note, the picker, the text, the part you picked or
// the starts, over the card's foot. A plain function the card calls in
// its body, with no hooks, so the tree React reconciles is the card's.

import type { ReactNode } from 'react';
import { moveTriggerToPrompts } from '../automation/automationTriggers';
import type {
  PromptConfig,
  PromptDesign,
  PromptLineTrigger,
  PromptPreset,
  PromptShowState,
  PromptState,
} from '../ipc/prompt';
import type {
  PromptDescribed,
  PromptEditOp,
  PromptFormatChoice,
  PromptPiece,
  PromptPreviewName,
} from '../ipc/promptDesign';
import type { BandEnv } from '../terminal/bandCells';
import { cardShowState, withDesign, withShow, withStart } from './cardRules';
import type { LayoutId } from './pickerRows';
import { LineTriggers } from './PromptCodes';
import { DesignFoot, TextFoot } from './PromptFoot';
import { PromptPicker } from './PromptPicker';
import { PromptPieceBody } from './PromptPiece';
import { NOWHERE, type Pointing } from './promptPieces';
import { notMatchingLine } from './promptSettings';
import { DrawOff, Starts, type StartChoices } from './PromptStarts';
import { PromptText } from './PromptText';
import type { CardView } from './PromptCard';

/** What the Lament preview hides, under the card at rest. */
const LAMENT_NOTE =
  "Lament hides your vitals, your tank's health, your opponent's health, your affects and your group. Vosh draws ? where the game hides a value.";

/** What the card hands its body at rest. */
export interface RestBody {
  step: 'start' | 'rest';
  config: PromptConfig;
  view: CardView;
  session: number;
  state: PromptState | null;
  show: PromptShowState | null;
  vitals: boolean;
  forsaken: boolean;
  drawn: PromptPreviewName;
  env: BandEnv;
  cellW: number;
  refresh: number;
  owner: string;
  confirmForget: boolean;
  setConfirmForget: (on: boolean) => void;
  pickerKeys: boolean;
  insertValue: (field: string, format: PromptFormatChoice) => void;
  insertLayout: (id: LayoutId) => void;
  openPicker: (from: 'design' | 'text') => void;
  described: { template: string; data: PromptDescribed } | null;
  insertRef: { current: ((token: string) => void) | null };
  textCaret: { current: { start: number; end: number } | null };
  textFocus: number;
  setTextFocus: (n: number) => void;
  pickedPiece: PromptPiece | null;
  setPointing: (pointing: Pointing) => void;
  presets: PromptPreset[];
  designs: PromptDesign[];
  vitalsStarts: StartChoices | undefined;
  lineTriggers: PromptLineTrigger[];
  setLineTriggers: (update: (list: PromptLineTrigger[]) => PromptLineTrigger[]) => void;
  shownIn: string;
  save: (next: PromptConfig, keepUndo?: boolean, asIs?: boolean) => void;
  edit: (ops: PromptEditOp[]) => void;
  setPreview: (preview: PromptPreviewName) => void;
  onPromptDone?: (() => void) | undefined;
  onClose: () => void;
}

/** The card's body as it starts and at rest. */
export function restBody({
  step,
  config,
  view,
  session,
  state,
  show,
  vitals,
  forsaken,
  drawn,
  env,
  cellW,
  refresh,
  owner,
  confirmForget,
  setConfirmForget,
  pickerKeys,
  insertValue,
  insertLayout,
  openPicker,
  described,
  insertRef,
  textCaret,
  textFocus,
  setTextFocus,
  pickedPiece,
  setPointing,
  presets,
  designs,
  vitalsStarts,
  lineTriggers,
  setLineTriggers,
  shownIn,
  save,
  edit,
  setPreview,
  onPromptDone,
  onClose,
}: RestBody): ReactNode {
  // With drawing off the card says so at rest, and Edit as text
  // still works on the design you keep.
  const drawOff = !config.draw && step === 'rest' && view === 'design';
  let content: ReactNode;
  if (drawOff) {
    content = (
      <DrawOff
        name={owner}
        other={!forsaken}
        confirming={confirmForget}
        onForget={() => setConfirmForget(true)}
      />
    );
  } else if (view === 'picker' && state) {
    content = (
      <PromptPicker
        session={session}
        state={state}
        preview={drawn}
        env={env}
        cellW={cellW}
        refresh={refresh}
        onInsert={insertValue}
        onInsertLayout={insertLayout}
        focusSearch={pickerKeys}
      />
    );
  } else if (view === 'text') {
    content = (
      <PromptText
        template={config.template}
        tokens={described?.data.tokens ?? []}
        describedFor={described?.template ?? ''}
        onChange={(next) => save(withDesign(config, next))}
        onCaretPiece={(piece) => setPointing({ picked: piece, caret: null })}
        onInsertValue={() => openPicker('text')}
        insertRef={insertRef}
        caretRef={textCaret}
        focusRequest={textFocus}
        onFocusTaken={() => setTextFocus(0)}
        fieldLabel={vitals ? 'Vitals text' : undefined}
      />
    );
  } else if (pickedPiece) {
    content = (
      <PromptPieceBody
        key={pickedPiece.piece}
        piece={pickedPiece}
        env={env}
        onEdit={(op) => edit([op])}
        onInsertValue={() => openPicker('design')}
      />
    );
  } else {
    content = (
      <Starts
        session={session}
        mode={step}
        config={config}
        presets={presets}
        designs={designs}
        own={vitalsStarts}
        values={(state?.packages.length ?? 0) > 0 ? 'live' : 'sample'}
        refresh={refresh}
        env={env}
        cellW={cellW}
        restHint={vitals ? 'Click any part of your vitals to change it.' : undefined}
        note={step === 'rest' && drawn === 'lament' ? LAMENT_NOTE : null}
        promptsOff={
          !vitals && (state?.status.status === 'prompts_off' || (show?.promptsOff ?? false))
        }
        notMatching={
          !vitals && state?.status.status === 'not_matching'
            ? notMatchingLine(state.status.last_match_at)
            : null
        }
        onPick={(row) => {
          setPointing(NOWHERE);
          // Picking a start is how you ask Vosh to draw it, so
          // drawing turns on. Same as the game follows the game,
          // and Start empty keeps its empty design.
          save(withStart(config, row), true, row.template === '');
        }}
        onInsertValue={() => openPicker('design')}
      >
        <LineTriggers
          triggers={lineTriggers}
          onMove={async (name) => {
            await moveTriggerToPrompts(name);
            setLineTriggers((list) => list.filter((t) => t.name !== name));
          }}
        />
      </Starts>
    );
  }
  return (
    <>
      {content}
      <div className="pc-rule" aria-hidden="true" />
      {vitals ? (
        <TextFoot
          note={shownIn === 'status' ? 'Draws in your status line' : 'Draws in your panel'}
          preview={drawn}
          forsaken={forsaken}
          onPreview={setPreview}
          onDone={onClose}
        />
      ) : (
        <DesignFoot
          draw={config.draw}
          onDraw={(draw) => save({ ...config, draw })}
          show={config.show}
          showState={cardShowState(show, config.capture)}
          onShow={(place) => save(withShow(config, place))}
          preview={drawn}
          forsaken={forsaken}
          onPreview={setPreview}
          onDone={() => {
            onPromptDone?.();
            onClose();
          }}
        />
      )}
    </>
  );
}
