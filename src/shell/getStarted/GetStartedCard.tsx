import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';
import { enabledPresetIds } from '../../automation/automationRecords';
import { PRESETS, type Preset } from '../../automation/presets';
import type { PromptShowState } from '../../ipc/prompt';
import { useEscape } from '../../lib/escapeStack';
import { APP_SHORTCUTS } from '../../lib/appMenu';
import { shortcutKeys } from '../../lib/shortcuts';
import type { PromptCardHost } from '../../prompt/PromptCard';
import type { CellSize } from '../../prompt/pinnedDock';
import { useCardPlace } from '../../prompt/useCardPlace';
import {
  Button,
  CheckIcon,
  ChevronRightIcon,
  CloseIcon,
  IconButton,
  Keycap,
  RingIcon,
  Toggle,
  cx,
} from '../../ui';
import { openNewSession } from '../newSession';
import { end, fold, showPage, type GetStartedView } from './getStartedStore';
import {
  doneCount,
  onForsakenLands,
  progress,
  stepMeta,
  stepsFor,
  suggestedPresets,
  suggestionsOn,
  type GetStartedFacts,
  type Step,
  type StepId,
} from './steps';
import { OpenPresets, PresetsStep } from './PresetsStep';
import { switchPresets } from './switchPresets';
import { TellSample } from './TellSample';

// The Get started card of First Run boards 1, 2 and 5, on the prompt
// card's recipe where the prompt card sits (Q2). It is a region with no
// scrim and no focus trap, so the name you type at the login prompt
// still goes to the command line (Q14). Inside, the arrows move through
// the steps, Enter opens one, Space flips a switch, and Esc folds the
// card to its notice and hands the caret back.

/** What the card reads of the selected session's link. */
export interface GetStartedPlay {
  /** Connecting or connected. */
  live: boolean;
  /** The character the game named, once you log in. */
  character: string | null;
  /** Dial the saved world, as Connect and Cmd+R do. */
  connect: () => void;
}

interface Props {
  view: GetStartedView;
  facts: GetStartedFacts;
  play: GetStartedPlay;
  /** The terminal the card sits over, as the prompt card reaches it. */
  host: PromptCardHost;
  cell: CellSize | null;
  show: PromptShowState | null;
  /** Fold the card and open what a step is about (Q3). */
  onShowMe: (step: StepId) => void;
  focusInput: () => void;
  /** The card slides away after a fold, out of reach. */
  folding: boolean;
  /** The slide ended. */
  onFolded: () => void;
}

/** Tells you send, which the Chat step suggests (Q5). */
const SENT_TELLS = PRESETS.find((p) => p.id === 'sent_tells') as Preset;

/** The steps a Show me opens something for. Connect dials, and the
 *  presets step holds its own switches. */
const SHOWS: readonly StepId[] = ['panes', 'affects', 'prompt'];

/** What the hint under the header says. */
function hint(host: string, all: boolean): string {
  if (all) return 'Every step is done. Settings changes any of it.';
  if (onForsakenLands(host)) {
    return 'Vosh starts plain, with every preset off. Each step shows one thing and what it does. Play when you like, and the list waits.';
  }
  return `Vosh's presets and its Affects, Chat and Group panes are made for The Forsaken Lands, so on ${host.trim()} this list keeps to two steps.`;
}

export function GetStartedCard({
  view,
  facts,
  play,
  host,
  cell,
  show,
  onShowMe,
  focusInput,
  folding,
  onFolded,
}: Props) {
  // The prompt card keeps the row over your prompt in view. Get started
  // points at no prompt, so it sits one row lower, its foot one row and
  // 4 px above the command line (board 1).
  const anchor = useCardPlace(host, cell, show, false);
  const row = cell?.height ?? 17.5;
  const world = view.target.host;
  const steps = stepsFor(view.target);
  const done = view.saved?.done ?? [];
  const page = steps.find((step) => step.id === view.page) ?? null;

  // A reader hears the card once it is up, and where to find it again.
  const [said, setSaid] = useState('');
  useEffect(() => {
    setSaid(
      'Get started is open above the command line. Find it again with Get started in the palette.',
    );
  }, []);

  // React 18 has no inert prop, so the ref sets it.
  const section = useRef<HTMLElement>(null);
  useEffect(() => {
    if (section.current) section.current.inert = folding;
  }, [folding]);

  useEscape(!folding, () => {
    fold();
    focusInput();
  });

  const close = () => {
    end();
    focusInput();
  };
  // A step after the login waits for the game to name you, and the
  // prompt on another game for the link (Q6).
  const ready = (step: Step) => {
    if (step.afterLogin) return play.character !== null;
    return step.id !== 'prompt' || play.live;
  };
  const showMe = (step: Step) => (
    <Button
      variant={steps.indexOf(step) === steps.length - 1 ? 'primary' : 'secondary'}
      disabled={!ready(step)}
      onClick={() => onShowMe(step.id)}
    >
      Show me
    </Button>
  );
  const connectButtons = (
    <>
      <Button onClick={() => void openNewSession()}>New session…</Button>
      <span className="pc-spacer" />
      <Button variant="primary" onClick={play.connect}>
        Connect
      </Button>
    </>
  );

  return (
    <section
      ref={section}
      className={cx('pc-card st-controls', folding && 'is-folding')}
      role="region"
      aria-label="Get started"
      aria-hidden={folding || undefined}
      onAnimationEnd={(event) => {
        if (folding && event.target === event.currentTarget) onFolded();
      }}
      style={{
        left: anchor && 'left' in anchor ? anchor.left : 12,
        bottom: anchor ? anchor.bottom - row : 0,
        maxHeight: anchor ? anchor.maxHeight + row : undefined,
        visibility: anchor ? 'visible' : 'hidden',
      }}
    >
      {page ? (
        <StepPage
          step={page}
          next={steps[steps.indexOf(page) + 1] ?? null}
          world={world}
          facts={facts}
          play={play}
          ready={ready(page)}
          showMe={SHOWS.includes(page.id) ? showMe(page) : null}
          connectButtons={connectButtons}
          onClose={close}
        />
      ) : (
        <StepList
          steps={steps}
          done={done}
          world={world}
          facts={facts}
          play={play}
          showMe={showMe}
          connectButtons={connectButtons}
          onClose={close}
        />
      )}
      <p className="visually-hidden" aria-live="polite">
        {said}
      </p>
    </section>
  );
}

function Header({
  title,
  meta,
  back,
  onClose,
}: {
  title: string;
  meta: string | null;
  back: boolean;
  onClose: () => void;
}) {
  return (
    <>
      <div className="pc-head">
        {back && <Button onClick={() => showPage(null)}>Back</Button>}
        <h2 className={cx('pc-title', back && 'is-picker')}>{title}</h2>
        {meta && <span className="pc-saved">{meta}</span>}
        <span className="pc-spacer" />
        <IconButton label="Close" icon={<CloseIcon />} onClick={onClose} />
      </div>
      <div className="pc-rule" aria-hidden="true" />
    </>
  );
}

function Foot({ children }: { children: ReactNode }) {
  return (
    <>
      <div className="pc-rule" aria-hidden="true" />
      <div className="pc-foot">{children}</div>
    </>
  );
}

/** The list of board 1, and with every step done the summary of board 5,
 *  each meta read live. */
function StepList({
  steps,
  done,
  world,
  facts,
  play,
  showMe,
  connectButtons,
  onClose,
}: {
  steps: Step[];
  done: readonly string[];
  world: string;
  facts: GetStartedFacts;
  play: GetStartedPlay;
  showMe: (step: Step) => ReactNode;
  connectButtons: ReactNode;
  onClose: () => void;
}) {
  const all = doneCount(steps, done) === steps.length;
  const first = steps.find((step) => !done.includes(step.id)) ?? steps[0];
  const [current, setCurrent] = useState<StepId>(first.id);
  const step = steps.find((s) => s.id === current) ?? first;
  const rows = useRef(new Map<StepId, HTMLButtonElement>());

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
    event.preventDefault();
    const at = steps.indexOf(step) + (event.key === 'ArrowDown' ? 1 : -1);
    const next = steps[Math.min(steps.length - 1, Math.max(0, at))];
    setCurrent(next.id);
    rows.current.get(next.id)?.focus();
  };

  const row = (s: Step) => {
    const isDone = done.includes(s.id);
    const dials = s.id === 'connect' && !isDone;
    let meta = isDone ? stepMeta(s.id, world, facts) : null;
    if (!isDone && s.id === 'presets') meta = `${suggestedPresets(world).length} suggested`;
    return (
      <li key={s.id}>
        <button
          type="button"
          ref={(el) => {
            if (el) rows.current.set(s.id, el);
            else rows.current.delete(s.id);
          }}
          className={cx('gs-step', isDone && 'is-done', !all && s.id === current && 'is-current')}
          onFocus={() => setCurrent(s.id)}
          onClick={() => (dials ? play.connect() : showPage(s.id))}
        >
          {isDone ? <CheckIcon className="gs-mark" /> : <RingIcon className="gs-mark" />}
          <span className="gs-name">{s.title}</span>
          {meta && <span className="gs-meta">{meta}</span>}
          {dials ? (
            <span className="gs-keys">
              {shortcutKeys(APP_SHORTCUTS.connect).map((key) => (
                <Keycap key={key}>{key}</Keycap>
              ))}
            </span>
          ) : (
            <ChevronRightIcon size={12} className="gs-chev" />
          )}
        </button>
      </li>
    );
  };

  const now = steps.filter((s) => all || !s.afterLogin);
  const later = all ? [] : steps.filter((s) => s.afterLogin);
  let foot: ReactNode;
  if (all) {
    foot = (
      <>
        <span className="pc-foot-note">Get started stays in Help.</span>
        <span className="pc-spacer" />
        <Button variant="primary" onClick={onClose}>
          Done
        </Button>
      </>
    );
  } else if (step.id === 'connect' && !play.live) {
    foot = connectButtons;
  } else {
    foot = (
      <>
        {!onForsakenLands(world) && <OpenPresets host={world} facts={facts} />}
        <span className="pc-spacer" />
        {SHOWS.includes(step.id) ? (
          showMe(step)
        ) : (
          <Button variant="primary" onClick={() => showPage(step.id)}>
            Open
          </Button>
        )}
      </>
    );
  }

  return (
    <>
      <Header title="Get started" meta={progress(steps, done)} back={false} onClose={onClose} />
      <div className="pc-body is-list" onKeyDown={onKeyDown}>
        <p className="pc-hint">{hint(world, all)}</p>
        <ul className="gs-steps">{now.map(row)}</ul>
        {later.length > 0 && (
          <>
            <p className="gs-caps">After you log in</p>
            <ul className="gs-steps is-after">{later.map(row)}</ul>
          </>
        )}
      </div>
      <Foot>{foot}</Foot>
    </>
  );
}

/** A step's page, board 2: Back, what the step does in a line, and
 *  Show me or what the step holds. */
function StepPage({
  step,
  next,
  world,
  facts,
  play,
  ready,
  showMe,
  connectButtons,
  onClose,
}: {
  step: Step;
  next: Step | null;
  world: string;
  facts: GetStartedFacts;
  play: GetStartedPlay;
  ready: boolean;
  showMe: ReactNode;
  connectButtons: ReactNode;
  onClose: () => void;
}) {
  const nextButton = next && (
    <Button variant="primary" onClick={() => showPage(next.id)}>
      Next step
    </Button>
  );
  let foot: ReactNode;
  if (step.id === 'connect' && !play.live) foot = connectButtons;
  else if (step.id === 'presets') {
    foot = (
      <>
        <OpenPresets host={world} facts={facts} />
        <span className="pc-spacer" />
        {nextButton}
      </>
    );
  } else {
    foot = (
      <>
        {next && showMe}
        <span className="pc-spacer" />
        {nextButton ?? showMe}
      </>
    );
  }
  const waits = onForsakenLands(world)
    ? 'Show me waits for your first room in the game.'
    : 'Show me waits until you connect.';

  return (
    <>
      <Header
        title={step.title}
        meta={step.id === 'presets' ? `${suggestionsOn(world, facts)} on` : null}
        back
        onClose={onClose}
      />
      <div className="pc-body">
        <p className="pc-question">{step.line}</p>
        {step.id === 'presets' && <PresetsStep host={world} facts={facts} />}
        {step.id === 'panes' && (
          <>
            <p className="pc-copy">
              Each new pane lands at the bottom of the panel. A pane's menu splits it, swaps it or
              closes it, and Vosh keeps the layout for each character.
            </p>
            <SentTells facts={facts} />
          </>
        )}
        {showMe !== null && !ready && <p className="pc-copy">{waits}</p>}
      </div>
      <Foot>{foot}</Foot>
    </>
  );
}

/** Tells you send, which the Chat step suggests, since it shows nothing
 *  until a Chat pane does (board 2). */
function SentTells({ facts }: { facts: GetStartedFacts }) {
  const stored = facts.enabledPresets;
  const on = stored !== null && enabledPresetIds(stored).includes(SENT_TELLS.id);
  return (
    <ul className="gs-sugs">
      <li className="gs-sug is-only">
        <div className="gs-sug-top">
          <span className="gs-sug-name">{SENT_TELLS.name}</span>
          <Toggle
            checked={on}
            aria-label={SENT_TELLS.name}
            disabled={stored === null}
            onChange={(next) => void switchPresets([{ id: SENT_TELLS.id, on: next }])}
          />
        </div>
        <p className="gs-sug-line">{SENT_TELLS.description}</p>
        <TellSample />
      </li>
    </ul>
  );
}
