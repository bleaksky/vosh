import type { ReactNode } from 'react';
import {
  AppearanceIcon,
  BoltIcon,
  BookIcon,
  LayoutIcon,
  LifebuoyIcon,
  PlugIcon,
  TerminalIcon,
  TickRingIcon,
  UserIcon,
} from '../ui';

/** Each section's icon, in the SPEC 6 stroke style. */
export const HELP_SECTION_ICONS: Record<string, () => ReactNode> = {
  'Get connected': () => <PlugIcon />,
  Play: () => <TerminalIcon />,
  Automate: () => <BoltIcon />,
  'Shape the window': () => <LayoutIcon />,
  'Tick and target': () => <TickRingIcon />,
  'Make it yours': () => <AppearanceIcon />,
  'Characters and data': () => <UserIcon />,
  'Fix it': () => <LifebuoyIcon />,
  Reference: () => <BookIcon />,
};

/** The icon of `section`, the book for one it does not know. */
export function sectionIcon(section: string): ReactNode {
  return HELP_SECTION_ICONS[section]?.() ?? <BookIcon />;
}
