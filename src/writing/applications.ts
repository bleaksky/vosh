import type { Guide } from './kinds';

// The guide of an application reads its subject the way the game does,
// and shows the help of the application it names. The game checks a
// class application at post when the subject holds a word that starts
// with the class and one that starts with app (recycle.c:2705, 2825,
// is_auto_name in handler.c:2723), and staff read the rest by the words
// their helps ask for. Vosh checks none of these rows and puts no word
// in your subject.

interface Application {
  /** What the guide calls it. */
  name: string;
  /** The words of the subject, each the start of a word in it. */
  words: string[];
  guide: Guide;
}

const LAST = 'Tell the immortals your history, goals, and deeds';

const APPLICATIONS: Application[] = [
  {
    name: 'psi',
    words: ['psi', 'app'],
    guide: {
      head: 'From help psi requirements',
      rows: [
        'Necromancer, invoker, battlemage, or monk',
        'Illithid, elf, half-elf, wood-elf, half-drow, human, gnome, avian, or faerie',
        'Rank 50',
        'Psi Application in the subject',
        'Good or neutral, unless you’re an illithid',
        'Not in a cabal',
        'At least 20 RP points',
        'A finished description and history',
        LAST,
      ],
      words: ['The game checks whether you can apply when you post.'],
      help: 'psi requirements',
    },
  },
  {
    name: 'crusader',
    words: ['crusader', 'app'],
    guide: {
      head: 'From help crusader requirements',
      rows: [
        'Warrior, paladin, or berserker',
        'Elf, half-elf, human, or storm giant',
        'Rank 50',
        'Crusader application in the subject',
        'Good, and not moderate',
        'A follower of the One God',
        'Not an avatar, and not in a cabal',
        'At least one player killed',
        'At least 20 RP points',
        'A finished history',
      ],
      words: ['The game checks whether you can apply when you post.'],
      help: 'crusader requirements',
    },
  },
  {
    name: 'avatar',
    words: ['avatar', 'app'],
    guide: {
      head: 'From help avatar requirements',
      rows: [
        'Ranks 15 to 30',
        'Avatar application in the subject',
        'Good, and not moderate',
        'A follower of Purity and the powers of Life',
        'At least one player killed',
        'At least 20 RP points',
        'A finished history',
      ],
      words: ['The game checks whether you can apply when you post.'],
      help: 'avatar requirements',
    },
  },
  {
    name: 'demon',
    words: ['demon', 'app'],
    guide: {
      head: 'From help demon requirements',
      rows: [
        'Warrior, berserker, cleric, shaman, battlemage, necromancer, or dark knight',
        'Human and chaotic',
        'Ranks 15 to 30',
        'Demon application in the subject',
        'Evil, and not moderate',
        'A follower of Discord and the powers of Chance',
        'At least one player killed',
        'At least 20 RP points',
        'A finished history',
      ],
      words: ['The game checks whether you can apply when you post.'],
      help: 'demon requirements',
    },
  },
  {
    name: 'undead',
    words: ['undead', 'app'],
    guide: {
      head: 'From help undead requirements',
      rows: [
        'Any class but ranger',
        'Human',
        'Ranks 15 to 30',
        'Undead application in the subject',
        'Evil, and not moderate',
        'A follower of the powers of Death',
        'At least one player killed',
        'At least 20 RP points',
        'A finished history',
      ],
      words: ['The game checks whether you can apply when you post.'],
      help: 'undead requirements',
    },
  },
  {
    name: 'vampire',
    words: ['vampire', 'app'],
    guide: {
      head: 'From help vampire requirements',
      rows: [
        'Dark knight',
        'Human',
        'Ranks 15 to 30',
        'Vampire application in the subject',
        'Evil, and not moderate',
        'A follower of the powers of Death',
        'At least one player killed',
        'At least 20 RP points',
        'A finished history',
      ],
      words: ['The game checks whether you can apply when you post.'],
      help: 'vampire requirements',
    },
  },
  {
    name: 'lich',
    words: ['lich', 'app'],
    guide: {
      head: 'From help lich requirements',
      rows: [
        'Necromancer, invoker, or battlemage',
        'Human or half-drow',
        'Rank 50',
        'Lich and application in the subject',
        'Evil, and not moderate',
        'A follower of the powers of Death',
        'Not in a cabal',
        'At least one player killed',
        'At least 20 RP points',
        'A finished description and history',
        LAST,
      ],
      words: ['The game checks whether you can apply when you post.'],
      help: 'lich requirements',
    },
  },
  {
    name: 'journeyman',
    words: ['journeyman', 'app'],
    guide: {
      head: 'From help journeyman',
      rows: [
        'For players who need help to compete',
        'Never a quest class or race after it',
        'Only you and the immortals see it',
      ],
      words: ['The game checks whether you can apply when you post.'],
      help: 'journeyman',
    },
  },
  {
    name: 'breaker',
    words: ['breaker', 'app'],
    guide: {
      head: 'From help breaker requirements',
      rows: [
        'Ranks 30 to 50',
        'Breaker application in the subject',
        'Evil for a soulreaver, neutral for a shepherd',
        'A follower of the powers of Death, Chance, or Chaos',
        LAST,
      ],
      words: ['The staff look for these words in your subject.'],
      help: 'breaker requirements',
    },
  },
  {
    name: 'noble',
    words: ['noble', 'app'],
    guide: {
      head: 'From help noble',
      rows: [
        'Noble application in the subject',
        'Which town’s house you wish to join',
        'Every qualification you feel you have',
      ],
      words: ['The staff look for these words in your subject.'],
      help: 'noble',
    },
  },
  {
    name: 'royal',
    words: ['royal', 'app'],
    guide: {
      head: 'From help royal',
      rows: [
        'Royal application in the subject',
        'Already of noble blood, with a seat open in your city',
        'Every qualification and why you should be accepted',
      ],
      words: ['The staff look for these words in your subject.'],
      help: 'royal',
    },
  },
  {
    name: 'custom warcry',
    words: ['custom', 'warcry', 'app'],
    guide: {
      head: 'From help custom warcry',
      rows: [
        'Custom Warcry Application in the subject',
        'Rank 50, in a guild that allows a warcry',
        'A good roleplay basis',
        'Short. A warcry, not a warsong',
      ],
      words: ['The staff look for these words in your subject.'],
      help: 'custom warcry',
    },
  },
  {
    name: 'cabal',
    words: ['cabal', 'app'],
    guide: {
      head: 'From help cabal',
      rows: [
        'Cabal application in the subject',
        'To the cabal’s name',
        'A finished description first',
        'Well written and formatted',
      ],
      words: ['The cabal votes on it once you post.'],
      help: 'cabal',
    },
  },
];

/** Help qrace's asks, for an application you mark as a custom race in
 *  the card's ⋯ menu, since nothing in a subject names one. */
export const CUSTOM_RACE: Guide = {
  head: 'From help qrace',
  rows: [
    'Exceptional RP from the day you arrived',
    'A race close to yours, within mortal bounds',
    'An extremely well written description first',
    'Your character’s history, thought out and formatted',
  ],
  words: ['The help asks for 70 characters a line at most, so Vosh keeps your lines to 70.'],
  help: 'qrace',
};

/** The application your subject names, read the way the game reads it:
 *  each word of the application starts a word of the subject. */
export function applicationOf(subject: string): Application | null {
  const words = subject.toLowerCase().split(/\s+/).filter(Boolean);
  return (
    APPLICATIONS.find((app) => app.words.every((w) => words.some((word) => word.startsWith(w)))) ??
    null
  );
}

/** The guide for an application with `subject`. With no words it knows,
 *  it lists the words each application's help asks for. */
export function applicationGuide(subject: string, customRace: boolean): Guide {
  if (customRace) return CUSTOM_RACE;
  const app = applicationOf(subject);
  if (app) return app.guide;
  return {
    head: 'Applications',
    rows: [
      'Psi, crusader, avatar, demon, undead, vampire, lich, or journeyman, with application',
      'Breaker, noble, royal, or custom warcry application',
      'Cabal application, to the cabal’s name',
      'A custom race, from the ⋯ menu',
    ],
    words: [
      'Put the words your application’s help asks for in the subject, and its guide shows here.',
    ],
    help: 'application',
  };
}
