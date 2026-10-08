import { useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import type { Draft, JobResult, WriteJob, WritingKind } from '../ipc/writing';
import { sendInput } from '../ipc/session';
import { useEscape } from '../lib/escapeStack';
import type { PromptCardHost } from '../prompt/PromptCard';
import type { CellSize } from '../prompt/pinnedDock';
import { useBandEnv } from '../prompt/useBandEnv';
import { useCharStatus } from '../stores/gmcp/charStatusStore';
import { useRoom } from '../stores/gmcp/roomStore';
import { useSessionConnection } from '../stores/session/connectionStore';
import { useSelectedRow } from '../stores/session/sessionsStore';
import { useWriting } from '../stores/session/writingStore';
import { pushToast } from '../stores/toasts';
import { Button } from '../ui';
import { ConfirmDialog } from '../ui/ConfirmDialog';
import { applicationGuide } from './applications';
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
  withoutDraft,
  type World,
} from './draftsStore';
import { hasBeast, keepsCodes, KINDS, switchOf, widthOf, writable } from './kinds';
import { cutLine, count, rewrapAll, rewrapParagraph, spamRun, storedBytes, type Row } from './text';
import { WritingBox, type BoxText, type PasteNote } from './WritingBox';
import { WritingFields, type FieldName } from './WritingFields';
import { FootCount, FootNote, WritingFoot } from './WritingFoot';
import { WritingGuide } from './WritingGuide';
import { WritingPreview } from './WritingPreview';
import { WritingHead, type KindsMenu, type MoreItem } from './WritingHead';
import { footFor, type Ended, type FootAction } from './cardFoot';
import {
  changedAsk,
  checkAsk,
  checkedNote,
  CLEAR_ASK,
  clearOtherAsk,
  DELETE_ASK,
  postAsk,
  postedNote,
  readAgainAsk,
  sameNoteAsk,
  sentNote,
  type Ask,
} from './cardDialogs';
import { draftRows, moreRows, otherRows, sentRows, type MoreAction } from './cardMenus';
import { useWritingJob, type JobSpec } from './useWritingJob';
import { useWritingPlace } from './useWritingPlace';
import { countLine, lineNote, metaLine, pasteNote, resultNote, roomFor, type Note } from './words';

// The writing card (Description Editor and Note Editor reviews). One card
// for every text the game's line editor takes, your description, a note
// on any board, your history, with the kind in its title. It floats over
// the terminal on the prompt card's recipe, its foot over the six newest
// rows, and keeps each draft for its character in writing.toml as you
// type. Send to game and Post… run a job in the session's writer, which
// drives the game's editor, and the game's answers show in the rows under
// the card.

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
}

const rowsOf = (text: readonly string[]): Row[] =>
  text.map((line) => ({ text: line, flows: false }));
const linesOf = (rows: readonly Row[]): string[] => rows.map((r) => r.text);
const sameLines = (a: readonly string[], b: readonly string[]) =>
  a.length === b.length &&
  a.every((line, k) => line.replace(/ +$/, '') === b[k].replace(/ +$/, ''));

/** The width of one column of the terminal face at `px`. */
function columnWidth(family: string, px: number): number {
  const canvas = document.createElement('canvas');
  const ctx = canvas.getContext('2d');
  if (!ctx) return px * 0.6;
  ctx.font = `${px}px ${family}`;
  return ctx.measureText('0'.repeat(10)).width / 10 || px * 0.6;
}

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
  useEffect(() => {
    void loadWriting();
  }, []);
  const file = useWritingFile();
  const row = useSelectedRow();
  const connection = useSessionConnection();
  const status = useCharStatus();
  const room = useRoom();
  const writing = useWriting();
  const env = useBandEnv(themeTerminalColors, brightBold, renderer);

  const live = connection.status.kind === 'connected' && connection.character !== null;
  const name = connection.character ?? row?.character ?? null;
  const world: World | null = row?.host && row.port ? { host: row.host, port: row.port } : null;
  const character = world && name ? characterOf(file, world, name) : null;
  const level = status.level ?? character?.level ?? null;
  const race = status.race ?? character?.race ?? null;
  const immortal = keepsCodes(level);

  // Keep the race and level last seen, so a login that sends no
  // Char.Status keeps the Beast switch (Description Editor Q11).
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
  const [badField, setBadField] = useState<FieldName | null>(null);
  const [sentView, setSentView] = useState(false);
  const [dropped, setDropped] = useState<{ sent: number; total: number } | null>(null);

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

  const done = (result: JobResult, job: WriteJob) => onDone(result, job);
  const jobs = useWritingJob(session, writing, done);
  const running = jobs.running;

  // ── Opening ───────────────────────────────────────────────────────
  const opened = useRef(-1);
  useEffect(() => {
    if (opened.current === request.n) return;
    const first = opened.current === -1;
    opened.current = request.n;
    if (!first) switchTo(request.kind);
    if (request.offer !== undefined) {
      jobs.take(request.offer, { kind: request.kind, action: 'read', name });
      return;
    }
    if (first) readIfNoDraft(request.kind, openDraft(request.kind));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [request.n]);

  /** With no draft, a text the game saves in place reads from the game
   *  once its prompt shows (Description Editor Q4). */
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
    setPreview(false);
  }

  // ── A job's end ───────────────────────────────────────────────────
  function onDone(result: JobResult, job: WriteJob) {
    setBadField(null);
    setDropped(null);
    const k = job.kind;
    switch (result.kind) {
      case 'read': {
        const note = result.note;
        const text = note ? note.lines : result.lines;
        const next: Draft = {
          ...(k === kind ? draft : openDraft(k)),
          kind: k,
          text,
          game: KINDS[k].board ? null : text,
          ...(note ? { to: note.to, subject: note.subject, language: note.language } : {}),
        };
        if (k !== kind) switchTo(k, next);
        else show(text);
        keep(next);
        if (result.beast && world && name) {
          const c = characterOf(getWritingFile(), world, name);
          if (c.beast !== result.beast) keepCharacter({ ...c, beast: result.beast });
        }
        setReadNow(true);
        setAdopt(note !== null);
        setEnded(null);
        return;
      }
      case 'changed':
        keep({ ...draft, game: result.lines });
        setConfirm({ ...changedAsk(k), run: () => run({ ...job, base: null }) });
        return;
      case 'sent':
        keep({ ...draft, game: result.lines });
        setPhase('sent');
        setEnded({
          note: sentNote(
            result.lines.length,
            sameLines(
              result.lines,
              lines.map((l) => l.replace(/"/g, "'")),
            ),
          ),
          actions: [],
        });
        return;
      case 'posted':
        if (world && name) keepCharacter(posted(characterOf(getWritingFile(), world, name), draft));
        setPhase('posted');
        setEnded({ note: postedNote(k, result.forum, result.vote), actions: [] });
        return;
      case 'checked':
        setPhase('checked');
        setEnded({ note: checkedNote(k, result.lines), actions: [] });
        return;
      case 'same_note':
        setConfirm({
          ...sameNoteAsk(k, result.note.subject),
          run: () => {
            saveShown(k, result.note);
            run({ ...job, clear_first: true });
          },
        });
        return;
      case 'other_note':
        if (result.board && result.note) saveShown(result.board, result.note);
        setEnded({
          note: resultNote(result, k) ?? { lead: '', rest: '', tone: 'warn' },
          actions: result.board ? ['clear-other'] : [],
          other: result.board,
        });
        return;
      case 'cleared':
        setEnded(null);
        return;
      case 'refused':
        if (result.field === 'to' || result.field === 'subject' || result.field === 'language') {
          setBadField(result.field);
        }
        setEnded({ note: resultNote(result, k)!, actions: [] });
        return;
      case 'dropped':
        if (!KINDS[k].board) setDropped({ sent: result.sent, total: lines.length });
        setEnded({
          note: resultNote(result, k)!,
          actions: KINDS[k].board ? ['again'] : ['restore', 'again'],
        });
        return;
      case 'stopped':
        setEnded({
          note: resultNote(result, k)!,
          actions: KINDS[k].board ? [] : ['restore', 'again'],
        });
        return;
      default: {
        const note = resultNote(result, k);
        if (note) setEnded({ note, actions: [] });
      }
    }
  }

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

  const post = () =>
    setConfirm({
      ...postAsk(kind, draft.to ?? '', draft.room ?? null, room.info?.name ?? null),
      run: () =>
        run({
          ...baseJob(),
          action: 'post',
          to: info.toImmortal ? 'immortal' : (draft.to ?? ''),
          subject: draft.subject ?? '',
          language: info.language ? (draft.language ?? null) : null,
          adopt,
        }),
    });

  const check = () =>
    setConfirm({
      ...checkAsk(kind, counted),
      run: () => run({ kind: kind === 'description' ? kind : 'history', action: 'check', name }),
    });

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
        message: `Draft kept for ${name}`,
        meta: `${counted.lines} ${counted.lines === 1 ? 'line' : 'lines'}, not ${info.board ? 'posted' : 'sent'}`,
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
  const narrowPx = 11;
  const [px, setPx] = useState(fontSize);
  const lineH = Math.round(px * 1.3);
  const chW = useMemo(() => columnWidth(fontFamily, px), [fontFamily, px]);
  const boxWidth = 32 + 82 * chW;
  const guideOn = file.guide && !preview;
  const cardWidth = boxWidth + 32 + (guideOn ? 248 : 0);
  const place = useWritingPlace(host, cell, cardWidth);
  useEffect(() => {
    setPx(place?.right !== null && place?.right !== undefined ? narrowPx : fontSize);
  }, [place?.right, fontSize]);
  const fieldsH = info.board ? (info.room || draft.language !== null ? 102 : 68) : 0;
  const chrome = 46 + 1 + 1 + 52 + 12 + 16 + 10 + fieldsH;
  const fit = place ? Math.floor((place.maxHeight - chrome) / lineH) : 12;
  const boxRows = Math.max(6, Math.min(Math.max(lines.length, 6), fit));

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
    matches: readNow && !empty && !!draft.game && sameLines(lines, draft.game),
    hasGame: !!draft.game,
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
    check,
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
  const switchKinds = (() => {
    const kinds = switchOf(kind);
    if (!kinds) return null;
    if (kinds.includes('beast')) return hasBeast(race, level) ? kinds : null;
    return kinds;
  })();

  const meta = metaLine({
    name: name ?? 'you',
    board: info.board,
    job: running,
    read: readNow,
    fresh: draft.text.length === 0 && !draft.subject,
    done: phase === 'posted' ? 'posted' : phase === 'sent' || phase === 'checked' ? 'sent' : null,
    dropped,
  });

  const kindsMenu: KindsMenu = {
    drafts: draftRows(character?.drafts ?? [], draft.id),
    boards: writable(level),
    aboutYou: ['description', 'history'],
    sent: sentRows(character?.sent ?? []),
    others: otherRows(file, character),
    onDraft: (id) => {
      const d = character?.drafts.find((x) => x.id === id);
      if (d) switchTo(d.kind, d);
    },
    onNew: (k) => {
      const d = KINDS[k].board
        ? newDraft(k, KINDS[k].room ? (room.info?.name ?? null) : null)
        : openDraft(k);
      switchTo(k, d);
      readIfNoDraft(k, d);
    },
    onSent: (id) => {
      const d = character?.sent.find((x) => x.id === id);
      if (!d) return;
      switchTo(d.kind, d);
      setSentView(true);
    },
    onOther: () => {
      pushToast({ kind: 'info', message: 'Play that character to open their drafts.' });
    },
  };

  const hasLanguage = draft.language !== null && draft.language !== undefined;
  const moreActions: Record<MoreAction, () => void> = {
    language: () => keep({ ...draft, language: hasLanguage ? null : '' }),
    race: () => keep({ ...draft, custom_race: !draft.custom_race }),
    rewrap: rewrapEvery,
    preview: () => setPreview(true),
    spelling: () => keepSwitches(!file.spelling, file.guide),
    copy: () => void navigator.clipboard.writeText(lines.join('\n')).catch(() => {}),
    'copy-draft': () =>
      switchTo(kind, {
        ...draft,
        ...newDraft(kind),
        to: draft.to ?? '',
        subject: draft.subject ?? '',
        text: lines,
      }),
    delete: () =>
      setConfirm({
        ...DELETE_ASK,
        run: () => {
          if (world && name)
            keepCharacter(withoutDraft(characterOf(getWritingFile(), world, name), draft.id));
          switchTo(kind, newDraft(kind));
        },
      }),
    read: readAgain,
    restore: () => {
      if (!draft.game) return;
      show(draft.game);
      keep({ ...draft, text: draft.game });
    },
    clear: () =>
      setConfirm({
        ...CLEAR_ASK,
        run: () => {
          show([]);
          keep({ ...draft, text: [] });
        },
      }),
  };
  const more: MoreItem[] = moreRows({
    kind,
    language: hasLanguage,
    customRace: draft.custom_race === true,
    spelling: file.spelling,
    sentView,
    canRead: live && running === null,
    canRestore: !!draft.game && !sameLines(lines, draft.game),
  }).map((row) => (row === 'separator' ? row : { ...row, run: moreActions[row.id] }));

  const guide =
    kind === 'application'
      ? applicationGuide(draft.subject ?? '', draft.custom_race === true)
      : info.guide;

  const help = () => {
    void sendInput(`help ${guide.help}`, session).catch(() => {});
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
            Play a character first. The card keeps your writing for each one.
          </p>
        </div>
      </div>
    );
  }

  const style: CSSProperties = {
    ...(place?.right !== null && place?.right !== undefined
      ? { left: place.left, right: place.right }
      : { left: place?.left ?? 12 }),
    bottom: place?.bottom ?? 0,
    maxHeight: place?.maxHeight,
    visibility: place ? 'visible' : 'hidden',
    ['--wr-px' as string]: `${px}px`,
    ['--wr-lh' as string]: `${lineH}px`,
    ['--wr-family' as string]: fontFamily,
    ['--wr-fg' as string]: env.fg,
    ['--wr-ground' as string]: env.bg,
  };

  const sending =
    running && running.stage === 'sending' ? { sent: running.sent, current: running.sent } : null;
  const subject = draft.subject ?? '';

  return (
    <>
      <div
        className={`pc-card st-controls wr-card${folded ? ' is-folded' : ''}`}
        role="dialog"
        aria-label={info.title}
        tabIndex={-1}
        style={style}
        onMouseUp={(e) => e.stopPropagation()}
      >
        <WritingHead
          kind={kind}
          title={info.title}
          switchKinds={switchKinds ?? (switchOf(kind)?.includes('history') ? switchOf(kind) : null)}
          onSwitch={(k) => {
            switchTo(k);
            readIfNoDraft(k, openDraft(k));
          }}
          meta={meta}
          guide={file.guide}
          onGuide={() => keepSwitches(file.spelling, !file.guide)}
          folded={folded}
          onUnfold={() => setFolded(false)}
          kinds={kindsMenu}
          more={more}
          moreLabel={`${info.title} options`}
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
                    language={info.language && draft.language !== undefined ? draft.language : null}
                    room={info.room ? (draft.room ?? room.info?.name ?? null) : null}
                    bad={badField}
                    readOnly={running !== null || sentView}
                    onTo={(to) => {
                      setBadField(null);
                      keep({ ...draft, to });
                    }}
                    onSubject={(s) => {
                      setBadField(null);
                      keep({ ...draft, subject: s });
                    }}
                    onLanguage={(language) => {
                      setBadField(null);
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
                    minRows={6}
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
              left={left}
              right={
                preview ? (
                  <Button variant="primary" onClick={() => setPreview(false)}>
                    Back to writing
                  </Button>
                ) : (
                  buttons
                )
              }
            />
          </>
        )}
      </div>
      {confirm && (
        <ConfirmDialog
          title={confirm.title}
          body={confirm.body}
          confirmLabel={confirm.label}
          {...(confirm.cancel ? { cancelLabel: confirm.cancel } : {})}
          tone={confirm.tone ?? 'danger'}
          onConfirm={() => {
            const go = confirm.run;
            setConfirm(null);
            go();
          }}
          onCancel={() => setConfirm(null)}
        />
      )}
    </>
  );
}
