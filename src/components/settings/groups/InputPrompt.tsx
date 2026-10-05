import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type ReactNode,
  type Ref,
} from 'react';
import type { BandEnv } from '../../../terminal/bandCells';
import { localStamp, savedForName } from '../../../lib/promptCard';
import {
  codesMeta,
  drawDescription,
  gameBlock,
  gameCodesOf,
  gameDescription,
  lastReadLine,
  notMatchingLine,
  POINT_DESCRIPTION,
  previewHeight,
  previewMeta,
  previewOptions,
  previewRows,
  promptWorld,
  shownPreview,
  type CodesMeta,
} from '../../../lib/promptSettings';
import { usePromptShow } from '../../../lib/promptShow';
import {
  sessionIdentityGet,
  subscribeSessionIdentity,
  type SessionIdentity,
} from '../../../ipc/characters';
import { profilesList, subscribeProfileSwitched } from '../../../ipc/profiles';
import {
  onGamePromptSeen,
  onPromptStatus,
  openPromptCard,
  promptCaptureCheck,
  promptCompile,
  promptConfigGet,
  promptConfigSet,
  promptLastSeen,
  promptStateGet,
  subscribePromptConfigChanged,
  type PromptCapture,
  type PromptCaptureCheck,
  type PromptCheckRead,
  type PromptCompileReport,
  type PromptConfig,
  type PromptFieldState,
  type PromptLastSeen,
  type PromptShow,
  type PromptState,
  type PromptShowState,
} from '../../../ipc/prompt';
import { promptDescribe, promptRender, type PromptPreviewName } from '../../../ipc/promptDesign';
import { onState } from '../../../ipc/session';
import { subscribeUiConfigReplaced } from '../../../ipc/uiConfig';
import { useTauriEvent } from '../../../ipc/useTauriEvent';
import { shownColumns, type Cell } from '../../../terminal/sgrCells';
import { warnBoxes, warnedPieces } from '../../../lib/promptWarn';
import { useGamePrompt } from '../../../stores/gmcp/gamePromptStore';
import { useBandEnv } from '../../../lib/useBandEnv';
import { CARD_ROW_PX, useCellWidth } from '../../../lib/useCellWidth';
import { knownWorld } from '../../../stores/session/useConnection';
import { ConfirmDialog } from '../../ConfirmDialog';
import { MenuItem, MenuSeparator, MenuSurface, type MenuPlacement } from '../../panel/MenuSurface';
import { CellLine } from '../../prompt/PromptCells';
import { CommandBox } from '../../prompt/PromptCodes';
import { nativeSurfaceEnabled } from '../../../terminal/Terminal';
import { PromptShowField } from '../rows/PromptShowRow';
import { Button, Field, IconButton, MoreIcon, Row, Section, Segmented, Toggle } from '../ui';

// Settings, Input, Prompt (section 7 step 12 of the prompt build spec,
// boards P12 and P13, with the Settings specimens on P0 and P14). One
// card in three parts: your game's prompt, then Draw your own prompt with
// Customize… and where your prompt shows, then the preview. Every change
// saves to the profile's [prompt] table through the prompt commands, and
// the section reads the table again whenever anything changes it: the
// card, a command such as #prompt, the game sending your prompt setting,
// a profile switch, or a connect.

/** What the section reads to draw itself. */
interface PromptData {
  config: PromptConfig;
  show: PromptShowState | null;
  state: PromptState | null;
  seen: PromptLastSeen | null;
  identity: SessionIdentity | null;
  active: string;
  host: string;
  check: PromptCaptureCheck | null;
  refresh: () => void;
  setConfig: (next: PromptConfig) => void;
}

/** Read the table, the prompt state and who the section saves for, and
 *  read them again on every change. */
function usePromptData(): PromptData | null {
  const [config, setConfig] = useState<PromptConfig | null>(null);
  const [state, setState] = useState<PromptState | null>(null);
  const [seen, setSeen] = useState<PromptLastSeen | null>(null);
  const [identity, setIdentity] = useState<SessionIdentity | null>(null);
  const [active, setActive] = useState('default');
  const [host, setHost] = useState('');
  const [check, setCheck] = useState<PromptCaptureCheck | null>(null);
  const [tick, setTick] = useState(0);
  const show = usePromptShow();
  const refresh = useCallback(() => setTick((n) => n + 1), []);

  useEffect(() => {
    let alive = true;
    void Promise.all([
      promptConfigGet(),
      promptStateGet().catch(() => null),
      promptLastSeen().catch(() => null),
      sessionIdentityGet().catch(() => null),
      profilesList().catch(() => null),
    ])
      .then(([table, now, last, who, list]) => {
        if (!alive) return;
        const name = list?.active ?? who?.profile ?? 'default';
        const entry = list?.profiles.find((p) => p.name === name);
        setConfig(table);
        setState(now);
        setSeen(last);
        setIdentity(who);
        setActive(name);
        setHost(who?.host ?? entry?.auto_match?.host ?? '');
      })
      .catch((e: unknown) => console.error('[settings prompt] reading the table failed', e));
    return () => {
      alive = false;
    };
  }, [tick]);

  // How the capture matches your last prompts, counted again with each
  // read.
  const capture = config?.capture;
  useEffect(() => {
    if (!capture || capture.kind === 'none') {
      setCheck(null);
      return;
    }
    let alive = true;
    void promptCaptureCheck(capture)
      .then((next) => {
        if (alive) setCheck(next);
      })
      .catch(() => {
        if (alive) setCheck(null);
      });
    return () => {
      alive = false;
    };
  }, [capture, tick]);

  useTauriEvent(subscribePromptConfigChanged, refresh);
  useTauriEvent(subscribeProfileSwitched, refresh);
  useTauriEvent(subscribeSessionIdentity, refresh);
  useTauriEvent(onGamePromptSeen, refresh);
  useTauriEvent(onPromptStatus, refresh);
  useTauriEvent(onState, refresh);
  useTauriEvent(subscribeUiConfigReplaced, refresh);
  // Coming back to the window counts your newest prompts again.
  useEffect(() => {
    window.addEventListener('focus', refresh);
    return () => window.removeEventListener('focus', refresh);
  }, [refresh]);

  if (!config) return null;
  return { config, show, state, seen, identity, active, host, check, refresh, setConfig };
}

/** Save a change to the table as it stands now, so a change the card
 *  made a moment ago stays. */
async function saveTable(change: (config: PromptConfig) => PromptConfig): Promise<PromptConfig> {
  const next = change(await promptConfigGet());
  await promptConfigSet(next);
  return next;
}

interface PromptSectionProps {
  /** The terminal face, which the codes and the preview are set in. */
  fontFamily: string;
  themeTerminalColors: boolean;
  brightBold: boolean;
  onError: (message: string | null) => void;
}

export function PromptSection({
  fontFamily,
  themeTerminalColors,
  brightBold,
  onError,
}: PromptSectionProps) {
  const data = usePromptData();
  const game = useGamePrompt();
  const env = useBandEnv(
    themeTerminalColors,
    brightBold,
    nativeSurfaceEnabled() ? 'native' : 'xterm',
  );
  const cellW = useCellWidth(fontFamily);
  const [preview, setPreview] = useState<PromptPreviewName>('now');
  const [confirmForget, setConfirmForget] = useState(false);
  // Forget takes the row with More away, so focus goes on to Customize…
  // once the profile reads no prompt, and never falls to the page.
  const customizeRef = useRef<HTMLButtonElement | null>(null);
  const [refocus, setRefocus] = useState(false);
  const reading = (data?.config.capture.kind ?? 'none') !== 'none';
  useEffect(() => {
    if (!refocus || reading) return;
    customizeRef.current?.focus({ preventScroll: true });
    setRefocus(false);
  }, [refocus, reading]);

  const fail = useCallback(
    (e: unknown) => onError(e instanceof Error ? e.message : String(e)),
    [onError],
  );

  if (!data) return null;
  const { config, show, state, identity, active, host } = data;
  const capture = config.capture;
  const reads = capture.kind !== 'none';
  const forsaken =
    (state?.forsaken ?? false) || knownWorld(host) !== undefined || capture.kind === 'aabahran';
  const gameSent = (state?.new_build ?? false) || (show?.gameSent ?? false) || game !== null;
  const block = gameBlock({ forsaken, gameSent, capture });
  const world = promptWorld({ forsaken, host });
  const connected = identity !== null;
  const promptsOff = (show?.promptsOff ?? false) || state?.status.status === 'prompts_off';
  const notMatching =
    state?.status.status === 'not_matching' ? notMatchingLine(state.status.last_match_at) : null;
  const owner = savedForName(identity, active);
  const previews = previewOptions(forsaken);
  // Lament leaves with the Forsaken Lands rules, and the preview draws
  // Now until they come back.
  const shown = shownPreview(preview, forsaken);

  const save = (change: (config: PromptConfig) => PromptConfig) => {
    data.setConfig(change(config));
    onError(null);
    void saveTable(change)
      .then(data.setConfig)
      .catch((e: unknown) => {
        fail(e);
        data.refresh();
      });
  };

  const forget = () => {
    setConfirmForget(false);
    setRefocus(true);
    save((c) => ({ ...c, capture: { kind: 'none' } }));
  };

  return (
    <Section
      id="prompt"
      title="Prompt"
      actions={<span className="st-meta">{owner}</span>}
      help={{ topic: 'shape.prompt-show', subject: 'your prompt' }}
    >
      {block === 'codes' && (
        <CodesBlock
          codes={gameCodesOf(game, data.seen)}
          capture={capture}
          check={data.check}
          promptsOff={promptsOff}
          notMatching={notMatching}
          description={gameDescription(world)}
        />
      )}
      {block === 'fields' && (
        <FieldsBlock
          capture={capture}
          seen={data.seen}
          check={data.check}
          promptsOff={promptsOff}
          notMatching={notMatching}
          description={gameDescription(world)}
          onSave={(next) => save((c) => ({ ...c, capture: next }))}
        />
      )}
      {block === 'line' && (
        <LineRow
          read={data.check?.reads[0] ?? null}
          lastRead={lastReadLine(state?.status.last_match_at ?? null)}
          emptyText={data.check?.text ?? null}
          notMatching={notMatching}
          onPoint={() => void openPromptCard('point').catch(fail)}
          onForget={() => setConfirmForget(true)}
        />
      )}
      {block === 'point' && <PointRow />}
      <DrawRow
        capture={reads}
        draw={config.draw}
        description={drawDescription({ capture: reads, gameSent, world })}
        customizeRef={customizeRef}
        onCustomize={() => void openPromptCard().catch(fail)}
        onDraw={(on) => save((c) => ({ ...c, draw: on }))}
      />
      <PromptShowField
        value={config.show}
        state={show}
        onChange={(next: PromptShow) => save((c) => ({ ...c, show: next }))}
      />
      {reads && (
        <PreviewBlock
          template={config.template}
          catalog={data.state?.catalog ?? NO_FIELDS}
          live={connected}
          preview={shown}
          options={previews}
          onPreview={setPreview}
          band={config.show !== 'text'}
          env={env}
          cellW={cellW}
          meta={previewMeta(connected)}
          tick={data.check}
        />
      )}
      {confirmForget && (
        <ConfirmDialog
          title="Forget your game's prompt?"
          body={`Vosh stops reading your prompt for ${owner.replace(/^Saved for /, '')}, and the game's own prompt shows again. Your design stays saved.`}
          confirmLabel="Forget"
          onConfirm={forget}
          onCancel={() => setConfirmForget(false)}
        />
      )}
    </Section>
  );
}

// ---------------------------------------------------------------------
// Your game's prompt
// ---------------------------------------------------------------------

/** The label line of the game prompt block. */
function GameLabel({ id, description }: { id: string; description: string }) {
  return (
    <div className="st-row-text">
      <span id={id} className="st-row-label">
        Your game&apos;s prompt
      </span>
      <span className="st-row-desc">{description}</span>
    </div>
  );
}

/** The meta under the codes: a sentence, or a warning with the command
 *  that fixes it, each with Copy. */
export function CodesMetaLine({ meta }: { meta: CodesMeta }) {
  if (meta.text.length === 0) return null;
  return (
    <>
      {meta.tone === 'warn' ? (
        <p className="st-prompt-meta is-warn" role="status">
          <span className="st-prompt-dot" aria-hidden="true" />
          <span>{meta.text}</span>
        </p>
      ) : (
        <p className="st-prompt-meta">{meta.text}</p>
      )}
      {meta.fixes.map((command) => (
        <CommandBox key={command} command={command} className="st-prompt-command" />
      ))}
    </>
  );
}

interface CodesRowsProps {
  labelId: string;
  children: [ReactNode, ReactNode];
}

/** The Prompt and Fight prompt rows in their 88 px label column. */
function CodesRows({ labelId, children }: CodesRowsProps) {
  return (
    <dl className="st-prompt-codes" aria-labelledby={labelId}>
      <div className="st-prompt-code-row">
        <dt>Prompt</dt>
        {children[0]}
      </div>
      <div className="st-prompt-code-row">
        <dt>Fight prompt</dt>
        {children[1]}
      </div>
    </dl>
  );
}

interface CodesTextProps {
  prompt: string;
  fprompt: string;
  description: string;
  meta: CodesMeta;
}

/** The codes the game sent, as text where the fields stood (D25, P12,
 *  P13). An empty fight prompt reads None set. */
export function CodesText({ prompt, fprompt, description, meta }: CodesTextProps) {
  const labelId = useId();
  return (
    <div className="st-block st-prompt-game" data-st-anchor="prompt-game" data-st-flash="">
      <GameLabel id={labelId} description={description} />
      <CodesRows labelId={labelId}>
        <dd className="st-prompt-code">{prompt}</dd>
        {fprompt.length > 0 ? (
          <dd className="st-prompt-code">{fprompt}</dd>
        ) : (
          <dd className="st-prompt-code is-none">None set</dd>
        )}
      </CodesRows>
      <CodesMetaLine meta={meta} />
    </div>
  );
}

/** What codes compile to, null while there are none. The newest report
 *  stays while the next compiles, so the meta never blinks, and `fresh`
 *  says it is the report of the codes as they stand. */
function useCompiled(
  prompt: string,
  fprompt: string,
  typed: boolean,
): { report: PromptCompileReport | null; fresh: boolean } {
  const key = JSON.stringify([prompt, fprompt, typed]);
  const [held, setHeld] = useState<{ key: string; report: PromptCompileReport } | null>(null);
  useEffect(() => {
    if (prompt.trim().length === 0) {
      setHeld(null);
      return;
    }
    let alive = true;
    void promptCompile({ kind: 'aabahran', prompt, fprompt, typed })
      .then((report) => {
        if (alive) setHeld({ key, report });
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [key, prompt, fprompt, typed]);
  if (prompt.trim().length === 0) return { report: null, fresh: true };
  return { report: held?.report ?? null, fresh: held?.key === key };
}

interface CodesBlockProps {
  codes: ReturnType<typeof gameCodesOf>;
  capture: PromptCapture;
  check: PromptCaptureCheck | null;
  promptsOff: boolean;
  notMatching: string | null;
  description: string;
}

/** The new build: the codes the game sent this session. */
function CodesBlock({
  codes,
  capture,
  check,
  promptsOff,
  notMatching,
  description,
}: CodesBlockProps) {
  const prompt = codes?.prompt ?? (capture.kind === 'aabahran' ? capture.prompt : '');
  const fprompt = codes?.fprompt ?? (capture.kind === 'aabahran' ? capture.fprompt : '');
  const { report } = useCompiled(prompt, fprompt, false);
  const meta = codesMeta({
    block: 'codes',
    game: codes,
    seen: null,
    capture,
    check,
    report,
    promptsOff,
    notMatching,
    now: new Date(),
  });
  return <CodesText prompt={prompt} fprompt={fprompt} description={description} meta={meta} />;
}

interface FieldsBlockProps {
  capture: PromptCapture;
  seen: PromptLastSeen | null;
  check: PromptCaptureCheck | null;
  promptsOff: boolean;
  notMatching: string | null;
  description: string;
  onSave: (capture: PromptCapture) => void;
}

/** How long typing settles before the codes save, as other Settings
 *  fields do. */
const SAVE_AFTER_MS = 400;

/** Without Char.Prompt this session: your codes in fields. What you type
 *  reads again at once, and once the codes compile they save as you set
 *  them in the game, as #prompt game does. */
function FieldsBlock({
  capture,
  seen,
  check,
  promptsOff,
  notMatching,
  description,
  onSave,
}: FieldsBlockProps) {
  const labelId = useId();
  const saved = capture.kind === 'aabahran' ? capture : null;
  const [prompt, setPrompt] = useState(saved?.prompt ?? '');
  const [fprompt, setFprompt] = useState(saved?.fprompt ?? '');
  const [dirty, setDirty] = useState(false);
  // The saved codes follow a change from elsewhere while you are not
  // typing, such as the game's answer to prompt.
  useEffect(() => {
    if (dirty) return;
    setPrompt(saved?.prompt ?? '');
    setFprompt(saved?.fprompt ?? '');
  }, [saved?.prompt, saved?.fprompt, dirty]);
  const { report, fresh } = useCompiled(prompt, fprompt, dirty);
  const onSaveRef = useRef(onSave);
  onSaveRef.current = onSave;
  // Codes you typed save once they compile and typing settles.
  useEffect(() => {
    if (!dirty || !fresh || !report?.ok) return;
    const id = window.setTimeout(() => {
      setDirty(false);
      if (saved && saved.prompt === report.prompt && saved.fprompt === report.fprompt) return;
      onSaveRef.current({
        kind: 'aabahran',
        prompt: report.prompt,
        fprompt: report.fprompt,
        follow_game: true,
        seen_at: localStamp(new Date()),
        source: 'typed',
      });
    }, SAVE_AFTER_MS);
    return () => window.clearTimeout(id);
  }, [dirty, fresh, report, saved]);
  const edit = (set: (value: string) => void) => (value: string) => {
    set(value);
    setDirty(true);
  };
  const meta = codesMeta({
    block: 'fields',
    game: null,
    seen,
    capture,
    check,
    report,
    promptsOff,
    notMatching,
    now: new Date(),
  });
  return (
    <div className="st-block st-prompt-game" data-st-anchor="prompt-game" data-st-flash="">
      <GameLabel id={labelId} description={description} />
      <CodesRows labelId={labelId}>
        <dd>
          <Field
            mono
            width={512}
            value={prompt}
            placeholder="%n%P%C<%hhp %mm %vmv>"
            aria-label="Prompt"
            onChange={edit(setPrompt)}
          />
        </dd>
        <dd>
          <Field
            mono
            width={512}
            value={fprompt}
            placeholder="None set"
            aria-label="Fight prompt"
            onChange={edit(setFprompt)}
          />
        </dd>
      </CodesRows>
      <CodesMetaLine meta={meta} />
    </div>
  );
}

interface LineRowProps {
  /** The newest prompt the pattern read, with its values marked. */
  read: PromptCheckRead | null;
  lastRead: string | null;
  /** What the check says when the pattern read no prompt yet. */
  emptyText: string | null;
  /** The not matching sentence while no prompt has matched, else null. */
  notMatching?: string | null;
  onPoint: () => void;
  onForget: () => void;
}

/** The line you pointed at, for a pattern (A4's capture row): the line in
 *  the terminal face at meta size with each value Vosh reads in the
 *  selection token, then when Vosh last read it, and More with Point at
 *  it again… and Forget your game's prompt. Once no prompt has matched
 *  (P14), the not matching sentence in warn takes the place of when Vosh
 *  last read it, and Point at it again… leaves More for a button of its
 *  own before it. */
export function LineRow({
  read,
  lastRead,
  emptyText,
  notMatching = null,
  onPoint,
  onForget,
}: LineRowProps) {
  const [menu, setMenu] = useState<{ at: MenuPlacement; anchor: HTMLElement } | null>(null);
  const labelId = useId();
  const lines = read ? read.plain.split('\n') : [];
  return (
    <div className="st-row st-prompt-line-row" data-st-anchor="prompt-game" data-st-flash="">
      <div className="st-row-text">
        <span id={labelId} className="st-row-label">
          Your game&apos;s prompt
        </span>
        {lines.map((line, i) => (
          <span key={i} className="st-prompt-line">
            {markedLine(line, read?.marks.filter((m) => m.line === i) ?? [])}
          </span>
        ))}
        {notMatching ? (
          <span className="st-prompt-line-meta is-warn" role="status">
            <span className="st-prompt-dot" aria-hidden="true" />
            <span>{notMatching}</span>
          </span>
        ) : (
          <>
            {read && lastRead && <span className="st-prompt-line-meta">{lastRead}</span>}
            {!read && emptyText && <span className="st-row-desc">{emptyText}</span>}
          </>
        )}
      </div>
      <div className="st-row-control">
        {notMatching && <Button onClick={onPoint}>Point at it again…</Button>}
        <IconButton
          label="Prompt options"
          icon={<MoreIcon />}
          aria-haspopup="menu"
          aria-expanded={menu !== null}
          onClick={(e) => {
            if (menu) {
              setMenu(null);
              return;
            }
            const r = e.currentTarget.getBoundingClientRect();
            setMenu({
              at: { x: r.left, y: r.bottom + 4, flipX: r.right, flipY: r.top - 4 },
              anchor: e.currentTarget,
            });
          }}
        />
      </div>
      {menu && (
        <MenuSurface
          label="Prompt options"
          at={menu.at}
          anchor={menu.anchor}
          className="st-prompt-menu"
          onClose={(reason) => {
            const anchor = menu.anchor;
            setMenu(null);
            if (reason === 'escape') anchor.focus();
          }}
        >
          {!notMatching && (
            <>
              <MenuItem
                onSelect={() => {
                  setMenu(null);
                  onPoint();
                }}
              >
                Point at it again…
              </MenuItem>
              <MenuSeparator />
            </>
          )}
          <MenuItem
            onSelect={() => {
              // The dialog hands focus back to More when it closes.
              const anchor = menu.anchor;
              setMenu(null);
              anchor.focus();
              onForget();
            }}
          >
            <span className="st-menu-danger">Forget your game&apos;s prompt</span>
          </MenuItem>
        </MenuSurface>
      )}
    </div>
  );
}

/** A line with the characters each mark covers in the selection token. */
function markedLine(line: string, marks: readonly { start: number; end: number }[]): ReactNode[] {
  const chars = Array.from(line);
  const out: ReactNode[] = [];
  let at = 0;
  for (const mark of [...marks].sort((a, b) => a.start - b.start)) {
    if (mark.start < at) continue;
    if (mark.start > at) out.push(chars.slice(at, mark.start).join(''));
    out.push(
      <span key={mark.start} className="st-prompt-mark">
        {chars.slice(mark.start, mark.end).join('')}
      </span>,
    );
    at = mark.end;
  }
  if (at < chars.length) out.push(chars.slice(at).join(''));
  return out;
}

/** Another game before you point at its line. */
export function PointRow() {
  return (
    <Row
      label="Your game's prompt"
      description={POINT_DESCRIPTION}
      anchor="prompt-game"
      className="st-prompt-point"
    />
  );
}

// ---------------------------------------------------------------------
// Draw your own prompt
// ---------------------------------------------------------------------

interface DrawRowProps {
  /** The profile reads a prompt. Without one the switch waits (P13). */
  capture: boolean;
  draw: boolean;
  description: string;
  /** Customize…, which takes focus after Forget your game's prompt. */
  customizeRef?: Ref<HTMLButtonElement>;
  onCustomize: () => void;
  onDraw: (on: boolean) => void;
}

/** Draw your own prompt with Customize… and the switch. Customize…
 *  stays open to you without a capture, since the card reads your
 *  prompt. */
export function DrawRow({
  capture,
  draw,
  description,
  customizeRef,
  onCustomize,
  onDraw,
}: DrawRowProps) {
  return (
    <Row
      label="Draw your own prompt"
      description={description}
      className={capture ? 'st-draw-row' : 'st-draw-row is-waiting'}
    >
      <Button ref={customizeRef} onClick={onCustomize}>
        Customize…
      </Button>
      <Toggle checked={capture && draw} disabled={!capture} onChange={onDraw} />
    </Row>
  );
}

// ---------------------------------------------------------------------
// The preview
// ---------------------------------------------------------------------

/** No catalog yet, before the prompt state is read. */
const NO_FIELDS: readonly PromptFieldState[] = [];

/** The text inset of the preview output, as P12 draws it. */
const OUT_X = 10;
const OUT_W = 600;
/** The band reaches 4 px past the text each side and 2 px past each row
 *  (the 2026-09-30 addendum, SPEC 1). */
const BAND_X = 4;
const BAND_Y = 2;

interface PreviewBlockProps {
  template: string;
  /** What each value reads now, to ring a part no value fills. */
  catalog: readonly PromptFieldState[];
  /** Draw live values, or samples while you are offline. */
  live: boolean;
  preview: PromptPreviewName;
  options: { value: PromptPreviewName; label: string }[];
  onPreview: (preview: PromptPreviewName) => void;
  /** Draw the design on the band, while your prompt shows lifted or
   *  pinned. */
  band: boolean;
  env: BandEnv;
  cellW: number;
  meta: string;
  /** Changes whenever the values may have, to draw again. */
  tick: unknown;
}

/** The preview: the Segmented, then your design drawn by prompt_render
 *  on the terminal ground, 600 wide and 28 tall plus 17.5 for each line
 *  past the first, then where to change it. It never changes the prompt
 *  on screen. */
function PreviewBlock({
  template,
  catalog,
  live,
  preview,
  options,
  onPreview,
  band,
  env,
  cellW,
  meta,
  tick,
}: PreviewBlockProps) {
  const [drawn, setDrawn] = useState<{ ansi: string; rings: Box[] } | null>(null);
  useEffect(() => {
    let alive = true;
    const shown = preview === 'now' ? null : preview;
    // A part no value fills draws its label in the ring, as the card
    // draws it, so you see which part stays blank (P14). Only live
    // values leave a part blank.
    const ringed = live
      ? promptDescribe(template, shown)
          .then((d) => warnedPieces(d.pieces, d.tokens, catalog))
          .catch(() => new Set<number>())
      : Promise.resolve(new Set<number>());
    void ringed
      .then((warn) =>
        promptRender({
          template,
          values: live ? 'live' : 'sample',
          preview: shown,
          placeholders: warn.size > 0,
        }).then((rendered) => ({
          ansi: rendered.ansi,
          rings: warnBoxes(rendered.spans, warn, {
            x: OUT_X,
            y: (28 - CARD_ROW_PX) / 2,
            cellW,
            rowH: CARD_ROW_PX,
          }),
        })),
      )
      .then((next) => {
        if (alive) setDrawn(next);
      })
      .catch(() => {
        if (alive) setDrawn(null);
      });
    return () => {
      alive = false;
    };
  }, [template, catalog, live, preview, cellW, tick]);
  const rows = useMemo(() => previewRows(drawn?.ansi ?? null), [drawn]);
  return (
    <PreviewView
      rows={rows}
      rings={drawn?.rings ?? []}
      preview={preview}
      options={options}
      onPreview={onPreview}
      band={band}
      env={env}
      cellW={cellW}
      meta={meta}
    />
  );
}

/** A ring's place in the preview output, in px. */
interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

interface PreviewViewProps {
  rows: Cell[][];
  /** The rings round parts no value fills. */
  rings?: readonly Box[];
  preview: PromptPreviewName;
  options: { value: PromptPreviewName; label: string }[];
  onPreview: (preview: PromptPreviewName) => void;
  band: boolean;
  env: BandEnv;
  cellW: number;
  meta: string;
}

/** The preview as drawn. Exported for its test. */
export function PreviewView({
  rows,
  rings = [],
  preview,
  options,
  onPreview,
  band,
  env,
  cellW,
  meta,
}: PreviewViewProps) {
  const labelId = useId();
  const height = previewHeight(rows.length);
  const top = (28 - CARD_ROW_PX) / 2;
  const limit = Math.floor((OUT_W - 2 * OUT_X) / cellW);
  const widest = Math.min(limit, Math.max(0, ...rows.map(shownColumns)));
  return (
    <div className="st-block st-prompt-preview-block" data-st-anchor="prompt-preview">
      <div className="st-prompt-preview-head">
        <span id={labelId} className="st-row-label">
          Preview
        </span>
        <Segmented
          label="Preview"
          options={options}
          value={preview}
          onChange={onPreview}
          className="st-prompt-preview-seg"
        />
      </div>
      <output
        className="st-prompt-preview"
        aria-label="Your prompt as Vosh draws it"
        style={{ height, color: env.fg }}
      >
        {band && widest > 0 && (
          <span
            className="prompt-band"
            aria-hidden="true"
            style={{
              left: OUT_X - BAND_X,
              top: top - BAND_Y,
              width: widest * cellW + 2 * BAND_X,
              height: rows.length * CARD_ROW_PX + 2 * BAND_Y,
            }}
          />
        )}
        {rings.map((ring, i) => (
          <span key={i} className="st-prompt-preview-warn" aria-hidden="true" style={ring} />
        ))}
        {rows.map((cells, i) => (
          <CellLine
            key={i}
            cells={cells}
            env={env}
            cellW={cellW}
            limit={limit}
            className="st-prompt-preview-row"
            style={{ top: top + i * CARD_ROW_PX }}
          />
        ))}
      </output>
      <p className="st-prompt-meta">{meta}</p>
    </div>
  );
}
