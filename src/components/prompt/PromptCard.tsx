import {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from 'react';
import { moveTriggerToPrompts } from '../../lib/automationTriggers';
import { BAND_OUTSET_Y, dockGap, type CellSize } from '../../lib/promptBand';
import {
  cardAnchor,
  type CardRequest,
  codeReaderStep,
  codesSourceLine,
  firstCapture,
  headerButtons,
  localStamp,
  moreItems,
  openingStep,
  savedCapture,
  savedForName,
  takeBackOnto,
  undoEntry,
  withCapture,
  type CardStep,
  type MoreItemId,
  type UndoEntry,
} from '../../lib/promptCard';
import { followCardProfile } from '../../lib/promptCardSync';
import { numberMarks, wholeMarks, type ScreenAsk } from '../../lib/promptScreen';
import { warnedPieces } from '../../lib/promptWarn';
import { LAYOUT_TOKENS, type LayoutId } from '../../lib/promptPicker';
import {
  caretAfter,
  deleteOp,
  insertOps,
  insertPlace,
  moveBack,
  moveOp,
  pickAnnouncement,
  pickable,
  rawMarks,
  step as stepPick,
  type MoveMade,
  type Pointing,
} from '../../lib/promptPieces';
import { notMatchingLine } from '../../lib/promptSettings';
import type { PromptShowState } from '../../lib/promptShow';
import {
  onPromptState,
  onPromptStatus,
  profilesList,
  promptCandidates,
  promptCardOpen,
  promptCodeReaderSet,
  promptCompile,
  promptConfigGet,
  promptConfigSet,
  promptDescribe,
  promptDesignsList,
  promptEdit,
  promptLineTriggers,
  promptPreviewSet,
  promptStateGet,
  promptWatch,
  sessionIdentityGet,
  subscribePromptConfigChanged,
  type PromptCapture,
  type PromptCheckRead,
  type PromptCompileReport,
  type PromptConfig,
  type PromptDescribed,
  type PromptDesign,
  type PromptEditOp,
  type PromptFormatChoice,
  type PromptLineTrigger,
  type PromptPreset,
  type PromptPreviewName,
  type PromptState,
  type SessionIdentity,
} from '../../lib/session';
import { useEscape } from '../../lib/escapeStack';
import { keepFocus, type FocusKeeper } from '../../lib/focusKeeper';
import { useGamePrompt } from '../../lib/stores/gamePromptStore';
import { pushToast } from '../../lib/toasts';
import { useBandEnv } from '../../lib/useBandEnv';
import { useCellWidth, useLabelMeasure } from '../../lib/useCellWidth';
import { knownWorld } from '../../lib/useConnection';
import { ConfirmDialog } from '../ConfirmDialog';
import type { TerminalHandle } from '../Terminal';
import {
  Button,
  CloseIcon,
  IconButton,
  MoreIcon,
  Segmented,
  Toggle,
  type SegmentedOption,
} from '../settings/ui';
import { CardMenu, MenuSeparator } from './CardMenu';
import { CodesEntry, CodesRead, LineTriggers, type CodesRequest } from './PromptCodes';
import { PromptMarks } from './PromptMarks';
import { PromptPicker } from './PromptPicker';
import { PromptPieceBody } from './PromptPiece';
import { PointName, PointPick, type PointedLine } from './PromptPoint';
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
  onClose: () => void;
}

/** The previews the footer offers. Lament only under the Forsaken Lands
 *  rules, where the game hides your values under lamented tears. */
function previewOptions(forsaken: boolean): SegmentedOption<PromptPreviewName>[] {
  const options: SegmentedOption<PromptPreviewName>[] = [
    { value: 'now', label: 'Now' },
    { value: 'low_health', label: 'Low health' },
    { value: 'fight', label: 'Fight' },
  ];
  if (forsaken) options.push({ value: 'lament', label: 'Lament' });
  return options;
}

/** What the Lament preview hides, under the card at rest (P8c). */
const LAMENT_NOTE =
  "Lament hides your vitals, your tank's health, your opponent's health, your affects and your group. Vosh draws ? where the game hides a value.";

const UNDO_DEPTH = 50;
const NOWHERE: Pointing = { picked: null, caret: null };

export function PromptCard({
  host,
  show,
  cell,
  monoFamily,
  themeTerminalColors,
  brightBold,
  renderer,
  opening,
  onBand,
  onClose,
}: PromptCardProps) {
  const drawId = useId();
  const cardRef = useRef<HTMLDivElement | null>(null);
  const [config, setConfig] = useState<PromptConfig | null>(null);
  const latest = useRef<PromptConfig | null>(null);
  const [state, setState] = useState<PromptState | null>(null);
  const [identity, setIdentity] = useState<SessionIdentity | null>(null);
  const [active, setActive] = useState('default');
  const [knownHost, setKnownHost] = useState(false);
  const [step, setStep] = useState<CardStep | null>(null);
  const [refresh, setRefresh] = useState(0);
  const [preview, setPreview] = useState<PromptPreviewName>('now');
  const previewRef = useRef(preview);
  previewRef.current = preview;
  const [moreAt, setMoreAt] = useState<HTMLElement | null>(null);
  const [confirmForget, setConfirmForget] = useState(false);
  // The code reader for another game, once you choose it in More.
  const [codesChosen, setCodesChosen] = useState(false);
  const [entryCodes, setEntryCodes] = useState<{ prompt: string; fprompt: string } | null>(null);
  const [request, setRequest] = useState<CodesRequest | null>(null);
  const [pointed, setPointed] = useState<PointedLine | null>(null);
  const [pickFrom, setPickFrom] = useState(0);
  const [presets, setPresets] = useState<PromptPreset[]>([]);
  const [designs, setDesigns] = useState<PromptDesign[]>([]);
  const [anchor, setAnchor] = useState<{ left: number; bottom: number; maxHeight: number } | null>(
    null,
  );
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
  const [newest, setNewest] = useState<PromptCheckRead | null>(null);
  // The game's newest line, which P2 marks whole while no row shows it.
  const [newestLine, setNewestLine] = useState<string | null>(null);
  // The line B2 proposes, and the one P15 names, as the terminal marks
  // them.
  const [pointShown, setPointShown] = useState<ScreenAsk | null>(null);
  const [lineTriggers, setLineTriggers] = useState<PromptLineTrigger[]>([]);
  const insertRef = useRef<((token: string) => void) | null>(null);
  // Where the caret was in Edit as text, kept while Insert value… is open.
  const textCaret = useRef<{ start: number; end: number } | null>(null);
  const edits = useRef<Promise<unknown>>(Promise.resolve());
  const undo = useRef<UndoEntry[]>([]);
  // The moves Option with Left or Right made, newest last, which the
  // opposite key takes back exactly.
  const moves = useRef<MoveMade[]>([]);
  const game = useGamePrompt();
  const env = useBandEnv(themeTerminalColors, brightBold, renderer);
  const cellW = useCellWidth(monoFamily);
  const measure = useLabelMeasure(11);

  const forsaken =
    codesChosen || (state?.forsaken ?? false) || knownHost || config?.capture.kind === 'aabahran';
  const gameSent = (state?.new_build ?? false) || game !== null;

  const take = (next: PromptConfig) => {
    latest.current = next;
    setConfig(next);
  };

  // The step the card was asked to open on, read once as it opens.
  const opensOn = useRef(opening.view === 'point' ? ('point' as const) : undefined);
  // Counts each time the card opens, for the profile active then. A
  // read or a save begun for an earlier one never lands on this one.
  const opens = useRef(0);

  // Open: keep the design among the earlier ones, read the state, and
  // name who the card saves for. When another profile becomes active the
  // card opens again for it, from its first step, with nothing to take
  // back, and saves nothing until it has read that profile's table.
  useEffect(() => {
    let alive = true;
    const open = (first: boolean) => {
      const at = ++opens.current;
      if (!first) {
        latest.current = null;
        setConfig(null);
        setStep(null);
        undo.current = [];
        moves.current = [];
        setPointing(NOWHERE);
        setView('design');
        setRequest(null);
        setEntryCodes(null);
        setCodesChosen(false);
        setPointed(null);
        setLineTriggers([]);
        setMoreAt(null);
        setConfirmForget(false);
      }
      void Promise.all([
        promptCardOpen(),
        promptStateGet(),
        sessionIdentityGet().catch(() => null),
        profilesList().catch(() => null),
      ])
        .then(([opened, now, who, list]) => {
          if (!alive || at !== opens.current) return;
          const name = list?.active ?? who?.profile ?? 'default';
          const entry = list?.profiles.find((p) => p.name === name);
          const host = who?.host ?? entry?.auto_match?.host ?? '';
          const known = knownWorld(host) !== undefined;
          take(opened);
          setState(now);
          setIdentity(who);
          setActive(name);
          setKnownHost(known);
          setStep(
            (first ? opensOn.current : undefined) ??
              openingStep({
                capture: opened.capture,
                forsaken: now.forsaken || known || opened.capture.kind === 'aabahran',
                gameSent: now.new_build,
              }),
          );
        })
        .catch((e: unknown) => console.error('[prompt card] opening failed', e));
    };
    open(true);
    void promptWatch(true).catch(() => {});
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
      onPromptState((next) => {
        setState(next);
        setRefresh((n) => n + 1);
      }),
    );
    // Whether Vosh reads your prompt changes between prompts too, such as
    // when three in a row did not match.
    keep(
      onPromptStatus((status) => {
        setState((now) => (now ? { ...now, status } : now));
      }),
    );
    keep(
      subscribePromptConfigChanged(() => {
        void promptConfigGet()
          .then((next) => {
            if (alive) take(next);
          })
          .catch(() => {});
      }),
    );
    return () => {
      alive = false;
      for (const fn of unlisteners) fn();
      void promptWatch(false).catch(() => {});
      // Your live prompt comes back as the card closes.
      void promptPreviewSet(null).catch(() => {});
    };
  }, []);

  // A request while the card is open: Point at it again… in Settings,
  // Edit prompt as text… in the palette, or Customize prompt… again.
  const firstRequest = useRef(opening.at);
  const asked = opening.view;
  useEffect(() => {
    if (opening.at === firstRequest.current) return;
    if (asked === 'point') {
      setStep((now) => (now === null ? now : 'point'));
      return;
    }
    setView(asked);
    if (asked === 'text') setTextFocus((n) => n + 1);
  }, [opening.at, asked]);

  // The code reader you chose on another host gives it the Forsaken Lands
  // rules while the card stays open, so the game's reply to prompt fills
  // P2's fields (D17). Closing the card lets it go.
  useEffect(() => {
    void promptCodeReaderSet(codesChosen).catch(() => {});
  }, [codesChosen]);
  useEffect(() => () => void promptCodeReaderSet(false).catch(() => {}), []);

  // While the card reads your codes your prompt shows the line the game
  // sent, so its marks sit on it. Once it draws your design, each value
  // with nothing to show draws its label, and the footer's preview runs.
  const reading = step === 'codes-entry' || step === 'codes' || step === 'point' || step === 'name';
  useEffect(() => {
    if (step === null) return;
    void promptPreviewSet(
      reading ? { raw: true } : { placeholders: true, preview: preview === 'now' ? null : preview },
    ).catch(() => {});
  }, [step, reading, preview]);

  // Past the capture steps the card works on your design: as text even
  // with drawing off (P11), and on your prompt while drawing is on.
  const editing = step === 'start' || step === 'rest';
  const designing = editing && (config?.draw ?? false);

  // The card draws your design over the band of Lifted in the text.
  useEffect(() => {
    onBand?.(designing && (show?.show ?? 'text') === 'text');
  }, [designing, show?.show, onBand]);
  useEffect(() => () => onBand?.(false), [onBand]);

  // The text view needs a design to work on.
  useEffect(() => {
    if (step !== null && !editing && view !== 'design') setView('design');
  }, [step, editing, view]);

  // The designs to start from, for the capture the profile holds.
  const capture = config?.capture;
  useEffect(() => {
    if (!capture || (step !== 'start' && step !== 'rest')) return;
    let alive = true;
    const request =
      capture.kind === 'aabahran'
        ? promptCompile({ kind: 'aabahran', prompt: capture.prompt, fprompt: capture.fprompt })
        : promptCompile({
            kind: 'regex',
            lines: capture.kind === 'regex' ? capture.lines : [],
            names: capture.kind === 'regex' ? (capture.names ?? {}) : {},
          });
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
  }, [capture, step]);

  // What each part of your design is, with what it reads in the preview.
  const template = config?.template ?? '';
  useEffect(() => {
    if (step !== 'start' && step !== 'rest') return;
    let alive = true;
    void promptDescribe(template, preview === 'now' ? null : preview)
      .then((data) => {
        if (alive) setDescribed({ template, data });
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [template, step, preview, refresh]);

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

  // Sit over your prompt, and follow it.
  const relayout = useCallback(async () => {
    const area = host.area();
    if (!area) return;
    const rect = area.getBoundingClientRect();
    const cellH = cell?.height ?? 17.5;
    const term = host.terminal();
    let lastRowTop = rect.bottom - cellH;
    let promptTop: number | null = null;
    if (term) {
      const rows = term.getSize().rows;
      lastRowTop = term.rowTop(rows - 1) ?? lastRowTop;
      const region = await term.promptRegion().catch(() => null);
      if (region && region.atBottom) {
        const top = term.rowTop(region.row);
        if (top !== null && top >= rect.top) promptTop = top;
      }
    }
    const pinned = show?.show === 'pinned' && show.capture;
    const dock = pinned ? host.dock() : null;
    const bandRowTop = dock
      ? dock.getBoundingClientRect().top + dockGap(cellH) + BAND_OUTSET_Y
      : null;
    const placed = cardAnchor({
      pinned: Boolean(pinned),
      promptTop,
      lastRowTop,
      bandRowTop,
      cellH,
      areaTop: rect.top,
      viewportH: window.innerHeight,
    });
    setAnchor({ left: rect.left + 12, ...placed });
  }, [host, cell, show]);

  useLayoutEffect(() => {
    void relayout();
  }, [relayout, step, refresh, view, template, preview]);

  useEffect(() => {
    const onResize = () => void relayout();
    window.addEventListener('resize', onResize);
    const area = host.area();
    const observer = area ? new ResizeObserver(onResize) : null;
    if (area) observer?.observe(area);
    return () => {
      window.removeEventListener('resize', onResize);
      observer?.disconnect();
    };
  }, [host, relayout]);

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

  const save = (next: PromptConfig, keepUndo = true, asIs = false) => {
    const before = latest.current;
    if (!before) return;
    const at = opens.current;
    // Command Z takes back only what this change made.
    const entry = keepUndo ? undoEntry(before, next) : null;
    if (entry) undo.current = [...undo.current, entry].slice(-UNDO_DEPTH);
    take(next);
    void promptConfigSet(next, { asIs }).catch((e: unknown) => {
      if (at === opens.current) take(before);
      pushToast({ kind: 'error', message: String(e) });
    });
  };

  const takeBack = () => {
    const last = undo.current.pop();
    const now = latest.current;
    if (last && now) save(takeBackOnto(now, last), false, true);
  };

  /** Make `ops` one after another on the design as it stands, save the
   *  result once, and follow the part the first one acted on: pick it,
   *  or with `caret`, put the caret past it. `made` hears the design
   *  before and after, and where that part landed. Edits queue, so typing
   *  fast loses no character. */
  const edit = (
    ops: PromptEditOp[],
    follow: 'pick' | 'caret' = 'pick',
    made?: (change: { before: string; after: string; landed: number | null }) => void,
  ) => {
    edits.current = edits.current.then(async () => {
      const base = latest.current;
      const at = opens.current;
      if (!base || ops.length === 0) return;
      try {
        let text = base.template;
        let landed: number | null = null;
        for (const [i, op] of ops.entries()) {
          // Later ops of a run act on the part the first one acted on:
          // text goes right after it, and a change goes to it.
          let placed = op;
          if (i > 0 && landed !== null) {
            if (op.op === 'insert_text') placed = { ...op, at: landed + 1 };
            else if ('piece' in op) placed = { ...op, piece: landed };
          }
          const result = await promptEdit(text, placed);
          text = result.template;
          if (i === 0 || placed.op !== 'insert_text') landed = result.piece;
        }
        // The parts of the new design come with it, so the card shows
        // the part it follows at once.
        const shown = previewRef.current;
        const data = await promptDescribe(text, shown === 'now' ? null : shown).catch(() => null);
        // Another profile became active meanwhile, so the edit was for a
        // table the card no longer shows.
        if (at !== opens.current) return;
        if (text !== base.template) save({ ...base, template: text });
        if (data) setDescribed({ template: text, data });
        made?.({ before: base.template, after: text, landed });
        const first = ops[0];
        if (landed === null || first.op === 'remove') {
          setPointing({ picked: null, caret: caretAfter(first, null) });
        } else if (follow === 'caret') {
          setPointing({ picked: null, caret: caretAfter(first, landed) });
        } else {
          setPointing({ picked: landed, caret: null });
        }
      } catch (e) {
        pushToast({ kind: 'error', message: e instanceof Error ? e.message : String(e) });
      }
    });
  };

  /** Put the design back as it was before move `back`, with the part it
   *  moved picked where it was. Only while the design is still what the
   *  move made. */
  const takeMoveBack = (back: MoveMade) => {
    edits.current = edits.current.then(async () => {
      const base = latest.current;
      const at = opens.current;
      if (!base || base.template !== back.after) return;
      const shown = previewRef.current;
      const data = await promptDescribe(back.before, shown === 'now' ? null : shown).catch(
        () => null,
      );
      if (at !== opens.current) return;
      save({ ...base, template: back.before });
      if (data) setDescribed({ template: back.before, data });
      setPointing({ picked: back.from, caret: null });
    });
  };

  /** Option with Left or Right: take the last move back when the key goes
   *  the other way, or move the part you picked past its neighbor. */
  const movePicked = (dir: -1 | 1) => {
    const back = moveBack(moves.current, latest.current?.template ?? '', pointing.picked, dir);
    if (back) {
      moves.current = moves.current.slice(0, -1);
      takeMoveBack(back);
      return;
    }
    const op = moveOp(pieces, pointing.picked, dir);
    if (!op || op.op !== 'move') return;
    edit([op], 'pick', ({ before, after, landed }) => {
      if (landed === null || before === after) return;
      moves.current = [...moves.current, { before, after, from: op.piece, landed, dir }].slice(
        -UNDO_DEPTH,
      );
    });
  };

  const more =
    config && step ? moreItems({ step, forsaken, gameSent, capture: config.capture }) : [];
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
        setEntryCodes(
          config?.capture.kind === 'aabahran'
            ? { prompt: config.capture.prompt, fprompt: config.capture.fprompt }
            : null,
        );
        setStep('codes-entry');
        return;
      case 'point':
        setStep('point');
        return;
      case 'use-codes':
        setCodesChosen(true);
        setEntryCodes(null);
        setRequest(null);
        setStep(codeReaderStep(gameSent && game !== null));
        return;
      case 'forget':
        setConfirmForget(true);
        return;
    }
  };

  // The codes P3 reads: the ones the game sent on the new build, or the
  // ones you told Vosh on P2.
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
   *  name the Line triggers that matched it (D6). */
  const saveCapture = (next: PromptCapture) => {
    if (!config) return;
    const first = firstCapture(config.capture);
    save(withCapture(config, next));
    setStep('start');
    if (first) {
      void promptLineTriggers(next)
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
      void promptEdit('', { op: 'insert_field', at: 0, field, format })
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
        movePicked(dir);
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
    void promptCandidates()
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
  }, [step, noRow, refresh]);
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
              state={state}
              preview={preview}
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
              onChange={(next) => save({ ...config, template: next })}
              onCaretPiece={(piece) => setPointing({ picked: piece, caret: null })}
              onInsertValue={() => openPicker('text')}
              insertRef={insertRef}
              caretRef={textCaret}
              focusRequest={textFocus}
              onFocusTaken={() => setTextFocus(0)}
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
              mode={step}
              config={config}
              presets={presets}
              designs={designs}
              values={(state?.packages.length ?? 0) > 0 ? 'live' : 'sample'}
              refresh={refresh}
              env={env}
              cellW={cellW}
              note={step === 'rest' && preview === 'lament' ? LAMENT_NOTE : null}
              promptsOff={state?.status.status === 'prompts_off' || (show?.promptsOff ?? false)}
              notMatching={
                state?.status.status === 'not_matching'
                  ? notMatchingLine(state.status.last_match_at)
                  : null
              }
              onPick={(template) => {
                setPointing(NOWHERE);
                // Picking a start is how you ask Vosh to draw it, so
                // drawing turns on. Start empty keeps its empty design.
                save({ ...config, template, draw: true }, true, template === '');
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
            <div className="pc-foot">
              <Toggle
                id={drawId}
                checked={config.draw}
                onChange={(draw) => save({ ...config, draw })}
              />
              <label className="pc-switch" htmlFor={drawId}>
                Draw your prompt
              </label>
              <span className="pc-spacer" />
              {config.draw && (
                <Segmented
                  label="Preview"
                  options={previewOptions(forsaken)}
                  value={preview}
                  onChange={setPreview}
                />
              )}
              <Button variant="primary" className="pc-done" onClick={onClose}>
                Done
              </Button>
            </div>
          </>
        );
        break;
      }
    }
  }

  // The marks on your prompt: your design's parts while the card draws
  // it, or the values Vosh reads on the game's own line while it reads
  // your codes.

  const raw =
    step === 'codes-entry' && openRow
      ? rawMarks(openRow, null, true)
      : step === 'codes' && openRow
        ? rawMarks(openRow, newest, false)
        : null;

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
        <h2 className="pc-title">Customize prompt</h2>
        <span className="pc-saved">{saved}</span>
        <span className="pc-spacer" />
        {buttons.editAsText && editing && (
          <Button onClick={() => setView(view === 'text' ? 'design' : 'text')}>
            {view === 'text' ? 'Edit pieces' : 'Edit as text'}
          </Button>
        )}
        {buttons.more && (
          <IconButton
            label="Prompt options"
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
      {step && (
        <PromptMarks
          host={host}
          show={show}
          cell={cell}
          openRow={openRow}
          design={designing ? { pieces, pointing, warn } : null}
          raw={designing ? null : raw}
          screen={designing ? null : screen}
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
        aria-label="Customize prompt"
        tabIndex={-1}
        data-occludes-surface="true"
        style={{
          left: anchor?.left ?? 12,
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
          label="Prompt options"
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
