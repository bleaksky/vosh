import { useCallback, useEffect, useRef, useState, type Ref } from 'react';
import { savedForName } from '../../prompt/cardRules';
import {
  drawDescription,
  gameBlock,
  gameCodesOf,
  gameDescription,
  lastReadLine,
  notMatchingLine,
  previewMeta,
  previewOptions,
  promptWorld,
  shownPreview,
} from '../../prompt/promptSettings';
import { usePromptShow } from '../../prompt/showState';
import {
  sessionIdentityGet,
  subscribeSessionIdentity,
  type SessionIdentity,
} from '../../ipc/characters';
import { profilesList, subscribeProfileSwitched } from '../../ipc/profiles';
import {
  onGamePromptSeen,
  onPromptStatus,
  openPromptCard,
  promptCaptureCheck,
  promptConfigGet,
  promptConfigSet,
  promptLastSeen,
  promptStateGet,
  subscribePromptConfigChanged,
  type PromptCaptureCheck,
  type PromptConfig,
  type PromptFieldState,
  type PromptLastSeen,
  type PromptShow,
  type PromptState,
  type PromptShowState,
} from '../../ipc/prompt';
import type { PromptPreviewName } from '../../ipc/promptDesign';
import { onState } from '../../ipc/session';
import { subscribeUiConfigReplaced } from '../../ipc/uiConfig';
import { useTauriEvent } from '../../ipc/useTauriEvent';
import { useGamePrompt } from '../../stores/gmcp/gamePromptStore';
import { useBandEnv } from '../../prompt/useBandEnv';
import { useCellWidth } from '../../lib/useCellWidth';
import { knownWorld } from '../../lib/knownWorlds';
import { errorText } from '../../lib/text';
import { ConfirmDialog } from '../../ui/ConfirmDialog';
import { nativeSurfaceEnabled } from '../../terminal/terminalRenderer';
import { PromptShowField } from './PromptShowRow';
import { CodesBlock, FieldsBlock, LineRow, PointRow } from './PromptGame';
import { PreviewBlock } from './PromptPreview';
import { useShown } from '../shownProfile';
import { Button, Row, Section, Toggle } from '../../ui';

// Settings, Input, Prompt. One card in three parts: your game's prompt,
// then Draw your own prompt with Customize… and where your prompt shows,
// then the preview. Every change saves to the profile's [prompt] table
// through the prompt commands, and the section reads the table again
// whenever anything changes it: the card, a command such as #prompt, the
// game sending your prompt setting, a profile switch, or a connect. The
// prompt commands read a session's prompt engine, so each names the
// session the Settings header names, which plays the profile Settings
// shows.

/** What the section reads to draw itself. */
interface PromptData {
  /** The session the calls name, undefined before the list names one. */
  session: number | undefined;
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
  const shown = useShown();
  const session = shown.session ?? undefined;
  const refresh = useCallback(() => setTick((n) => n + 1), []);

  useEffect(() => {
    let alive = true;
    void Promise.all([
      promptConfigGet(session),
      promptStateGet(session).catch(() => null),
      promptLastSeen(session).catch(() => null),
      sessionIdentityGet(session).catch(() => null),
      profilesList().catch(() => null),
    ])
      .then(([table, now, last, who, list]) => {
        if (!alive) return;
        const name = shown.profile ?? list?.active ?? who?.profile ?? 'default';
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
  }, [tick, session, shown.profile]);

  // How the capture matches your last prompts, counted again with each
  // read.
  const capture = config?.capture;
  useEffect(() => {
    if (!capture || capture.kind === 'none') {
      setCheck(null);
      return;
    }
    let alive = true;
    void promptCaptureCheck(capture, session)
      .then((next) => {
        if (alive) setCheck(next);
      })
      .catch(() => {
        if (alive) setCheck(null);
      });
    return () => {
      alive = false;
    };
  }, [capture, tick, session]);

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
  return { session, config, show, state, seen, identity, active, host, check, refresh, setConfig };
}

/** Save a change to the table of `session`'s profile as it stands now,
 *  so a change the card made a moment ago stays. */
async function saveTable(
  change: (config: PromptConfig) => PromptConfig,
  session: number | undefined,
): Promise<PromptConfig> {
  const next = change(await promptConfigGet(session));
  await promptConfigSet(next, { session });
  return next;
}

/** No catalog yet, before the prompt state is read. */
const NO_FIELDS: readonly PromptFieldState[] = [];

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

  const fail = useCallback((e: unknown) => onError(errorText(e)), [onError]);

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
    void saveTable(change, data.session)
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
// Draw your own prompt
// ---------------------------------------------------------------------

interface DrawRowProps {
  /** The profile reads a prompt. Without one the switch waits. */
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
