import { useEffect } from 'react';
import { useTauriEvent } from '../ipc/useTauriEvent';
import { subscribeMigrationApplied } from '../ipc/wizard';
import { startGamePromptToasts } from '../prompt/gamePromptToast';
import { showMigrationApplied } from './launchNotices';
import { useAlertTones } from './useAlertTones';
import { useSceneSaved } from './useSceneSaved';

// What the main window says on its own: the tone of an alert, a saved
// scene, a prompt setting the game changed and the catalog wizard's
// files. `writeLive` writes a line into the selected session's terminal.
export function useWindowNotices(writeLive: (text: string) => void): void {
  // Play the tone of each alert a session rings.
  useAlertTones();

  // Say when Settings saved a scene, with a button that shows the file.
  useSceneSaved();

  useEffect(() => {
    // The game sent a new prompt setting and your capture follows it.
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void startGamePromptToasts().then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // The shared catalog wizard wrote its files. Nothing this session
  // changes saves until Vosh opens again, so say so in the terminal and
  // in a toast that stays up.
  useTauriEvent(subscribeMigrationApplied, () => {
    showMigrationApplied(writeLive);
  });
}
