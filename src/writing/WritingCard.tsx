import { useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { createPortal } from 'react-dom';
import type { Draft, JobResult, WriteJob, WritingKind } from '../ipc/writing';
import { sendInput } from '../ipc/session';
import { stopAskingToPost } from '../ipc/uiConfig';
import { useEscape } from '../lib/escapeStack';
import type { PromptCardHost } from '../prompt/PromptCard';
import type { CellSize } from '../prompt/pinnedDock';
import { useBandEnv } from '../prompt/useBandEnv';
import { useColorVision } from '../theme/fitGameColors';
import { themeTokens } from '../theme/themes';
import { useActiveTheme } from '../theme/useActiveTheme';
import { useCharStatus } from '../stores/gmcp/charStatusStore';
import { useRoom } from '../stores/gmcp/roomStore';
import { useSessionConnection } from '../stores/session/connectionStore';
import { useSelectedRow } from '../stores/session/sessionsStore';
import { useWriting } from '../stores/session/writingStore';
import { pushToast } from '../stores/toasts';
import { Button } from '../ui';
import { ConfirmDialog } from '../ui/ConfirmDialog';
import { applicationGuide } from './applications';
import { boxInks } from './boxInks';
import { stopsAsking, useAskPost } from './askPost';
import { DontAskAgain } from './DontAskAgain';
import {
  characterOf,
  getWritingFile,
  keepCharacter,
  keepSwitches,
  loadWriting,
  newDraft,
  onlyDraft,
  posted,
  useWritingFile,
  withDraft,
  type World,
} from './draftsStore';
import { BEAST_LOOKS, hasBeast, keepsCodes, KINDS, switchOf, widthOf } from './kinds';
import {
  cutLine,
  count,
  rewrapAll,
  rewrapParagraph,
  sameLines,
  spamRun,
  storedBytes,
  type Row,
} from './text';
import { endJob } from './writingJobEnd';
import { kindsMenuFor, moreItemsFor, type MenuCard } from './writingMenus';
import { WritingBox, type BoxText, type PasteNote } from './WritingBox';
import { WritingFields, type FieldName } from './WritingFields';
import { FootCount, FootNote, WritingFoot } from './WritingFoot';
import { WritingGuide } from './WritingGuide';
import { WritingPreview } from './WritingPreview';
import { WritingHead, type MoreItem } from './WritingHead';
import { findToStart, type Drop, type Find } from './cardDrop';
import { footFor, type Ended, type FootAction } from './cardFoot';
import {
  checkAsk,
  clearOtherAsk,
  postAsk,
  postStillAsks,
  readAgainAsk,
  type Ask,
} from './cardDialogs';
import { checkable } from './cardMenus';
import { useWritingJob, type JobSpec } from './useWritingJob';
import { useWritingFrame } from './useWritingFrame';
import { CARD_MARGIN } from './cardPlace';
import { BoxGrip, RowGrip } from './WritingGrips';
import { countLine, lineNote, metaLine, pasteNote, previewLine, roomFor, type Note } from './words';

// The writing card. One card for every text the game's line editor
// takes, your description, a note on any board, your history, with the
// kind in its title. It floats over the terminal on the prompt card's
// recipe, its foot over the six newest rows, and keeps each draft for
// its character in writing.toml as you type. Send to game and Post… run
// a job in the session's writer, which drives the game's editor, and
// the game's answers show in the rows under the card. You can drag it
// anywhere in the window by its header, drag its box taller or shorter
// by the grip on its free edge, and pin it to the panel, where it fills
// its own pane (cardPlace.ts, pinnedPane.ts). The card draws through a
// portal into a box of its own, which moves between the window and the
// pane's slot, so pinning never starts it over.

/** What the card was asked to open on. */
export interface WritingRequest {
  kind: WritingKind;
  /** The offer to take, when the card opens from the notice. */
  offer?: number;
  /** Each request counts, so the open card hears a repeat. */
  n: number;
}

interface Props {
  session: number;
  request: WritingRequest;
  host: PromptCardHost;
  cell: CellSize | null;
  fontFamily: string;
  fontSize: number;
  themeTerminalColors: boolean;
  brightBold: boolean;
  renderer: 'xterm' | 'native';
  onClose: () => void;
}

interface Confirm extends Ask {
  run: () => void;
  /** The confirm offers Don't ask again, which turns Ask before you
   *  post off. */
  skip?: boolean;
}

const rowsOf = (text: readonly string[]): Row[] =>
  text.map((line) => ({ text: line, flows: false }));
const linesOf = (rows: readonly Row[]): string[] => rows.map((r) => r.text);

export function WritingCard({
  session,
  request,
  host,
  cell,
  fontFamily,
  fontSize,
  themeTerminalColors,
  brightBold,
  renderer,
  onClose,
}: Props) {
  // The card opens once writing.toml is read, so it opens on your draft.
  const [ready, setReady] = useState(false);
  useEffect(() => {
    let alive = true;
    void loadWriting().then(() => alive && setReady(true));
    return () => {
      alive = false;
    };
  }, []);
  const file = useWritingFile();
  const row = useSelectedRow();
  const connection = useSessionConnection();
  const status = useCharStatus();
  const room = useRoom();
  const writing = useWriting();
  const env = useBandEnv(themeTerminalColors, brightBold, renderer);
  // The marks and the selection in the box, each readable on its ground.
  const theme = useActiveTheme();
  const vision = useColorVision();
  const inks = useMemo(
    () => boxInks({ ground: env.bg, ...themeTokens(theme, vision) }),
    [env.bg, theme, vision],
  );

  // Another character's drafts, opened from Other characters, wait for
  // a session that plays them.
  const [other, setOther] = useState<{ world: World; name: string } | null>(null);
  const playing = connection.status.kind === 'connected' && connection.character !== null;
  const name = other?.name ?? connection.character ?? row?.character ?? null;
  const world: World | null =
    other?.world ?? (row?.host && row.port ? { host: row.host, port: row.port } : null);
  const live =
    playing && (other === null || other.name.toLowerCase() === connection.character?.toLowerCase());
  const character = world && name ? characterOf(file, world, name) : null;
  const level = status.level ?? character?.level ?? null;
  const race = status.race ?? character?.race ?? null;
  const immortal = keepsCodes(level);

  // Keep the race and level last seen, so a login that sends no
  // Char.Status keeps the Beast switch.
  const worldHost = world?.host ?? null;
  const worldPort = world?.port ?? null;
  useEffect(() => {
    if (worldHost === null || worldPort === null || !name || !live || status.level === null) return;
    const now = characterOf(getWritingFile(), { host: worldHost, port: worldPort }, name);
    if (now.level === status.level && now.race === status.race) return;
    keepCharacter({ ...now, level: status.level, race: status.race });
  }, [worldHost, worldPort, name, live, status.level, status.race]);

  const [kind, setKind] = useState<WritingKind>(request.kind);
  const info = KINDS[kind];
  const [draft, setDraft] = useState<Draft>(() => openDraft(request.kind));
  const [rows, setRows] = useState<Row[]>(() => rowsOf(draft.text));
  const [box, setBox] = useState<BoxText>({ rows, revision: 0 });
  const [caretRow, setCaretRow] = useState(0);
  const [ended, setEnded] = useState<Ended | null>(null);
  const [phase, setPhase] = useState<'edit' | 'sent' | 'posted' | 'checked'>('edit');
  const [readNow, setReadNow] = useState(false);
  const [adopt, setAdopt] = useState(false);
  const [paste, setPaste] = useState<Note | null>(null);
  const [preview, setPreview] = useState(false);
  const [folded, setFolded] = useState(false);
  const [confirm, setConfirm] = useState<Confirm | null>(null);
  // Don't ask again under Post…'s confirm.
  const [skipAsk, setSkipAsk] = useState(false);
  const askPost = useAskPost();
  const [badField, setBadField] = useState<FieldName | null>(null);
  const [sentView, setSentView] = useState(false);
  const [dropped, setDropped] = useState<Drop | null>(null);
  // A drop after the post went out, which a look at the board's list
  // settles once the session plays again.
  const [find, setFind] = useState<Find | null>(null);

  /** The draft of `k` to open on: the newest of a board's, or the one a
   *  text about you keeps. */
  function openDraft(k: WritingKind): Draft {
    if (!world || !name) return newDraft(k);
    const c = characterOf(getWritingFile(), world, name);
    if (KINDS[k].board) {
      return (
        c.drafts.find((d) => d.kind === k) ??
        newDraft(k, KINDS[k].room ? (room.info?.name ?? null) : null)
      );
    }
    return onlyDraft(c, k);
  }

  /** Put `lines` in the box, from outside it. */
  const show = (lines: readonly string[], next: Row[] = rowsOf(lines)) => {
    setRows(next);
    setBox((b) => ({ rows: next, revision: b.revision + 1 }));
  };

  /** Keep the draft, now and in writing.toml. A text about you keeps
   *  one draft, which it replaces. */
  const keep = (next: Draft) => {
    setDraft(next);
    if (!world || !name || sentView) return;
    const c = characterOf(getWritingFile(), world, name);
    const kept = KINDS[next.kind].board
      ? c
      : { ...c, drafts: c.drafts.filter((d) => d.kind !== next.kind || d.id === next.id) };
    keepCharacter(withDraft(kept, next));
  };

  const width = widthOf(kind === 'application' && draft.custom_race === true);
  const helpWidth = info.helpWidth || (kind === 'application' && draft.custom_race === true);
  const lines = linesOf(rows);
  const counted = count(lines, width);
  const roomLeft = roomFor(kind);
  const cut = storedBytes(lines) > roomLeft ? cutLine(lines, roomLeft) : null;
  const spam = info.board ? spamRun(lines) : null;

  // ── A job's end ───────────────────────────────────────────────────
  const done = (result: JobResult, job: WriteJob) =>
    endJob(result, job, {
      find,
      lines,
      kind,
      draft,
      world,
      name,
      openDraft,
      switchTo,
      show,
      keep,
      markPosted,
      run,
      saveShown,
      setBadField,
      setDropped,
      setFind,
      setEnded,
      setReadNow,
      setAdopt,
      setConfirm,
      setPhase,
    });
  const jobs = useWritingJob(session, writing, done);
  const running = jobs.running;

  // ── Opening ───────────────────────────────────────────────────────
  const opened = useRef(-1);
  useEffect(() => {
    if (!ready || opened.current === request.n) return;
    opened.current = request.n;
    const d = openDraft(request.kind);
    switchTo(request.kind, d);
    if (request.offer !== undefined) {
      jobs.take(request.offer, { kind: request.kind, action: 'read', name });
      return;
    }
    readIfNoDraft(request.kind, d);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [request.n, ready]);

  /** With no draft, a text the game saves in place reads from the game
   *  once its prompt shows. */
  function readIfNoDraft(k: WritingKind, d: Draft) {
    if (KINDS[k].board || d.text.length > 0 || !live) return;
    jobs.run({ kind: k, action: 'read', name });
  }

  /** Open the card on another kind, its draft and a clean footer. */
  function switchTo(k: WritingKind, d: Draft = openDraft(k)) {
    setKind(k);
    setDraft(d);
    show(d.text);
    setEnded(null);
    setPaste(null);
    setPhase('edit');
    setReadNow(false);
    setAdopt(false);
    setBadField(null);
    setSentView(false);
    setDropped(null);
    setFind(null);
    setPreview(false);
  }

  /** The note is on its board: it moves to Sent. */
  function markPosted() {
    if (world && name) keepCharacter(posted(characterOf(getWritingFile(), world, name), draft));
    setPhase('posted');
  }

  // Look for the note once the session plays again. Vosh never sends it
  // again on its own.
  useEffect(() => {
    const now = findToStart(find, live, writing.job !== null);
    if (!now) return;
    setFind({ ...now, started: true });
    run({ kind, action: 'find', name, subject: now.subject, immortal, baseline: now.baseline });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [find, live, writing.job]);

  /** Keep a note the game held as a draft of its board. */
  function saveShown(
    k: WritingKind,
    note: { subject: string; to: string; language: string | null; lines: string[] },
  ) {
    if (!world || !name) return;
    const d: Draft = {
      ...newDraft(k),
      to: note.to,
      subject: note.subject,
      language: note.language,
      text: note.lines,
    };
    keepCharacter(withDraft(characterOf(getWritingFile(), world, name), d));
  }

  const run = (spec: JobSpec) => {
    setEnded(null);
    setPaste(null);
    jobs.run(spec);
  };

  const baseJob = (): JobSpec => ({ kind, action: 'send', lines, name, immortal });

  const sendToGame = (text: string[] = lines) =>
    run({ ...baseJob(), lines: text, base: draft.game ?? null });

  // Post asks first, unless Ask before you post is off and the post
  // loses nothing you could want back.
  const postAsks = askPost || postStillAsks(kind, draft.room ?? null, room.info?.name ?? null);
  const post = () => {
    const go = () =>
      run({
        ...baseJob(),
        action: 'post',
        to: info.toImmortal ? 'immortal' : (draft.to ?? ''),
        subject: draft.subject ?? '',
        language: info.language ? (draft.language ?? null) : null,
        adopt,
        // After a drop the game holds the note the card put there,
        // which Post again clears with no question.
        clear_first: ended?.actions.includes('again') ?? false,
      });
    if (!postAsks) {
      go();
      return;
    }
    setSkipAsk(false);
    setConfirm({
      ...postAsk(kind, draft.to ?? '', draft.room ?? null, room.info?.name ?? null),
      run: go,
      skip: askPost,
    });
  };

  const check = () => {
    if (!canCheck) return;
    setConfirm({
      ...checkAsk(kind, counted),
      run: () => run({ kind: kind === 'description' ? kind : 'history', action: 'check', name }),
    });
  };

  const readAgain = () => {
    const go = () => run({ kind, action: 'read', name });
    if (draft.game && sameLines(lines, draft.game)) go();
    else setConfirm({ ...readAgainAsk(kind), run: go });
  };

  const restore = () => {
    if (draft.game) sendToGame(draft.game);
  };

  // ── Closing ───────────────────────────────────────────────────────
  const close = () => {
    const kept = lines.some((l) => l.trim().length > 0) && phase === 'edit' && !sentView;
    const differs = !draft.game || !sameLines(lines, draft.game);
    if (kept && differs && name) {
      pushToast({
        kind: 'info',
        message: info.board
          ? `Saved ${name}’s ${info.title.toLowerCase()}`
          : `Saved ${name}’s draft`,
        meta: `${counted.lines} ${counted.lines === 1 ? 'line' : 'lines'}, not ${info.board ? 'posted' : 'sent'} yet`,
      });
    }
    onClose();
  };
  useEscape(confirm === null, close);

  // ── The box ───────────────────────────────────────────────────────
  const onBoxChange = (next: Row[]) => {
    setRows(next);
    setEnded(null);
    setPaste(null);
    setReadNow(false);
    setDropped(null);
    if (phase !== 'edit') setPhase('edit');
    keep({ ...draft, text: linesOf(next) });
  };

  const rewrapAt = () => show([], rewrapParagraph(rows, caretRow, width));
  const rewrapEvery = () => {
    const next = rewrapAll(rows, width);
    show([], next);
    keep({ ...draft, text: linesOf(next) });
  };

  // ── Where it sits and how big ─────────────────────────────────────
  const guideOn = file.guide && !preview;
  const {
    place,
    prefs,
    docked,
    awaitingPane,
    moving,
    setSizing,
    setSizingCols,
    viewH,
    narrow,
    px,
    lineH,
    colW,
    fieldLanguage,
    fieldRoom,
    cardRef,
    cardRefOf,
    colsFit,
    boxCols,
    boxWidth,
    fit,
    boxRows,
    boxMinRows,
    movedAt,
    putBack,
    togglePin,
    drag,
    gripEdge,
    hostEl,
  } = useWritingFrame({
    host,
    cell,
    fontFamily,
    fontSize,
    guideOn,
    info,
    draft,
    roomName: room.info?.name ?? null,
    lineCount: lines.length,
    folded,
    preview,
  });

  // ── Footer ────────────────────────────────────────────────────────
  const busy = writing.game === 'editor' && !running;
  const empty = counted.lines === 0;
  const canSend =
    live && !busy && !running && cut === null && !(kind === 'beast' && empty) && !sentView;
  const fieldsSet =
    (info.toImmortal || (draft.to ?? '').trim().length > 0) &&
    (draft.subject ?? '').trim().length > 0;
  const flagged =
    running || sentView
      ? null
      : lineNote(rows[caretRow]?.text ?? '', caretRow, width, helpWidth, immortal);
  const matches = readNow && !empty && !!draft.game && sameLines(lines, draft.game);
  const canCheck = checkable({ live, running: running !== null, matches, phase, game: draft.game });
  const foot = footFor({
    kind,
    running,
    ended,
    paste,
    over: cut === null ? null : storedBytes(lines) - roomLeft,
    spam,
    flagged,
    busy,
    count: countLine(kind, counted),
    live,
    phase,
    sentView,
    canSend,
    canPost: canSend && fieldsSet && spam === null,
    finding: find !== null,
    matches,
    hasGame: !!draft.game,
    asksPost: postAsks,
  });
  const footActions: Record<FootAction, () => void> = {
    stop: jobs.stop,
    undo: () => document.execCommand('undo'),
    rewrap: rewrapAt,
    'clear-other': () => {
      const other = ended?.other;
      if (!other) return;
      setConfirm({
        ...clearOtherAsk(other),
        run: () => run({ kind: other, action: 'clear', name }),
      });
    },
    restore,
    done: close,
    post,
    send: () => sendToGame(),
  };
  const left =
    'progress' in foot.left ? (
      <span className="pc-foot-note">{foot.left.progress}</span>
    ) : 'note' in foot.left ? (
      <FootNote note={foot.left.note} />
    ) : (
      <FootCount {...foot.left.count} />
    );
  const buttons = foot.buttons.map((b) => (
    <Button
      key={b.id}
      variant={b.primary ? 'primary' : 'secondary'}
      disabled={b.disabled === true}
      onClick={footActions[b.id]}
    >
      {b.label}
    </Button>
  ));

  // ── Header ────────────────────────────────────────────────────────
  // Description and Beast for a werebeast past level 15, and History,
  // Personality and Purpose for everyone.
  const switchKinds =
    switchOf(kind)?.includes('beast') && !hasBeast(race, level) ? null : switchOf(kind);

  const meta = metaLine({
    name: name ?? 'you',
    board: info.board,
    job: running,
    read: readNow,
    fresh: draft.text.length === 0 && !draft.subject,
    done: phase === 'posted' ? 'posted' : phase === 'sent' || phase === 'checked' ? 'sent' : null,
    dropped,
  });

  const menuCard: MenuCard = {
    character,
    draft,
    level,
    file,
    roomName: room.info?.name ?? null,
    kind,
    lines,
    world,
    name,
    sentView,
    live,
    running,
    canCheck,
    openDraft,
    switchTo,
    readIfNoDraft,
    keep,
    show,
    rewrapEvery,
    readAgain,
    check,
    setSentView,
    setOther,
    setPreview,
    setConfirm,
  };
  const kindsMenu = kindsMenuFor(menuCard);
  const moreItems = moreItemsFor(menuCard);

  const more: MoreItem[] =
    !docked && prefs.left !== null && prefs.top !== null
      ? [...moreItems, 'separator', { id: 'put-back', label: 'Put the card back', run: putBack }]
      : moreItems;

  // ── Sizing ────────────────────────────────────────────────────────
  const grip = gripEdge && (
    <RowGrip edge={gripEdge} boxRows={boxRows} fit={fit} lineH={lineH} setSizing={setSizing} />
  );
  const boxGrip = gripEdge && (
    <BoxGrip
      boxRows={boxRows}
      boxCols={boxCols}
      fit={fit}
      colsFit={colsFit}
      lineH={lineH}
      colW={colW}
      narrow={narrow}
      setSizing={setSizing}
      setSizingCols={setSizingCols}
    />
  );

  const guide =
    kind === 'application'
      ? applicationGuide(draft.subject ?? '', draft.custom_race === true)
      : info.guide;

  // Read help folds the card to its header while the game prints the
  // help, keeping its top where it was.
  const [foldTop, setFoldTop] = useState<number | null>(null);
  const help = () => {
    void sendInput(`help ${guide.help}`, session).catch(() => {});
    setFoldTop(cardRef.current?.getBoundingClientRect().top ?? null);
    setFolded(true);
  };

  if (!world || !name) {
    return (
      <div
        className="pc-card st-controls wr-card"
        role="dialog"
        aria-label="Write"
        style={{ left: 12, bottom: 120 }}
      >
        <div className="pc-head">
          <h2 className="pc-title">Write</h2>
          <span className="pc-spacer" />
          <Button onClick={onClose}>Close</Button>
        </div>
        <div className="pc-rule" />
        <div className="pc-body">
          <p className="pc-copy">
            Log in with a character first. Vosh keeps your writing separate for each one.
          </p>
        </div>
      </div>
    );
  }

  const placed: CSSProperties = docked
    ? {}
    : movedAt
      ? { left: movedAt.left, top: movedAt.top, maxHeight: viewH - 2 * CARD_MARGIN }
      : {
          ...(place?.right !== null && place?.right !== undefined
            ? { left: place.left, right: place.right }
            : { left: place?.left ?? 12 }),
          ...(folded && foldTop !== null
            ? { top: foldTop }
            : { bottom: place?.bottom ?? 0, maxHeight: place?.maxHeight }),
        };
  const style: CSSProperties = {
    ...placed,
    visibility: (place || docked) && ready && prefs.loaded && !awaitingPane ? 'visible' : 'hidden',
    ['--wr-px' as string]: `${px}px`,
    ['--wr-lh' as string]: `${lineH}px`,
    ['--wr-cols' as string]: boxCols,
    ['--wr-family' as string]: fontFamily,
    ['--wr-fg' as string]: env.fg,
    ['--wr-ground' as string]: env.bg,
  };

  // A confirm sits over the card's foot, 12 in from its right.
  const cardBox = confirm ? cardRef.current?.getBoundingClientRect() : null;
  const confirmAt = cardBox
    ? {
        right: window.innerWidth - cardBox.right + 12,
        bottom: window.innerHeight - cardBox.bottom + 60,
      }
    : null;

  const sending =
    running && running.stage === 'sending'
      ? { sent: running.sent, current: running.sent }
      : !running && dropped
        ? { sent: dropped.sent, current: null }
        : null;
  const subject = draft.subject ?? '';

  return (
    <>
      {createPortal(
        <div
          ref={cardRefOf}
          className={`pc-card st-controls wr-card${folded ? ' is-folded' : ''}${docked ? ' is-pinned' : ''}${moving ? ' is-moving' : ''}`}
          role="dialog"
          aria-label={info.title}
          tabIndex={-1}
          style={style}
          onMouseUp={(e) => e.stopPropagation()}
        >
          <WritingHead
            kind={kind}
            title={info.title}
            switchKinds={switchKinds}
            onSwitch={(k) => {
              switchTo(k);
              readIfNoDraft(k, openDraft(k));
            }}
            meta={meta}
            guide={file.guide}
            preview={preview}
            onPreview={() => setPreview(false)}
            onGuide={() => keepSwitches(file.spelling, !file.guide)}
            folded={folded}
            onUnfold={() => setFolded(false)}
            kinds={kindsMenu}
            more={more}
            moreLabel={`${info.title} options`}
            pinned={prefs.pinned}
            onPin={togglePin}
            drag={drag}
            onClose={close}
          />
          {!folded && (
            <>
              <div className="pc-rule" aria-hidden="true" />
              <div className="wr-main">
                <div className="pc-body wr-body">
                  {info.board && (
                    <WritingFields
                      to={draft.to ?? ''}
                      toFixed={info.toImmortal}
                      subject={subject}
                      language={fieldLanguage}
                      room={fieldRoom}
                      bad={badField}
                      readOnly={running !== null || sentView}
                      onTo={(to) => {
                        setBadField(null);
                        setEnded(null);
                        setDropped(null);
                        keep({ ...draft, to });
                      }}
                      onSubject={(s) => {
                        setBadField(null);
                        setEnded(null);
                        setDropped(null);
                        keep({ ...draft, subject: s });
                      }}
                      onLanguage={(language) => {
                        setBadField(null);
                        setEnded(null);
                        setDropped(null);
                        keep({ ...draft, language });
                      }}
                      onText={() =>
                        document.querySelector<HTMLElement>('.wr-card .cm-content')?.focus()
                      }
                    />
                  )}
                  {preview ? (
                    <WritingPreview
                      head={
                        info.board
                          ? [
                              `${name}: ${subject}`,
                              `To: ${info.toImmortal ? 'Immortal' : (draft.to ?? '')}`,
                            ]
                          : []
                      }
                      lines={lines}
                      palette={env.palette}
                      immortal={immortal}
                      width={boxWidth}
                    />
                  ) : (
                    <WritingBox
                      text={box}
                      empty={
                        kind === 'beast' && lines.every((l) => l.length === 0)
                          ? {
                              says: `Lookers see the game’s own line for your ${character?.beast ?? 'beast'}.`,
                              line: character?.beast
                                ? (BEAST_LOOKS[character.beast] ?? null)
                                : null,
                            }
                          : null
                      }
                      width={width}
                      helpWidth={helpWidth}
                      immortal={immortal}
                      spellcheck={file.spelling}
                      readOnly={running !== null || sentView}
                      sending={sending}
                      cut={cut}
                      palette={env.palette}
                      label={info.title}
                      rows={boxRows}
                      minRows={boxMinRows}
                      cols={boxCols}
                      inks={inks}
                      corner={boxGrip}
                      onChange={onBoxChange}
                      onCaret={setCaretRow}
                      onPaste={(p: PasteNote) =>
                        setPaste(pasteNote(p.wrapped, width, p.folded, p.word))
                      }
                    />
                  )}
                </div>
                {guideOn && <WritingGuide guide={guide} onHelp={help} />}
              </div>
              {running && running.total > 0 ? (
                <div
                  className="wr-progress"
                  role="progressbar"
                  aria-valuenow={running.sent}
                  aria-valuemax={running.total}
                >
                  <i style={{ width: `${(100 * running.sent) / running.total}%` }} />
                </div>
              ) : (
                <div className="pc-rule" aria-hidden="true" />
              )}
              <WritingFoot
                left={preview ? <span className="pc-foot-note">{previewLine(kind)}</span> : left}
                right={buttons}
              />
            </>
          )}
          {grip}
        </div>,
        hostEl,
      )}
      {confirm && (
        <ConfirmDialog
          title={confirm.title}
          body={confirm.body}
          confirmLabel={confirm.label}
          {...(confirm.cancel ? { cancelLabel: confirm.cancel } : {})}
          tone={confirm.tone ?? 'danger'}
          onConfirm={() => {
            const go = confirm.run;
            if (stopsAsking(confirm.skip, skipAsk)) void stopAskingToPost().catch(() => {});
            setConfirm(null);
            go();
          }}
          onCancel={() => setConfirm(null)}
          {...(confirmAt ? { at: confirmAt } : {})}
        >
          {confirm.skip && <DontAskAgain checked={skipAsk} onChange={setSkipAsk} />}
        </ConfirmDialog>
      )}
    </>
  );
}
