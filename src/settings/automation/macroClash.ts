import { appKeyOfMacro, appShortcut, type MacroKeptShortcutId } from '../../lib/appMenu';
import { shortcutLabel } from '../../lib/shortcuts';

// The note over a macro's Key when the key is also one of the session
// keys, the sessions toggle's key or one of the Settings keys. The
// macro keeps the key in every session on its profile, so the note says
// what the key does elsewhere.

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

const DOES: Record<MacroKeptShortcutId, string> = {
  'session-new': 'opens a new session',
  'session-close': 'closes the session in front',
  'close-window': 'closes the window',
  'session-next': 'goes to the next session',
  'session-previous': 'goes to the previous session',
  'sessions-sidebar': 'shows or hides your sessions',
  'settings-timers': 'opens Timers in Settings',
  'settings-aliases': 'opens Aliases in Settings',
  'settings-triggers': 'opens Triggers in Settings',
  'settings-macros': 'opens Macros in Settings',
};

/** The note for a macro on `canonical`, or null when no session or
 *  Settings key shares it. */
export function macroClashNote(canonical: string, mac: boolean): string | null {
  const hit = appKeyOfMacro(canonical, mac);
  if (!hit) return null;
  const does =
    hit.kind === 'goto'
      ? `${shortcutLabel(`Mod+${hit.place}`, mac)} also goes to your ${ORDINALS[hit.place - 1]} session.`
      : `${shortcutLabel(appShortcut(hit.id, mac), mac)} also ${DOES[hit.id]}.`;
  return `${does} In sessions on this profile it runs this macro.`;
}
