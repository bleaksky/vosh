import { APP_SHORTCUTS, sessionKeyOfMacro, type SessionShortcutId } from '../../lib/appMenu';
import { shortcutLabel } from '../../lib/shortcuts';

// The note over a macro's Key when the key is also one of the session
// keys (Sessions Q11). The macro keeps the key in every session on its
// profile, so the note says what the key does elsewhere.

const ORDINALS = [
  'first',
  'second',
  'third',
  'fourth',
  'fifth',
  'sixth',
  'seventh',
  'eighth',
  'ninth',
];

const DOES: Record<SessionShortcutId, string> = {
  'session-new': 'opens a new session',
  'session-close': 'closes the session in front',
  'close-window': 'closes the window',
  'session-next': 'goes to the next session',
  'session-previous': 'goes to the previous session',
};

/** The note for a macro on `canonical`, or null when no session key
 *  shares it. */
export function macroClashNote(canonical: string, mac: boolean): string | null {
  const hit = sessionKeyOfMacro(canonical, mac);
  if (!hit) return null;
  const does =
    hit.kind === 'goto'
      ? `${shortcutLabel(`Mod+${hit.place}`, mac)} also goes to your ${ORDINALS[hit.place - 1]} session.`
      : `${shortcutLabel(APP_SHORTCUTS[hit.id], mac)} also ${DOES[hit.id]}.`;
  return `${does} In sessions on this profile it runs this macro.`;
}
