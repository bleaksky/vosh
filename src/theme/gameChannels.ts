// The channels Aabahran sends over Comm.Channel, and the ANSI slot the
// game prints each one in (comm.c color codes, act_comm.c and
// languages.c). The game sends bold in the bright slot, so a bold code
// maps to its bright slot. The chat pane colors its lines by this table
// (chatColors), and a color vision swap keeps every two of these colors
// apart (gameFit CHANNEL_PAIRS), so the two cannot drift. The module
// imports nothing but a type, so the fit worker can read it.

import type { AnsiSlot } from './baseAnsi';

export const GAME_CHANNEL_SLOTS: ReadonlyMap<string, AnsiSlot> = new Map<string, AnsiSlot>([
  ['say', 'brightYellow'], // `# bold yellow
  ['tell', 'green'], // `2 green
  ['gtell', 'brightMagenta'], // `9 bold magenta
  ['yell', 'cyan'], // `6 cyan
  ['pray', 'brightWhite'], // `& bold white
  ['cabal', 'brightBlue'], // `0 bold blue
  ['clan', 'brightCyan'], // `^ bold cyan
  ['faction', 'yellow'], // `3 yellow
  ['newbie', 'brightGreen'], // `@ bold green
  ['immortal', 'brightRed'], // `! bold red
  ['imp', 'brightCyan'], // the message in `^ bold cyan
]);
