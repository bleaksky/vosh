import { useEffect, useRef } from 'react';
import type { PromptShowState } from '../../ipc/prompt';
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
// screen arrives, so nothing covers the name prompt (board 1), and the
// prompt card folds it as it opens, so the two never show together.
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

  if (view.shows !== 'open' || covered) return null;
  return <GetStartedCard view={view} facts={facts} play={play} {...card} />;
}
