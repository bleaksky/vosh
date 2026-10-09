import type { RefObject } from 'react';
import type { InputHandle } from '../input/Input';
import { promptConfigGet, promptConfigSet } from '../ipc/prompt';
import { disconnectSession, type SessionRow } from '../ipc/session';
import { snoopClose, snoopStop, snoopWindowOpen } from '../ipc/snoop';
import { openHelpWindow, openSettingsWindow } from '../ipc/windows';
import type { WritingKind } from '../ipc/writing';
import { openSettingsTab } from '../lib/settingsLink';
import { togglePane } from '../panel/paneActions';
import { isOfferedPaneType, PANE_TYPES } from '../panel/paneLayout';
import { togglePanelOpen } from '../panel/panelLayoutStore';
import { offeredPaneTypes } from '../panel/paneTypes';
import type { CardRequestView } from '../prompt/cardRules';
import type { usePromptShow } from '../prompt/showState';
import type { useCharStatus } from '../stores/gmcp/charStatusStore';
import type { ConnectionStatus } from '../stores/session/connectionStore';
import { getSelected, goTo, sessionStep } from '../stores/session/sessionsStore';
import { getSnoops } from '../stores/session/snoopStore';
import type { Connection } from '../stores/session/useConnection';
import { pushToast } from '../stores/toasts';
import { getNativeScroll } from '../terminal/native/nativeScroll';
import { readPrompt } from '../terminal/readerVoice';
import { nativeSurfaceEnabled } from '../terminal/terminalRenderer';
import { hasBeast } from '../writing/kinds';
import { openList as openGetStarted } from './getStarted/getStartedStore';
import { openNewSession } from './newSession';
import type { PaletteDeps } from './overlays/palette';
import { requestSnoop } from './snoopKeys';
import type { Closing } from './useClosing';
import type { SessionsSidebar } from './useSessionsSidebar';

interface PaletteState {
  connected: boolean;
  status: ConnectionStatus;
  connection: Connection;
  panelOpen: boolean;
  splitOpen: boolean;
  toggleSplit: () => void;
  shownPanes: string[];
  openFind: () => void;
  renameSession: () => void;
  closing: Pick<Closing, 'closeSession'>;
  sessions: SessionRow[];
  selected: number;
  sessionsSidebar: Pick<SessionsSidebar, 'pressed' | 'toggle'>;
  inputRef: RefObject<InputHandle | null>;
  promptShow: ReturnType<typeof usePromptShow>;
  openPromptCard: (view: CardRequestView) => void;
  writeKinds: WritingKind[];
  charStatus: Pick<ReturnType<typeof useCharStatus>, 'race' | 'level'>;
  openWriting: (kind: WritingKind) => void;
  readerOn: boolean;
}

// Everything the palette can reach, built fresh at each open from the
// main window's live state so labels track it.
export function paletteDepsFor({
  connected,
  status,
  connection,
  panelOpen,
  splitOpen,
  toggleSplit,
  shownPanes,
  openFind,
  renameSession,
  closing,
  sessions,
  selected,
  sessionsSidebar,
  inputRef,
  promptShow,
  openPromptCard,
  writeKinds,
  charStatus,
  openWriting,
  readerOn,
}: PaletteState): PaletteDeps {
  // A snoop row's call to the game, which says in a toast when it fails.
  const snoopCall = (call: Promise<void>) =>
    void call.catch((e: unknown) => pushToast({ kind: 'error', message: String(e) }));

  return {
    connected,
    redialing: connection.redialing,
    host: status.kind === 'connected' || status.kind === 'connecting' ? status.host : null,
    worldName: connection.world,
    panelOpen,
    togglePanel: togglePanelOpen,
    splitOpen: splitOpen || (nativeSurfaceEnabled() && getNativeScroll().offset > 0),
    toggleSplit,
    // The staff queues row waits for Imm.Queues, like Add a pane, but a
    // pane the tree already shows stays listed so you can hide it.
    paneTypes: PANE_TYPES.filter(isOfferedPaneType).filter(
      (t) => offeredPaneTypes().includes(t) || shownPanes.includes(t),
    ),
    paneVisible: (pane) => panelOpen && shownPanes.includes(pane),
    togglePane,
    openHelp: openHelpWindow,
    openGetStarted,
    openFind,
    openSettings: openSettingsWindow,
    openSettingsTab,
    connect: () => void connection.connect(),
    newSession: () => void openNewSession(),
    renameSession,
    closeSession: () => closing.closeSession(),
    sessions: {
      rows: sessions,
      selected,
      shown: sessionsSidebar.pressed,
      goTo,
      step: (step) => goTo(sessionStep(step)),
      toggleShown: sessionsSidebar.toggle,
    },
    snoops: {
      tabs: getSnoops().tabs,
      goTo: () => requestSnoop('enter'),
      next: () => requestSnoop('next'),
      stop: (name) => snoopCall(snoopStop(getSelected(), name)),
      openWindow: () => snoopCall(snoopWindowOpen(getSelected())),
      closeEnded: () => snoopCall(snoopClose(getSelected())),
    },
    disconnect: () => void disconnectSession(getSelected()),
    insertInput: (text) => inputRef.current?.insert(text),
    promptShow: promptShow?.capture ? promptShow.show : null,
    openPromptCard: (view) => openPromptCard(view === 'text' ? 'text' : 'design'),
    writing: {
      kinds: writeKinds,
      beast: hasBeast(charStatus.race, charStatus.level),
      open: (kind) => openWriting(kind),
    },
    readPrompt: readerOn ? readPrompt : undefined,
    promptDraw: promptShow?.capture ? promptShow.draw : null,
    setPromptDraw: (on) => {
      const session = getSelected();
      void promptConfigGet(session)
        .then((config) => promptConfigSet({ ...config, draw: on }, { session }))
        .catch((e: unknown) => pushToast({ kind: 'error', message: String(e) }));
    },
  };
}
