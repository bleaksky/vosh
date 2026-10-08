import { useCallback, useEffect, useMemo, useState, type RefObject } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { promptCodeReaderSet, subscribePromptCardOpen } from '../ipc/prompt';
import { promptPreviewSet } from '../ipc/promptDesign';
import { useTauriEvent } from '../ipc/useTauriEvent';
import type { WritingKind } from '../ipc/writing';
import { WRITING_PANE } from '../panel/paneLayout';
import { PROMPT_BINDING, VITALS_TEXT_BINDING, type CardBinding } from '../prompt/cardBinding';
import type { PromptCardHost } from '../prompt/PromptCard';
import { nextCardRequest, type CardRequest, type CardRequestView } from '../prompt/cardRules';
import { useWritingCardPrefs } from '../stores/config/writingCardStore';
import { useCharStatus } from '../stores/gmcp/charStatusStore';
import type { TerminalHandle } from '../terminal/terminalHandle';
import { keepWritingPane } from '../writing/pinnedPane';
import type { WritingRequest } from '../writing/WritingCard';
import { writable } from '../writing/kinds';

interface CardRequestsArgs {
  termRef: RefObject<TerminalHandle | null>;
  terminalAreaRef: RefObject<HTMLDivElement | null>;
  focusInput: () => void;
  shownPanes: string[];
  panelLoaded: boolean;
}

// The prompt card and the writing card the main window opens over your
// prompt, what asks for each, and the Writing pane the pinned card keeps.
export function useCardRequests({
  termRef,
  terminalAreaRef,
  focusInput,
  shownPanes,
  panelLoaded,
}: CardRequestsArgs) {
  // The prompt card (Customize prompt…), open over your prompt, and the
  // view it opens on, or `point` to open on pointing at your game's line.
  const [promptCard, setPromptCard] = useState<CardRequest | null>(null);
  // What the card edits: your prompt, or your vitals text.
  const [cardBinding, setCardBinding] = useState<CardBinding>(PROMPT_BINDING);
  // The writing card, open over the terminal on a kind of text, or on
  // the offer the game's editor brought. It and the prompt card share the
  // place over your prompt, so one opening closes the other.
  const [writingCard, setWritingCard] = useState<WritingRequest | null>(null);
  // Every request counts, so the open card hears a repeat of one.
  const openPromptCard = useCallback(
    (view: CardRequestView, binding: CardBinding = PROMPT_BINDING) => {
      setWritingCard(null);
      setCardBinding(binding);
      setPromptCard((prev) => nextCardRequest(prev, view));
    },
    [],
  );
  const openWriting = useCallback((kind: WritingKind, offer?: number) => {
    setPromptCard(null);
    setWritingCard((prev) => ({
      kind,
      n: (prev?.n ?? 0) + 1,
      ...(offer !== undefined ? { offer } : {}),
    }));
  }, []);
  // Your race and level, which decide the boards you write on and a
  // werebeast's beast.
  const charStatus = useCharStatus();
  const writeKinds = writable(charStatus.level);
  // The card draws your design over the band of Lifted in the text.
  const [cardBand, setCardBand] = useState(false);

  // A preview the prompt card left on before this window loaded again
  // would go on drawing on your prompt, so the window clears it as it
  // mounts.
  useEffect(() => {
    promptPreviewSet(null).catch((e: unknown) =>
      console.error('[main] clearing the prompt preview failed', e),
    );
    // And the code reader the card chose on another host.
    promptCodeReaderSet(false).catch((e: unknown) =>
      console.error('[main] clearing the code reader failed', e),
    );
  }, []);

  // The prompt card reaches the terminal it sits over through these. The
  // refs never change, so the host never does.
  const promptCardHost = useMemo<PromptCardHost>(
    () => ({
      terminal: () => termRef.current,
      area: () => terminalAreaRef.current,
      dock: () => document.querySelector<HTMLElement>('.prompt-dock'),
    }),
    [termRef, terminalAreaRef],
  );
  const closePromptCard = () => {
    setPromptCard(null);
    setCardBand(false);
    focusInput();
  };
  const closeWriting = () => {
    setWritingCard(null);
    focusInput();
  };
  // The Writing pane shows while the card is open and pinned, so closing
  // or unpinning the card takes it out, and a pane a saved tree kept with
  // no card open goes too.
  const writingPrefs = useWritingCardPrefs();
  const wantsWritingPane = writingCard !== null && writingPrefs.pinned;
  const hasWritingPane = shownPanes.includes(WRITING_PANE);
  useEffect(() => {
    if (panelLoaded && writingPrefs.loaded && wantsWritingPane !== hasWritingPane) {
      keepWritingPane(wantsWritingPane);
    }
  }, [panelLoaded, writingPrefs.loaded, wantsWritingPane, hasWritingPane]);

  // Customize… in Settings, and anything else in another window, opens
  // the card here and brings this window forward. Edit… under Customize
  // vitals and Edit your text… on the vitals menu open it on your
  // vitals text.
  useTauriEvent(subscribePromptCardOpen, (request) => {
    openPromptCard(request.view ?? 'design', request.vitals ? VITALS_TEXT_BINDING : PROMPT_BINDING);
    void getCurrentWindow()
      .setFocus()
      .catch(() => {});
  });

  return {
    promptCard,
    cardBinding,
    writingCard,
    openPromptCard,
    openWriting,
    charStatus,
    writeKinds,
    cardBand,
    setCardBand,
    promptCardHost,
    closePromptCard,
    closeWriting,
  };
}
