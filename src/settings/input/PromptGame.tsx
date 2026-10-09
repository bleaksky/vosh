import { useEffect, useId, useRef, useState, type ReactNode } from 'react';
import { localStamp } from '../../prompt/cardRules';
import {
  codesMeta,
  gameCodesOf,
  POINT_DESCRIPTION,
  type CodesMeta,
} from '../../prompt/promptSettings';
import {
  promptCompile,
  type PromptCapture,
  type PromptCaptureCheck,
  type PromptCheckRead,
  type PromptCompileReport,
  type PromptLastSeen,
} from '../../ipc/prompt';
import { MenuItem, MenuSeparator, MenuSurface, type MenuPlacement } from '../../ui/MenuSurface';
import { menuBelow } from '../../ui/menuPlacement';
import { CommandBox } from '../../prompt/PromptCodes';
import { Button, Field, IconButton, MoreIcon, Row } from '../../ui';
import { useShown } from '../shownProfile';

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
          <span className="st-warn-dot dot is-warn" aria-hidden="true" />
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

/** The codes the game sent, as text where the fields stood. An empty
 *  fight prompt reads None set. */
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
  // The session the Settings header names, whose engine compiles.
  const session = useShown().session ?? undefined;
  useEffect(() => {
    if (prompt.trim().length === 0) {
      setHeld(null);
      return;
    }
    let alive = true;
    void promptCompile({ kind: 'aabahran', prompt, fprompt, typed }, session)
      .then((report) => {
        if (alive) setHeld({ key, report });
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [key, prompt, fprompt, typed, session]);
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
export function CodesBlock({
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
export function FieldsBlock({
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

/** The line you pointed at, for a pattern: the line in the terminal face
 *  at meta size with each value Vosh reads in the selection token, then
 *  when Vosh last read it, and More with Point at it again… and Forget
 *  your game's prompt. Once no prompt has matched, the not matching
 *  sentence in warn takes the place of when Vosh last read it, and Point
 *  at it again… leaves More for a button of its own before it. */
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
            <span className="st-warn-dot dot is-warn" aria-hidden="true" />
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
            setMenu({
              at: menuBelow(e.currentTarget.getBoundingClientRect()),
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
