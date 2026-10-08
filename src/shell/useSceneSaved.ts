import { revealScene, subscribeSceneSaved } from '../ipc/logs';
import { useTauriEvent } from '../ipc/useTauriEvent';
import { revealLabel } from '../lib/revealLabel';
import { pushToast } from '../stores/toasts';

/** The toast a saved scene raises in the main window: it names the
 *  file, and its button shows it in Finder or Explorer, or opens the
 *  folder on Linux, through the command Scripts uses. */
export function sceneSavedToast(name: string, platform: string | undefined): void {
  pushToast({
    kind: 'success',
    message: 'Saved the scene',
    meta: name,
    action: {
      label: revealLabel(platform),
      run: () => {
        revealScene(name).catch((e: unknown) => pushToast({ kind: 'error', message: String(e) }));
      },
    },
  });
}

/** Raise the toast for each scene Settings saves. */
export function useSceneSaved(): void {
  useTauriEvent(subscribeSceneSaved, (name) =>
    sceneSavedToast(name, document.documentElement.dataset.platform),
  );
}
