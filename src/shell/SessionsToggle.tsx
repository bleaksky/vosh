import { appShortcut } from '../lib/appMenu';
import { ariaKeyshortcuts, shortcutLabel } from '../lib/shortcuts';
import { SidebarIcon } from './icons';

// The one sessions toggle, in the window's top left corner just after
// the traffic lights on macOS and 10 in elsewhere (Sessions toggle T1).
// AppShell holds it in one spot, over the sidebar's top while the
// sidebar shows and at the band's left end while it hides, so it never
// moves. It wears the panel toggle's recipe (T2): the band's 28 by 24
// icon button, the secondary tone while the sidebar shows and the
// quieter tertiary tone while it hides, and no fill. Its label and
// tooltip say what a press does, so it has no pressed state on top of
// them ("Hide sessions, pressed" reads backward). MainWindow shows it
// while two or more sessions are open.

interface Props {
  /** The sidebar shows, in its column or over the terminal. */
  pressed: boolean;
  /** Hide the sidebar or show it, and put the caret where it belongs. */
  onToggle: () => void;
}

export function SessionsToggle({ pressed, onToggle }: Props) {
  const label = pressed ? 'Hide sessions' : 'Show sessions';
  const spec = appShortcut('sessions-sidebar');
  return (
    <button
      type="button"
      className={pressed ? 'shell-icon-button' : 'shell-icon-button is-quiet'}
      aria-label={label}
      title={`${label} (${shortcutLabel(spec)})`}
      aria-keyshortcuts={ariaKeyshortcuts(spec)}
      onClick={onToggle}
    >
      <SidebarIcon />
    </button>
  );
}
