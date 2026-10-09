import { useEffect, useRef, useState } from 'react';
import type { PromptShowState } from '../../ipc/prompt';
import { systemReducesMotion } from '../../lib/blink';
import type { PromptCardHost } from '../../prompt/PromptCard';
import type { CellSize } from '../../prompt/pinnedDock';
import { GetStartedCard, type GetStartedPlay } from './GetStartedCard';
import { fold, mountGetStarted, useGetStarted } from './getStartedStore';
import type { StepId } from './steps';
import { useGetStartedFacts } from './useGetStartedFacts';

interface Props {
  play: GetStartedPlay;
  /** The prompt card is open, which shares the card's place. */
  covered: boolean;
  host: PromptCardHost;
  cell: CellSize | null;
  show: PromptShowState | null;
  onShowMe: (step: StepId) => void;
  focusInput: () => void;
}

// Get started in the main window. It reads where the install stands
// as the window mounts and follows the facts that finish steps while
// the card is shut too. Connect folds the card before the game's first
// screen arrives, so nothing covers the name prompt, and the
// prompt card folds it as it opens, so the two never show together.
// A fold slides the card away, out of reach while it goes. The
// prompt card's fold and reduced motion take it at once.
export function GetStarted({ play, covered, ...card }: Props) {
  useEffect(() => mountGetStarted(), []);
  const view = useGetStarted();
  const facts = useGetStartedFacts();

  const wasLive = useRef(play.live);
  useEffect(() => {
    if (play.live && !wasLive.current) fold();
    wasLive.current = play.live;
  }, [play.live]);
  useEffect(() => {
    if (covered) fold();
  }, [covered]);

  const [was, setWas] = useState(view.shows);
  const [folding, setFolding] = useState(false);
  if (view.shows !== was) {
    setWas(view.shows);
    setFolding(was === 'open' && view.shows === 'folded' && !covered && !systemReducesMotion());
  }
  if (covered && folding) setFolding(false);

  if (covered || (view.shows !== 'open' && !folding)) return null;
  return (
    <GetStartedCard
      view={view}
      facts={facts}
      play={play}
      folding={folding}
      onFolded={() => setFolding(false)}
      {...card}
    />
  );
}
