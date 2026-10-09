import type { EditorKind, WritingKind } from '../ipc/writing';

// The kinds of text the writing card takes, one row each.
// A kind sets the card's title, its fields, its
// width, its guide and how it ends. The writer in Rust holds the game's
// side of each, the commands that open, read and end it.

/** A row of a guide: a reminder alone, or a reminder and a line under
 *  it. */
export type GuideRow = string | { head: string; sub: string };

/** The guide beside the text: the help's reminders in Vosh's words, as
 *  plain rows, Vosh's own words under them, and the help it comes from,
 *  which its button asks the game for. */
export interface Guide {
  head: string;
  rows: GuideRow[];
  words: string[];
  help: string;
}

export interface KindInfo {
  kind: WritingKind;
  /** The card's title. */
  title: string;
  /** Its short name, on the command line's pill while the game's editor
   *  holds it. */
  pill: string;
  /** The row under Write in the terminal's menu. */
  menu: string;
  /** The palette's row, named for what you do. */
  palette: string;
  keywords: string;
  /** A note on one of the game's boards, which posts. */
  board: boolean;
  /** The game takes only immortal in To and stores it as Immortal. */
  toImmortal: boolean;
  /** Only a note on the note board takes a language. */
  language: boolean;
  /** The game records the room a bug or typo report posts from. */
  room: boolean;
  /** A help sets the width, so a line past it reads in danger. */
  helpWidth: boolean;
  /** The most lines its help allows, or null where no help sets one. */
  maxLines: number | null;
  /** The command that sends the text for its review, once. */
  check: 'dcheck' | 'history check' | null;
  /** The level a character needs to write it, from Char.Status. */
  level: number;
  guide: Guide;
}

const NOTE_GUIDE: Guide = {
  head: 'From help note',
  rows: [
    'Spell every name in To right, or the note never gets there',
    'To takes a name, a cabal, your class or race, immortal or all',
  ],
  words: ['Readers see your lines exactly as you break them, so Vosh keeps them to 75 columns.'],
  help: 'note',
};

const REPORT_GUIDE: Guide = {
  head: 'From help bug',
  rows: ['Post it from the room where it happened'],
  words: ['The game adds your name, your level and the room you post from.'],
  help: 'bug',
};

const DESCRIPTION_GUIDE: Guide = {
  head: 'From help description',
  rows: [
    {
      head: 'A picture frozen in time',
      sub: 'How you look, or look to be doing, with no action under way.',
    },
    { head: 'Never force the looker', sub: 'Leave what they do, feel or think to them.' },
    {
      head: 'You, not your history',
      sub: 'Your stance, face, body and gear. Not who you are or where you came from.',
    },
    { head: 'No boasting', sub: 'Not how powerful, fearful or deadly you are.' },
    { head: 'Gear you want is fine', sub: 'A crested shield or a sword you would like to carry.' },
  ],
  words: [
    'The game gives no dragons or other rewards for a description made by AI. Vosh never writes or rewrites a word, and spelling guesses come only from your Mac’s own menu.',
  ],
  help: 'description',
};

const BEAST_GUIDE: Guide = {
  head: 'From help beastdesc',
  rows: [
    {
      head: 'Optional',
      sub: 'An empty beast description shows the game’s own line for your beast.',
    },
    { head: 'The same rules', sub: 'Help beastdesc asks your beast to follow help description.' },
  ],
  words: ['There’s no dcheck for your beast description.'],
  help: 'beastdesc',
};

const HISTORY_GUIDE: Guide = {
  head: 'From help history',
  rows: [
    'History is your story up to now',
    'Personality is who you are and your quirks',
    'Purpose is your goal in life',
  ],
  words: ['When it’s ready, send it for review and an immortal will read it.'],
  help: 'history',
};

const base = {
  board: true,
  toImmortal: false,
  language: false,
  room: false,
  helpWidth: false,
  maxLines: null,
  check: null,
  level: 0,
};

export const KINDS: Record<WritingKind, KindInfo> = {
  note: {
    ...base,
    kind: 'note',
    title: 'Note',
    pill: 'Note',
    menu: 'Note…',
    palette: 'Write a note…',
    keywords: 'note board letter post',
    language: true,
    guide: NOTE_GUIDE,
  },
  journal: {
    ...base,
    kind: 'journal',
    title: 'Journal entry',
    pill: 'Journal entry',
    menu: 'Journal entry…',
    palette: 'Write a journal entry…',
    keywords: 'journal rp points diary',
    toImmortal: true,
    guide: {
      head: 'From help rp points',
      rows: ['Journal entries can earn you RP points'],
      words: ['Only the immortals read your journal.'],
      help: 'rp points',
    },
  },
  application: {
    ...base,
    kind: 'application',
    title: 'Application',
    pill: 'Application',
    menu: 'Application…',
    palette: 'Write an application…',
    keywords: 'application apply psi crusader cabal noble royal warcry qrace race',
    guide: NOTE_GUIDE,
  },
  idea: {
    ...base,
    kind: 'idea',
    title: 'Idea',
    pill: 'Idea',
    menu: 'Idea…',
    palette: 'Write an idea…',
    keywords: 'idea suggest suggestion',
    toImmortal: true,
    guide: {
      head: 'From help idea',
      rows: ['It takes everything a note does'],
      words: ['Only the immortals read ideas, so Vosh keeps what you send under Sent.'],
      help: 'idea',
    },
  },
  bug: {
    ...base,
    kind: 'bug',
    title: 'Bug report',
    pill: 'Bug report',
    menu: 'Bug report…',
    palette: 'Report a bug…',
    keywords: 'bug report broken',
    toImmortal: true,
    room: true,
    guide: REPORT_GUIDE,
  },
  typo: {
    ...base,
    kind: 'typo',
    title: 'Typo report',
    pill: 'Typo report',
    menu: 'Typo report…',
    palette: 'Report a typo…',
    keywords: 'typo report spelling mistake',
    toImmortal: true,
    room: true,
    guide: REPORT_GUIDE,
  },
  news: {
    ...base,
    kind: 'news',
    title: 'News',
    pill: 'News',
    menu: 'News…',
    palette: 'Write news…',
    keywords: 'news board immortal staff',
    level: 53,
    guide: NOTE_GUIDE,
  },
  changes: {
    ...base,
    kind: 'changes',
    title: 'Changes',
    pill: 'Changes',
    menu: 'Changes…',
    palette: 'Write changes…',
    keywords: 'changes board immortal staff',
    level: 59,
    guide: NOTE_GUIDE,
  },
  penalty: {
    ...base,
    kind: 'penalty',
    title: 'Penalty',
    pill: 'Penalty',
    menu: 'Penalty…',
    palette: 'Write a penalty…',
    keywords: 'penalty board immortal staff',
    level: 52,
    guide: NOTE_GUIDE,
  },
  description: {
    ...base,
    kind: 'description',
    board: false,
    title: 'Your description',
    pill: 'Description',
    menu: 'Your description…',
    palette: 'Edit your description…',
    keywords: 'description desc look dcheck',
    helpWidth: true,
    maxLines: 30,
    check: 'dcheck',
    guide: DESCRIPTION_GUIDE,
  },
  beast: {
    ...base,
    kind: 'beast',
    board: false,
    title: 'Your description',
    pill: 'Beast description',
    menu: 'Your beast description…',
    palette: 'Edit your beast description…',
    keywords: 'beast beastdesc werebeast description',
    helpWidth: true,
    maxLines: 30,
    level: 15,
    guide: BEAST_GUIDE,
  },
  history: {
    ...base,
    kind: 'history',
    board: false,
    title: 'Your history',
    pill: 'History',
    menu: 'Your history…',
    palette: 'Edit your history…',
    keywords: 'history personality purpose background story',
    check: 'history check',
    guide: HISTORY_GUIDE,
  },
  personality: {
    ...base,
    kind: 'personality',
    board: false,
    title: 'Your history',
    pill: 'Personality',
    menu: 'Your personality…',
    palette: 'Edit your personality…',
    keywords: 'personality history quirks',
    guide: HISTORY_GUIDE,
  },
  purpose: {
    ...base,
    kind: 'purpose',
    board: false,
    title: 'Your history',
    pill: 'Purpose',
    menu: 'Your purpose…',
    palette: 'Edit your purpose…',
    keywords: 'purpose history goal',
    guide: HISTORY_GUIDE,
  },
};

/** The card takes the text, which a tome, a cabal vote, paper and a
 *  pet's description wait for (Note Editor Q4). */
export function cardTakes(kind: EditorKind): kind is WritingKind {
  return kind in KINDS;
}

/** The pill names of the texts the card does not take yet. */
const EDITOR_ONLY_PILLS: Record<Exclude<EditorKind, WritingKind>, string> = {
  tome: 'Tome',
  vote: 'Cabal vote',
  paper: 'Paper',
  pet: 'Pet description',
};

/** The command line pill's name for a text the game's editor holds and
 *  the most lines its help allows. No help sets a limit for a text the
 *  card does not take. */
export function editorPill(kind: EditorKind): Pick<KindInfo, 'pill' | 'maxLines'> {
  return cardTakes(kind) ? KINDS[kind] : { pill: EDITOR_ONLY_PILLS[kind], maxLines: null };
}

/** The boards in the order the menus list them, the staff boards last. */
export const BOARD_KINDS: WritingKind[] = [
  'note',
  'journal',
  'application',
  'idea',
  'bug',
  'typo',
  'news',
  'changes',
  'penalty',
];

/** The texts about you, which the game saves in place and holds one of
 *  each. */
export const ABOUT_YOU: WritingKind[] = ['description', 'history'];

/** The switch a text about you shares its card through: Description and
 *  Beast, or History, Personality and Purpose. */
export function switchOf(kind: WritingKind): WritingKind[] | null {
  if (kind === 'description' || kind === 'beast') return ['description', 'beast'];
  if (kind === 'history' || kind === 'personality' || kind === 'purpose')
    return ['history', 'personality', 'purpose'];
  return null;
}

/** The switch's label for a kind. */
export const SWITCH_LABELS: Partial<Record<WritingKind, string>> = {
  description: 'Description',
  beast: 'Beast',
  history: 'History',
  personality: 'Personality',
  purpose: 'Purpose',
};

/** A werebeast of level 15 and up has a beast to describe
 *  (act_info.c:7645). */
export function hasBeast(
  race: string | null | undefined,
  level: number | null | undefined,
): boolean {
  return (race ?? '').toLowerCase() === 'werebeast' && (level ?? 0) >= 15;
}

/** The kinds a character can write: every board but the staff boards,
 *  which wait for the level each needs (recycle.c:4233, interp.c:545),
 *  and the texts about you. */
export function writable(level: number | null | undefined): WritingKind[] {
  const at = level ?? 0;
  return BOARD_KINDS.filter((kind) => at >= KINDS[kind].level);
}

/** From trust 55 the game keeps a code anywhere in a line
 *  (comm.c:1499). */
export function keepsCodes(level: number | null | undefined): boolean {
  return (level ?? 0) >= 55;
}

/** The width the card keeps for a kind. A custom race application keeps
 *  to the 70 help qrace asks for (help.txt:18877). */
export function widthOf(customRace: boolean): number {
  return customRace ? 70 : 75;
}

/** What a looker sees of each beast with no beast description, the
 *  game's own lines (act_info.c:1077 to 1121). */
export const BEAST_LOOKS: Record<string, string> = {
  tiger: 'You see a fierce tiger standing upright on its hind legs.',
  wolf: 'You see a fearsome wolf standing upright on its hind legs.',
  bear: 'You see a massive bear standing upright on its hind legs.',
  falcon: 'A fierce Werefalcon spreads its wings before you.',
  badger: 'You see a fierce badger standing upright on its hind legs.',
  vulture: 'A fierce vulture spreads its wings before you, carrion dripping from its talons.',
  boar: 'You see a fierce boar standing upright on its hind legs, tusks ready to impale.',
  jaguar: 'You see a fierce jaguar standing upright on its hind legs.',
};
