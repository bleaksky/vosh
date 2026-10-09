// The prompt card's first effect, which opens the card for a request
// and opens it again for another profile. The card calls it before every
// other effect of its own, so they run in the order they did.

import { useEffect, useRef, type Dispatch, type SetStateAction } from 'react';
import { sessionIdentityGet, type SessionIdentity } from '../ipc/characters';
import { profilesList } from '../ipc/profiles';
import {
  onPromptState,
  onPromptStatus,
  promptStateGet,
  promptWatch,
  type PromptConfig,
  type PromptState,
} from '../ipc/prompt';
import { promptPreviewSet } from '../ipc/promptDesign';
import { knownWorld } from '../lib/knownWorlds';
import { getSessions } from '../stores/session/sessionsStore';
import type { CardBinding } from './cardBinding';
import { openingStep, type CardRequest, type CardStep } from './cardRules';
import type { CardView } from './PromptCard';
import { followCardProfile } from './promptCardSync';
import { NOWHERE, type Pointing } from './promptPieces';

type Setter<T> = Dispatch<SetStateAction<T>>;

/** What the card hands its open steps: who it opens for, the edits it
 *  forgets and the steps it resets, and the card state it sets. */
interface CardOpen {
  session: number;
  binding: CardBinding;
  vitals: boolean;
  opening: CardRequest;
  opens: { current: number };
  take: (config: PromptConfig) => void;
  forgetEdits: () => void;
  resetSteps: () => void;
  setStep: Setter<CardStep | null>;
  setPointing: Setter<Pointing>;
  setView: Setter<CardView>;
  setMoreAt: Setter<HTMLElement | null>;
  setConfirmForget: Setter<boolean>;
  setState: Setter<PromptState | null>;
  setIdentity: Setter<SessionIdentity | null>;
  setActive: Setter<string>;
  setKnownHost: Setter<boolean>;
  setRefresh: Setter<number>;
}

/** Open the card, and open it again from its first step each time
 *  another profile becomes active. */
export function useCardOpen({
  session,
  binding,
  vitals,
  opening,
  opens,
  take,
  forgetEdits,
  resetSteps,
  setStep,
  setPointing,
  setView,
  setMoreAt,
  setConfirmForget,
  setState,
  setIdentity,
  setActive,
  setKnownHost,
  setRefresh,
}: CardOpen) {
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
    // The setters are the card's own state setters, which keep one
    // identity, so the effect still runs only as the card mounts.
  }, [
    session,
    binding,
    vitals,
    forgetEdits,
    opens,
    resetSteps,
    take,
    setStep,
    setPointing,
    setView,
    setMoreAt,
    setConfirmForget,
    setState,
    setIdentity,
    setActive,
    setKnownHost,
    setRefresh,
  ]);
}
