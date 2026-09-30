import { onGamePromptSeen, type GamePromptSeenPayload } from './session';
import { pushToast, type ToastInput } from './toasts';

// When the game tells Vosh a new prompt setting and your profile's
// capture takes it, Vosh says so once with the codes it now reads.

/** The toast for one report, or null when the capture took nothing. */
export function gamePromptToast(payload: GamePromptSeenPayload): ToastInput | null {
  if (!payload.applied || payload.kind === 'off') return null;
  return { kind: 'info', message: 'Vosh reads your new prompt.', meta: payload.text };
}

/** Show the toast for every setting your capture takes. Returns the
 *  function that stops listening. */
export async function startGamePromptToasts(): Promise<() => void> {
  return onGamePromptSeen((payload) => {
    const toast = gamePromptToast(payload);
    if (toast) pushToast(toast);
  });
}
