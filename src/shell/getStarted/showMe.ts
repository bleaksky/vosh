import { openSettingsTab } from '../../lib/settingsLink';
import { PANE_LABELS } from '../../panel/paneTypes';
import { profileInFront } from '../../stores/session/sessionsStore';
import { menuRows, showCoach } from '../../ui/coach';
import { fold } from './getStartedStore';
import type { StepId } from './steps';

// Show me on a step of Get started folds the card, opens what the step
// is about as a click would, and rings what to pick.
// Tracked affects ring in the Settings window, which reads the ring off
// the Add affect anchor.

/** What Show me opens in the main window. */
export interface ShowMeShell {
  /** Turn arrange mode on, whose shelf holds the panes to add. */
  arrange: () => void;
  /** Open the terminal menu at the terminal's last row. */
  openTerminalMenu: () => void;
}

export function showMe(step: StepId, shell: ShowMeShell): void {
  fold();
  if (step === 'panes') {
    shell.arrange();
    showCoach({
      find: () =>
        Array.from(
          document.querySelectorAll<HTMLElement>(
            '[data-shelf-pane="chat"], [data-shelf-pane="group"]',
          ),
        ),
      line: `Drag ${PANE_LABELS.chat} or ${PANE_LABELS.group} onto the panel, then press Done.`,
    });
  } else if (step === 'affects') {
    const profile = profileInFront();
    openSettingsTab(profile ? `characters:${profile}#add-affect` : 'characters#add-affect');
  } else if (step === 'prompt') {
    shell.openTerminalMenu();
    showCoach({
      find: () => menuRows('Terminal', ['Customize prompt…']),
      line: 'Pick Customize prompt…',
    });
  }
}
