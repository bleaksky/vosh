import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from 'react';
import { moveTriggerToPrompts } from '../automation/automationTriggers';
import { vitalsLegacyText, type CardBinding } from './cardBinding';
import type { CellSize } from './pinnedDock';
import {
  type CardRequest,
  cardShowState,
  headerButtons,
  moreItems,
  openingStep,
  savedForName,
  cardNames,
  vitalsStartRows,
  VITALS_MORE,
  withDesign,
  withShow,
  withStart,
  type CardStep,
  type MoreItemId,
} from './cardRules';
import { followCardProfile } from './promptCardSync';
import { warnedPieces } from './promptWarn';
import { LAYOUT_TOKENS, type LayoutId } from './pickerRows';
import {
  deleteOp,
  insertOps,
  insertPlace,
  NOWHERE,
  pickAnnouncement,
  pickable,
  step as stepPick,
  type Pointing,
} from './promptPieces';
import { notMatchingLine, shownPreview } from './promptSettings';
import { sessionIdentityGet, type SessionIdentity } from '../ipc/characters';
import { profilesList } from '../ipc/profiles';
import {
  onPromptState,
  onPromptStatus,
  promptCodeReaderSet,
  promptCompile,
  promptDesignsList,
  promptStateGet,
  promptWatch,
  type PromptDesign,
  type PromptPreset,
  type PromptState,
  type PromptShowState,
} from '../ipc/prompt';
import {
  promptDescribe,
  promptEdit,
  promptPreviewSet,
  type PromptDescribed,
  type PromptFormatChoice,
  type PromptPreviewName,
} from '../ipc/promptDesign';
import { useEscape } from '../lib/escapeStack';
import { keepFocus, type FocusKeeper } from '../lib/focusKeeper';
import { useGamePrompt } from '../stores/gmcp/gamePromptStore';
import { useVitalsOptions } from '../stores/config/vitalsOptionsStore';
import { setVitalsCardMarks } from '../stores/session/vitalsCardStore';
import { VOSH_VITALS_TEXT } from '../ipc/vitals';
import { openSettingsTab } from '../lib/settingsLink';
import { formatSettingsTarget } from '../lib/settingsNav';
import { pushToast } from '../stores/toasts';
import { useBandEnv } from './useBandEnv';
import { useCaptureSteps } from './useCaptureSteps';
import { useCardPlace } from './useCardPlace';
import { useDesignEdits } from './useDesignEdits';
import { getSessions } from '../stores/session/sessionsStore';
import { useCellWidth, useLabelMeasure } from '../lib/useCellWidth';
import { knownWorld } from '../lib/knownWorlds';
import { ConfirmDialog } from '../ui/ConfirmDialog';
import type { TerminalHandle } from '../terminal/terminalHandle';
import { Button, CloseIcon, IconButton, MoreIcon } from '../ui';
import { CardMenu, MenuSeparator } from './CardMenu';
import { LineTriggers } from './PromptCodes';
import { PromptMarks } from './PromptMarks';
import { PromptPicker } from './PromptPicker';
import { PromptPieceBody } from './PromptPiece';
import { DesignFoot, TextFoot } from './PromptFoot';
import { DrawOff, Starts } from './PromptStarts';
import { PromptText } from './PromptText';

// The prompt card (section 7 of the prompt build spec). It opens from the
// terminal menu on any row, the palette, or Customize… in Settings, over
// your prompt: 4 px above the row right above it in the text and lifted,
// 4 px above the band while it is pinned, and over the last row while no
// prompt is open. It is saved for the character that owns the profile.
//
// With no capture it walks you through the capture steps: the codes the
// game sent (P3), your setting when the game sent none (P2), or the line
// another game prints (P15). Then it offers designs to start from (P4),
// and on every later open it rests with the Presets menu. Click a part of
// your prompt to change it (P5, P7, P10), or past its end to place the
// caret, add a value there with Insert value… (P6), or edit the design as
// text (P9). Left and Right pick parts, Option with them moves one,
// Delete removes it, typing adds text at the caret and Return a line
// break. Every change saves as you make it, and Command Z takes the last
// one back. While it reads your codes your prompt shows the line the game
// sent with the values it reads marked, and once it draws your design it
// labels each value with nothing to show, so you can point at it, over
// the band of Lifted in the text. Closing it puts your live prompt back.
// At its foot, beside Draw your prompt, a button picks where your prompt
// shows, and the card moves with your prompt to the place you pick. A
// menu before Done picks the preview while drawing is on (DesignFoot).
//
// Bound to your vitals text (VITALS_TEXT_BINDING) it is titled Your
// vitals text (Vitals Styles Q10). It reads no prompt, so it has no
// capture steps and rests at once, and it keeps the parts, Insert
// value…, Edit as text, Command Z, the Preview menu and Done. Its
// Presets are Vosh's text, Yours, Your text before that and Your 0.7
// text, and its foot says where the text draws in place of Draw your
// prompt and where your prompt shows. It floats over the terminal 12 px
// from the panel, its foot over the input band, and rings the part you
// pick on the footer, where a click on a part turns the card to it.
//
// The card works on one session's prompt, the selected session's, and
// names that session on every call, so an edit or a save still under
// way when you select another session lands on the session it began in.
// MainWindow mounts the card again for each session a selection brings
// to the front.

/** Where the card reaches the terminal it sits over. */
export interface PromptCardHost {
  terminal: () => TerminalHandle | null;
  /** The terminal area, which the card keeps inside. */
  area: () => HTMLElement | null;
  /** The pinned band's dock, while your prompt shows pinned. */
  dock: () => HTMLElement | null;
}

/** What the card shows of your design: the parts, the picker, or the
 *  text. */
export type CardView = 'design' | 'picker' | 'text';

interface PromptCardProps {
  /** The session whose prompt the card works on. */
  session: number;
  /** The table the card edits and saves. */
  binding: CardBinding;
  host: PromptCardHost;
  show: PromptShowState | null;
  cell: CellSize | null;
  /** The terminal's face, which the card sets prompts in. */
  monoFamily: string;
  themeTerminalColors: boolean;
  brightBold: boolean;
  renderer: 'xterm' | 'native';
  /** What the card was asked to open on: the design's parts, its text
   *  as Edit prompt as text… asks, or pointing at the line your game
   *  prints as Point at it again… in Settings asks. A request that comes
   *  while the card is open takes it there. */
  opening: CardRequest;
  /** The card draws your design over the band of Lifted in the text, so
   *  the page turns the band pass on while it does. */
  onBand?: (on: boolean) => void;
  /** Done on your prompt's card, before it closes. Get started counts
   *  it as Customize your prompt. */
  onPromptDone?: () => void;
  onClose: () => void;
}

/** What the Lament preview hides, under the card at rest (P8c). */
const LAMENT_NOTE =
  "Lament hides your vitals, your tank's health, your opponent's health, your affects and your group. Vosh draws ? where the game hides a value.";

export function PromptCard({
  session,
  binding,
  host,
  show,
  cell,
  monoFamily,
  themeTerminalColors,
  brightBold,
  renderer,
  opening,
  onBand,
  onPromptDone,
  onClose,
}: PromptCardProps) {
  const cardRef = useRef<HTMLDivElement | null>(null);
  const [state, setState] = useState<PromptState | null>(null);
  const [identity, setIdentity] = useState<SessionIdentity | null>(null);
  const [active, setActive] = useState('default');
  const [knownHost, setKnownHost] = useState(false);
  const [step, setStep] = useState<CardStep | null>(null);
  const [refresh, setRefresh] = useState(0);
  const [preview, setPreview] = useState<PromptPreviewName>('now');
  const previewRef = useRef<PromptPreviewName>('now');
  const [moreAt, setMoreAt] = useState<HTMLElement | null>(null);
  const [confirmForget, setConfirmForget] = useState(false);
  const [presets, setPresets] = useState<PromptPreset[]>([]);
  const [designs, setDesigns] = useState<PromptDesign[]>([]);
  const vitals = binding.kind === 'vitals';
  // Your 0.7 text, among the vitals text's Presets.
  const [legacy, setLegacy] = useState<string | null>(null);
  const [view, setView] = useState<CardView>(opening.view === 'text' ? 'text' : 'design');
  // Edit prompt as text… asks for the text field, so what you type goes to
  // your design, as the card opens or while it is open.
  const [textFocus, setTextFocus] = useState(opening.view === 'text' ? 1 : 0);
  // The view the picker goes back to, and adds into.
  const [pickerFor, setPickerFor] = useState<'design' | 'text'>('design');
  const [pointing, setPointing] = useState<Pointing>(NOWHERE);
  const [described, setDescribed] = useState<{
    template: string;
    data: PromptDescribed;
  } | null>(null);
  const insertRef = useRef<((token: string) => void) | null>(null);
  // Where the caret was in Edit as text, kept while Insert value… is open.
  const textCaret = useRef<{ start: number; end: number } | null>(null);
  const { config, take, opens, forgetEdits, save, takeBack, edit, movePicked } = useDesignEdits(
    session,
    binding.write,
    setDescribed,
    setPointing,
    previewRef,
  );
  const game = useGamePrompt();
  const env = useBandEnv(themeTerminalColors, brightBold, renderer);
  const cellW = useCellWidth(monoFamily);
  const measure = useLabelMeasure(11);
  const {
    forsaken,
    gameSent,
    codesChosen,
    openRow,
    screen,
    raw,
    body: stepBody,
    lineTriggers,
    setLineTriggers,
    resetSteps,
    changeCodes,
    chooseCodeReader,
    forget,
  } = useCaptureSteps({
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
  });

  // Lament leaves with the Forsaken Lands rules, and the card draws Now
  // until they come back, as Settings does. Your pick stays, so a
  // profile that opens again keeps Lament, and the Preview menu always
  // names the preview it draws.
  const drawn = shownPreview(preview, forsaken);
  previewRef.current = drawn;

  // The step the card was asked to open on, read once as it opens.
  const opensOn = useRef(opening.view === 'point' ? ('point' as const) : undefined);

  // Open: keep the design among the earlier ones, read the state, and
  // name who the card saves for. When another profile becomes active the
  // card opens again for it, from its first step, with nothing to take
  // back, and saves nothing until it has read that profile's table.
  useEffect(() => {
    let alive = true;
    const open = (first: boolean) => {
      const at = ++opens.current;
      if (!first) {
        forgetEdits();
        setStep(null);
        setPointing(NOWHERE);
        setView('design');
        resetSteps();
        setMoreAt(null);
        setConfirmForget(false);
      }
      void Promise.all([
        binding.open(session),
        promptStateGet(session),
        sessionIdentityGet(session).catch(() => null),
        profilesList().catch(() => null),
      ])
        .then(([opened, now, who, list]) => {
          if (!alive || at !== opens.current) return;
          // The profile the session's row names, which the app may not
          // have made active yet when you just selected the session.
          const played = getSessions().find((row) => row.id === session)?.profile;
          const name = played ?? list?.active ?? who?.profile ?? 'default';
          const entry = list?.profiles.find((p) => p.name === name);
          const host = who?.host ?? entry?.auto_match?.host ?? '';
          const known = knownWorld(host) !== undefined;
          take(opened);
          setState(now);
          setIdentity(who);
          setActive(name);
          setKnownHost(known);
          // A vitals text reads no prompt, so it rests at once.
          setStep(
            vitals
              ? 'rest'
              : ((first ? opensOn.current : undefined) ??
                  openingStep({
                    capture: opened.capture,
                    forsaken: now.forsaken || known || opened.capture.kind === 'aabahran',
                    gameSent: now.new_build,
                  })),
          );
        })
        .catch((e: unknown) => console.error('[prompt card] opening failed', e));
    };
    open(true);
    void promptWatch(true, session).catch(() => {});
    const unlisteners: (() => void)[] = [];
    const keep = (p: Promise<() => void>) =>
      void p.then((fn) => (alive ? unlisteners.push(fn) : fn())).catch(() => {});
    keep(
      followCardProfile({
        reopen: () => open(false),
        identity: (who) => setIdentity(who),
      }),
    );
    keep(
      onPromptState((next, from) => {
        if (from !== session) return;
        setState(next);
        setRefresh((n) => n + 1);
      }),
    );
    // Whether Vosh reads your prompt changes between prompts too, such as
    // when three in a row did not match.
    keep(
      onPromptStatus((status, from) => {
        if (from === session) setState((now) => (now ? { ...now, status } : now));
      }),
    );
    keep(
      binding.follow(session, (next) => {
        if (alive) take(next);
      }),
    );
    return () => {
      alive = false;
      for (const fn of unlisteners) fn();
      void promptWatch(false, session).catch(() => {});
      // Your live prompt comes back as the card closes.
      void promptPreviewSet(null, session).catch(() => {});
    };
  }, [session, binding, vitals, forgetEdits, opens, resetSteps, take]);

  // A request while the card is open: Point at it again… in Settings,
  // Edit prompt as text… in the palette, or Customize prompt… again.
  const firstRequest = useRef(opening.at);
  const asked = opening.view;
  useEffect(() => {
    if (opening.at === firstRequest.current) return;
    if (asked === 'point') {
      if (vitals) return;
      setStep((now) => (now === null ? now : 'point'));
      return;
    }
    setView(asked);
    if (asked === 'text') setTextFocus((n) => n + 1);
  }, [opening.at, asked, vitals]);

  // The code reader you chose on another host gives it the Forsaken Lands
  // rules while the card stays open, so the game's reply to prompt fills
  // P2's fields (D17). Closing the card lets it go.
  useEffect(() => {
    void promptCodeReaderSet(codesChosen, session).catch(() => {});
  }, [codesChosen, session]);
  useEffect(() => () => void promptCodeReaderSet(false, session).catch(() => {}), [session]);

  // While the card reads your codes your prompt shows the line the game
  // sent, so its marks sit on it. Once it draws your design, each value
  // with nothing to show draws its label, and the footer's preview runs.
  const reading = step === 'codes-entry' || step === 'codes' || step === 'point' || step === 'name';
  // A vitals text leaves your prompt live and previews on the footer.
  useEffect(() => {
    if (step === null || vitals) return;
    void promptPreviewSet(
      reading ? { raw: true } : { placeholders: true, preview: drawn === 'now' ? null : drawn },
      session,
    ).catch(() => {});
  }, [step, reading, drawn, session, vitals]);

  // Past the capture steps the card works on your design: as text even
  // with drawing off (P11), and on your prompt while drawing is on.
  const editing = step === 'start' || step === 'rest';
  const designing = editing && (config?.draw ?? false);
  // Where the marks go: your prompt in the terminal, or for a vitals
  // text the footer.
  const marksOnPrompt = designing && !vitals;

  // The card draws your design over the band of Lifted in the text.
  useEffect(() => {
    onBand?.(marksOnPrompt && (show?.show ?? 'text') === 'text');
  }, [marksOnPrompt, show?.show, onBand]);
  useEffect(() => () => onBand?.(false), [onBand]);

  // The text view needs a design to work on.
  useEffect(() => {
    if (step !== null && !editing && view !== 'design') setView('design');
  }, [step, editing, view]);

  // The designs to start from, for the capture the profile holds.
  const capture = config?.capture;
  useEffect(() => {
    if (vitals || !capture || (step !== 'start' && step !== 'rest')) return;
    let alive = true;
    const request =
      capture.kind === 'aabahran'
        ? promptCompile(
            { kind: 'aabahran', prompt: capture.prompt, fprompt: capture.fprompt },
            session,
          )
        : promptCompile(
            {
              kind: 'regex',
              lines: capture.kind === 'regex' ? capture.lines : [],
              names: capture.kind === 'regex' ? (capture.names ?? {}) : {},
            },
            session,
          );
    void Promise.all([request, promptDesignsList().catch(() => [])])
      .then(([report, others]) => {
        if (!alive) return;
        setPresets(report.presets);
        setDesigns(others);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [capture, step, session, vitals]);

  // Your 0.7 text, for a vitals text's Presets.
  useEffect(() => {
    if (!vitals) return;
    let alive = true;
    void vitalsLegacyText()
      .then((text) => alive && setLegacy(text))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [vitals]);

  // What each part of your design is, with what it reads in the preview.
  const template = config?.template ?? '';
  useEffect(() => {
    if (step !== 'start' && step !== 'rest') return;
    let alive = true;
    void promptDescribe(template, drawn === 'now' ? null : drawn, null, session)
      .then((data) => {
        if (alive) setDescribed({ template, data });
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [template, step, drawn, refresh, session]);

  // The parts as last described. Right after a change they can trail the
  // design for a moment, and the card keeps showing them meanwhile, so the
  // part you work on never blinks away or loses focus.
  const pieces = useMemo(() => described?.data.pieces ?? [], [described]);
  const fresh = described?.template === template;
  // A part the design no longer has is no longer picked.
  useEffect(() => {
    if (pointing.picked !== null && fresh) {
      if (!pickable(pieces).includes(pointing.picked)) setPointing(NOWHERE);
    }
  }, [pieces, pointing.picked, fresh]);

  // Parts no value fills: a code your prompt in the game does not show,
  // or a name Vosh has no value for.
  const warn = useMemo(
    () => warnedPieces(pieces, described?.data.tokens ?? [], state?.catalog ?? []),
    [pieces, described, state?.catalog],
  );

  // A vitals text's own Presets.
  const vitalsStarts = useMemo(
    () => (vitals && config ? vitalsStartRows(config, VOSH_VITALS_TEXT, legacy) : undefined),
    [vitals, config, legacy],
  );

  const anchor = useCardPlace(host, cell, show, vitals, { step, refresh, view, template, drawn });

  // The footer rings the part you pick in a vitals text and draws the
  // preview, and a click on a part there turns the card to it.
  const pickRef = useRef<(piece: number) => void>(() => {});
  pickRef.current = (piece) => {
    setView('design');
    setPointing({ picked: piece, caret: null });
  };
  const ringed = view === 'design' ? pointing.picked : null;
  useEffect(() => {
    if (!vitals || step === null) return;
    setVitalsCardMarks({
      template,
      picked: ringed,
      preview: drawn,
      pick: (piece) => pickRef.current(piece),
    });
  }, [vitals, step, template, ringed, drawn]);
  useEffect(() => () => setVitalsCardMarks(null), []);
  const shownIn = useVitalsOptions().place;

  // Focus moves into the card so the keyboard reaches it, unless a field
  // in it took focus first.
  useEffect(() => {
    const card = cardRef.current;
    if (step !== null && card && !card.contains(document.activeElement)) {
      card.focus({ preventScroll: true });
    }
  }, [step === null]); // eslint-disable-line react-hooks/exhaustive-deps

  // And stays there when the control that had it goes away, as a part's
  // rows remount, Remove takes its button, the picker closes or a step
  // gives way to the next.
  const keeper = useRef<FocusKeeper | null>(null);
  useEffect(() => {
    const card = cardRef.current;
    if (!card) return;
    const kept = keepFocus(card);
    keeper.current = kept;
    return () => {
      kept.stop();
      keeper.current = null;
    };
  }, []);
  useLayoutEffect(() => keeper.current?.check());

  // Escape closes a menu, then the picker, then the card. A menu and the
  // confirm dialog take it first through their own handlers.
  useEscape(true, () => {
    if (view === 'picker') {
      setView(pickerFor);
      return;
    }
    onClose();
  });

  const more = vitals
    ? VITALS_MORE
    : config && step
      ? moreItems({ step, forsaken, gameSent, capture: config.capture })
      : [];
  const buttons = step ? headerButtons(step, more) : { editAsText: false, more: false };
  const saved = savedForName(identity, active);
  const owner = saved.replace(/^Saved for /, '');

  const runMore = (id: MoreItemId) => {
    // Focus goes back to More first, so the dialog Forget opens hands it
    // back there, and a step that keeps More keeps it.
    moreAt?.focus({ preventScroll: true });
    setMoreAt(null);
    switch (id) {
      case 'change-codes':
        changeCodes();
        return;
      case 'point':
        setStep('point');
        return;
      case 'use-codes':
        chooseCodeReader();
        return;
      case 'forget':
        setConfirmForget(true);
        return;
      case 'customize-vitals':
        openSettingsTab(formatSettingsTarget({ group: 'layout', section: 'customize-vitals' }));
        return;
    }
  };

  // The picker's search takes focus when you reach Insert value… from
  // the keyboard, so you type at once. A click leaves it at rest.
  const [pickerKeys, setPickerKeys] = useState(false);
  const openPicker = (from: 'design' | 'text') => {
    const opener = document.activeElement;
    setPickerKeys(opener instanceof HTMLElement && opener.matches(':focus-visible'));
    setPickerFor(from);
    setView('picker');
  };

  const place = insertPlace(pieces, pointing);

  /** A value the picker chose goes in at the caret, or into the text. */
  const insertValue = (field: string, format: PromptFormatChoice) => {
    if (pickerFor === 'text') {
      setView('text');
      void promptEdit('', { op: 'insert_field', at: 0, field, format }, session)
        .then((result) => insertRef.current?.(result.template))
        .catch((e: unknown) => pushToast({ kind: 'error', message: String(e) }));
      return;
    }
    setView('design');
    edit(insertOps(pieces, place, field, format));
  };

  const insertLayout = (id: LayoutId) => {
    if (pickerFor === 'text') {
      setView('text');
      const token = LAYOUT_TOKENS[id];
      // The field takes the token once it shows again.
      requestAnimationFrame(() => insertRef.current?.(token));
      return;
    }
    setView('design');
    if (id === 'space') {
      edit([{ op: 'insert_text', at: place, text: ' ' }], 'caret');
    } else if (id === 'nl') {
      edit([{ op: 'insert_nl', at: place }]);
    } else if (id === 'right') {
      edit([{ op: 'insert_right', at: place }]);
    } else {
      // A break, then In a fight on it.
      edit([
        { op: 'insert_nl', at: place },
        { op: 'set_when', piece: place, when: 'fight' },
      ]);
    }
  };

  // The keys that work on your design (section 7.1): Left and Right pick
  // parts, Option with them moves one, Delete removes it, typing adds text
  // at the caret, and Return adds a line break.
  const designKeys = (e: KeyboardEvent<HTMLDivElement>) => {
    if (!designing || view !== 'design') return false;
    const target = e.target as HTMLElement;
    const typingInField =
      target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement ||
      target.isContentEditable;
    if (typingInField) return false;
    const onCard = target === cardRef.current;
    // A control you just used keeps focus, so Delete and typing reach the
    // design from it too. Space and Return stay the control's own.
    const onButton = target instanceof HTMLButtonElement;
    const mod = e.metaKey || e.ctrlKey;
    if ((e.key === 'ArrowLeft' || e.key === 'ArrowRight') && !mod) {
      const dir = e.key === 'ArrowLeft' ? -1 : 1;
      if (e.altKey) {
        movePicked(pieces, pointing.picked, dir);
      } else {
        setPointing(stepPick(pieces, pointing, dir));
      }
      return true;
    }
    // The rest work at the caret or the part you picked.
    if (!(onCard || onButton) || mod) return false;
    if (pointing.picked === null && pointing.caret === null) return false;
    if (e.key === 'Backspace' || e.key === 'Delete') {
      const op = deleteOp(pieces, pointing, e.key === 'Backspace' ? -1 : 1);
      if (op) edit([op]);
      return true;
    }
    if (onButton && (e.key === 'Enter' || e.key === ' ')) return false;
    if (e.key === 'Enter') {
      edit([{ op: 'insert_nl', at: place }]);
      return true;
    }
    if (e.key.length === 1 && !e.altKey) {
      edit([{ op: 'insert_text', at: place, text: e.key }], 'caret');
      return true;
    }
    return false;
  };

  const pickedPiece =
    view === 'design' && pointing.picked !== null
      ? (pieces.find((p) => p.piece === pointing.picked) ?? null)
      : null;

  let body: ReactNode = null;
  if (config && step) {
    switch (step) {
      case 'codes-entry':
      case 'codes':
      case 'point':
      case 'name':
        body = stepBody;
        break;
      case 'start':
      case 'rest': {
        // With drawing off the card says so at rest, and Edit as text
        // still works on the design you keep (P11).
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
        body = (
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
        break;
      }
    }
  }

  const names = cardNames(binding.kind);
  const title = names.title;
  const header =
    view === 'picker' && editing ? (
      <div className="pc-head">
        <Button onClick={() => setView(pickerFor)}>Back</Button>
        <h2 className="pc-title is-picker">Insert value</h2>
        <span className="pc-spacer" />
        <IconButton label="Close" icon={<CloseIcon />} onClick={onClose} />
      </div>
    ) : (
      <div className="pc-head">
        <h2 className="pc-title">{title}</h2>
        <span className="pc-saved">{saved}</span>
        <span className="pc-spacer" />
        {buttons.editAsText && editing && (
          <Button onClick={() => setView(view === 'text' ? 'design' : 'text')}>
            {view === 'text' ? 'Edit pieces' : 'Edit as text'}
          </Button>
        )}
        {buttons.more && (
          <IconButton
            label={names.options}
            icon={<MoreIcon />}
            aria-haspopup="menu"
            aria-expanded={moreAt !== null}
            onClick={(e) => setMoreAt(moreAt ? null : e.currentTarget)}
          />
        )}
        <IconButton label="Close" icon={<CloseIcon />} onClick={onClose} />
      </div>
    );

  return (
    <>
      {step && !vitals && (
        <PromptMarks
          host={host}
          show={show}
          cell={cell}
          openRow={openRow}
          design={marksOnPrompt ? { pieces, pointing, warn } : null}
          raw={marksOnPrompt ? null : raw}
          screen={marksOnPrompt ? null : screen}
          refresh={refresh}
          onPoint={(next) => {
            if (view === 'text') setView('design');
            setPointing(next);
          }}
          card={() => cardRef.current}
        />
      )}
      <div
        ref={cardRef}
        className="pc-card st-controls"
        role="dialog"
        aria-label={title}
        tabIndex={-1}
        style={{
          ...(anchor && 'right' in anchor ? { right: anchor.right } : { left: anchor?.left ?? 12 }),
          bottom: anchor?.bottom ?? 0,
          maxHeight: anchor?.maxHeight,
          visibility: anchor && step ? 'visible' : 'hidden',
        }}
        onMouseUp={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          const mod = e.metaKey || e.ctrlKey;
          const field =
            e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
          if (mod && !e.shiftKey && e.key.toLowerCase() === 'z' && !field) {
            e.preventDefault();
            takeBack();
            return;
          }
          if (designKeys(e)) e.preventDefault();
        }}
      >
        {header}
        <div className="pc-rule" aria-hidden="true" />
        {body}
        {/* The part you pick on your prompt, for a reader, since the marks
            on the terminal are pictures. */}
        <p className="st-visually-hidden" aria-live="polite">
          {pickAnnouncement(pickedPiece)}
        </p>
      </div>
      {moreAt && more.length > 0 && (
        <CardMenu
          anchor={moreAt}
          place="below-end"
          width={forsaken ? 232 : 264}
          label={names.options}
          onClose={() => setMoreAt(null)}
        >
          {more.map((item, i) =>
            item === 'separator' ? (
              <MenuSeparator key={`sep-${i}`} />
            ) : (
              <li key={item.id} role="none">
                <button
                  type="button"
                  role="menuitem"
                  className={`ov-menu-item${item.danger ? ' is-danger' : ''}`}
                  onClick={() => runMore(item.id)}
                >
                  <span className="ov-menu-label">{item.label}</span>
                </button>
              </li>
            ),
          )}
        </CardMenu>
      )}
      {confirmForget && (
        <ConfirmDialog
          title="Forget your game's prompt?"
          body={`Vosh stops reading your prompt for ${owner}, and the game's own prompt shows again. Your design stays saved.`}
          confirmLabel="Forget"
          onConfirm={forget}
          onCancel={() => setConfirmForget(false)}
        />
      )}
    </>
  );
}
