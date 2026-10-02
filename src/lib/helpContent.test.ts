import { describe, expect, it } from 'vitest';
import helpMd from '../../HELP.md?raw';
import { HELP_SECTIONS, HELP_TOPICS, parseHelpBody, PROMPT_DESIGN_CODES } from './helpContent';

function body(id: string): string {
  const topic = HELP_TOPICS.find((t) => t.id === id);
  if (!topic) throw new Error(`no help topic ${id}`);
  return topic.body;
}

describe('the help on values the game hides', () => {
  // Under lamented tears the game hides your vitals, affects, and group,
  // and Vosh shows that instead of zeros, missing affects, or solo.
  it('says what the vitals show', () => {
    const text = body('shape.read-vitals');
    expect(text).toContain(
      'When the game hides your vitals, as it does under lamented tears, every value reads `?` in dim text over an empty meter',
    );
    expect(text).toContain('Nothing turns yellow or red while they stay hidden.');
    expect(text).toContain('the status line drops the health of your target');
  });

  it('says what the affects and group panes show', () => {
    const text = body('shape.group-affects');
    expect(text).toContain(
      'When the game hides your affects or your group, as it does under lamented tears, the pane says so in place of its rows.',
    );
    expect(text).toContain('marks no tracked affect missing');
    expect(text).toContain('shows no member health from before');
  });
});

describe('the help on the vitals', () => {
  it('says the vitals leave the panel while your prompt is pinned, and how to keep them', () => {
    const text = body('shape.read-vitals');
    expect(text).toContain(
      '- Leave `Hide vitals while your prompt is pinned` on and the panel drops its vitals while `Where your prompt shows` is `Pinned`, so the panes take their room. Turn it off to keep them, or pick another place for your prompt, and they come back at once.',
    );
    expect(body('shape.prompt-show')).toContain(
      'While your prompt is pinned, the panel hides its vitals and gives their room to the panes. Turn off `Hide vitals while your prompt is pinned` under Layout, then Vitals, to keep them.',
    );
  });

  it('matches HELP.md word for word', () => {
    for (const id of ['shape.read-vitals', 'shape.prompt-show']) {
      const found = HELP_TOPICS.find((t) => t.id === id);
      if (!found) throw new Error(`no help topic ${id}`);
      expect(helpMd).toContain(`### ${found.number} ${found.title}\n\n${found.body}\n`);
    }
  });
});

describe('the help on the affects pane', () => {
  const topic = () => {
    const found = HELP_TOPICS.find((t) => t.id === 'shape.group-affects');
    if (!found) throw new Error('no affects topic');
    return found;
  };

  it('describes the timers first pane with the exact names', () => {
    const text = topic().body;
    expect(text).toContain('then the name exactly as the game sends it');
    expect(text).toContain('`+` means permanent and `-` means you do not have it');
    expect(text).toContain('keep their places as the hours change');
    expect(text).toContain('Harmful affects like `faerie fire` come first');
    expect(text).toContain('the last entry says how many more there are, like `5 more`');
    expect(text).not.toContain('duration mini bar');
    expect(text).not.toContain('renders nothing until you list');
  });

  it('describes the three styles, the markers, the gauge, and the tint', () => {
    const text = topic().body;
    expect(text).toContain(
      'Pick how the affects pane draws in Settings under Layout, then Affects',
    );
    expect(text).toContain('`Countdown` lists every affect by the hours it has left.');
    expect(text).toContain('`Grouped chips` puts what to recast first.');
    expect(text).toContain('the fill drains from the left as its hours run down');
    expect(text).toContain('the most hours Vosh has seen for it since you last cast it');
    expect(text).toContain('Pick a dot, a square, plus and minus, or none.');
    expect(text).toContain('Turn on `Tint what to recast`');
    // The writing style keeps colons and semicolons out of the prose.
    expect(text).not.toMatch(/[;:] /);
  });

  it('reads the same in HELP.md', () => {
    const { number, title, body: text } = topic();
    expect(helpMd).toContain(`### ${number} ${title}\n\n${text}\n`);
  });

  it('sends you to Characters for tracked affects everywhere in HELP.md', () => {
    expect(helpMd).not.toMatch(/[Tt]racked affects[^.\n]*`panels` tab/);
    expect(helpMd).toContain(
      'Pick the affects you track in Settings under Characters, then Tracked affects.',
    );
  });
});

describe('the help on password prompts', () => {
  const kept =
    'Lines you type at a password prompt are not saved. Each one shows as `> (hidden)` in its place.';

  it('says the session log keeps no line typed at a password prompt', () => {
    const text = body('characters-and-data.search-logs');
    expect(text).toContain('The log keeps what the game sent and each line you sent, marked `> `.');
    expect(text).toContain(kept);
    expect(helpMd).toContain(kept);
  });

  it('says a session from an older version can still hold a password, and how to clear it', () => {
    // Earlier builds wrote every sent line to logs.sqlite in full, and
    // the game shows some passwords as you type them, so the help must
    // not read as if none exist. #logs forget-passwords clears them
    // without deleting every saved session.
    const older =
      'Older versions of Vosh saved those lines in full, so a session you logged before updating can still show your password after a `> `. The game also shows two kinds of password as you type them, the one you set for a new character and any you give a command like `password <old> <new>`, and the log saves those in full in every version.';
    const changeIt =
      'If you copied or shared one of those sessions, change your password in the game.';
    for (const text of [body('characters-and-data.search-logs'), helpMd]) {
      expect(text).toContain(older);
      expect(text).toContain(changeIt);
      expect(text).not.toContain('That removes every saved session.');
    }
  });

  it('says the masked field keeps the password out of the log and the pipeline', () => {
    const text = body('get-connected.connect');
    const masked =
      'Nothing you type shows on screen, echoes to the terminal, lands in command history, or reaches the session log. Vosh sends it exactly as typed, with no aliases, variables, or `#` commands applied.';
    expect(text).toContain(masked);
    expect(helpMd).toContain(masked);
  });
});

describe('the help on loadouts', () => {
  // The shared catalog wizard keeps each profile file in place with every
  // setting but its items, puts a copy in profiles/legacy, and builds the
  // catalog only once.
  it('says what the wizard keeps, copies, and refuses', () => {
    const text = body('characters-and-data.loadouts');
    expect(text).not.toMatch(/parks/);
    expect(text).toContain('copies each profile file to `profiles/legacy/`');
    // The preset list is shared in loadout mode, so the help no longer
    // says every other setting stays with its profile.
    expect(text).not.toContain('Every other setting stays with its profile.');
    const presets = [
      'Every other setting stays with its profile except the presets.',
      'Loadout mode keeps one list of presets that are on, and every character shares it.',
      'The list starts with every preset that any profile file had on, and the preview names each character that gains or loses a preset.',
    ];
    for (const sentence of presets) {
      expect(text).toContain(sentence);
      expect(helpMd).toContain(sentence);
    }
    expect(text).not.toMatch(/run the migration wizard again/);
    expect(text).toContain('The migration wizard runs once');
    expect(text).toContain('while `profiles/legacy/` holds the copies from an earlier run');
    expect(text).toContain('or while an earlier run waits to finish at the next launch');
  });

  it('says what the legacy copies are and when you may copy one back', () => {
    // The help used to say only the copies held your items, which is
    // false in loadout mode, and then that a copy back dropped every
    // change since the move. With the catalog in place it spreads the
    // old items to every character instead.
    const text = body('characters-and-data.loadouts');
    const truth = [
      'After the move, `catalog.toml` holds your aliases, triggers, and macros, with every one you add or change later.',
      'Each file in `profiles/legacy/` is a backup of its profile as it was before the move.',
      'Never copy a backup back while `catalog.toml` sits in the app data folder.',
      'Vosh would lay the old aliases, triggers, and macros of the backup over the catalog, turn on for that character items that only other characters had, and at the next save put the old versions in the catalog for every character.',
      'To keep your items, leave `catalog.toml` where it is and change them in Automation settings.',
      'To build a new catalog from the backups, quit Vosh first, since Vosh saves `catalog.toml` again as it quits.',
      'Then move `catalog.toml` and `loadouts.toml` out of the app data folder, copy the files in `profiles/legacy/` back over the ones in `profiles/`, and move `profiles/legacy/` out too.',
      'Each profile comes back as it was before the move, every setting included, and loses every change you made to its settings since.',
      'Your items as they are now stay in the `catalog.toml` you moved out.',
    ];
    for (const sentence of truth) {
      expect(text).toContain(sentence);
      expect(helpMd).toContain(sentence);
    }
    expect(text).not.toMatch(/only the copies hold/);
    expect(helpMd).not.toMatch(/only the copies hold/);
    // A backup copied back beside the catalog does not drop the items you
    // added since. It lays its old items over the catalog for everyone.
    for (const doc of [text, helpMd]) {
      expect(doc).not.toContain('to the profile and to your shared items');
      expect(doc).not.toContain('Copying a backup over its file in `profiles/` brings back');
    }
  });

  it('says what the folders become in the shared catalog', () => {
    const text = body('characters-and-data.loadouts');
    expect(text).toContain('`combat (Healer)` holds the combat items only the Healer had');
    expect(text).toContain('`#group combat on` and `#group combat off` still turn on and off');
    expect(text).toContain('keeps each version, and the second one takes a name');
    expect(text).toContain('Triggers keep the order each character had them in');
  });

  it('says where the items live in loadout mode', () => {
    const text = body('fix-it.data-on-disk');
    expect(text).toContain(
      'In loadout mode the aliases, triggers, and macros live in `catalog.toml` instead',
    );
    expect(text).toContain('`profiles/legacy/`');
  });
});

const tickTopic = () => {
  const topic = HELP_TOPICS.find((t) => t.id === 'tick.tick-timer');
  if (!topic) throw new Error('no tick topic');
  return topic;
};

describe('the tick timer help', () => {
  it('measures the same tick window from the tick, the way the timer does', () => {
    const text = tickTopic().body;
    expect(text).toContain('A signal within 2 seconds of a tick counts as that tick');
    expect(text).not.toContain('within 2 seconds of each other');
  });

  it('says a connection starts the tick and a switch keeps it running', () => {
    expect(tickTopic().body).toContain(
      'Every connection starts the tick, and switching characters keeps it running until you turn it off.',
    );
  });

  it('reads the same in HELP.md', () => {
    const { number, title, body: text } = tickTopic();
    expect(helpMd).toContain(`### ${number} ${title}\n\n${text}\n`);
  });
});

describe('the help on reading your prompt with a pattern', () => {
  const howTo =
    '- Tell Vosh how to read your prompt with `#prompt game {setting}` and `#prompt fight {setting}`, the codes you type in the game, or with `#prompt {regex}`, each named group like `(?<hp>\\d+)` a value. `#prompt` alone says how Vosh reads it, and `#unprompt` stops.';
  const reference =
    '- `#prompt game {setting}` and `#prompt fight {setting}` read your prompt in this profile from the codes of your PROMPT and fight prompt, `#prompt {regex}` reads it with a pattern, `#prompt` says how Vosh reads it, and `#unprompt` stops reading it.';

  it('says the codes and the pattern are read in the profile, with no trigger', () => {
    const all = HELP_TOPICS.map((t) => t.body).join('\n');
    expect(all).toContain(howTo);
    expect(all).toContain(reference);
    expect(all).not.toContain('binds named captures to prompt vars');
    expect(helpMd).toContain(howTo);
    expect(helpMd).toContain(reference);
  });
});

describe('the help on the default prompt design', () => {
  const howTo =
    "- Use Vosh's default prompt design with `#prompt default`. It takes the place of the design in this profile, and Vosh keeps yours as an earlier design.";
  const reference =
    "- `#prompt default` puts Vosh's default design in place of the design in this profile and keeps yours as an earlier design.";

  it('lists #prompt default with the slash commands, after #prompt show', () => {
    const automate = body('automate.slash-commands');
    const listed = body('reference.slash-commands');
    expect(automate).toContain(`\`#prompt show text|lifted|pinned\`.\n${howTo}\n`);
    expect(listed).toContain(`above the command line.\n${reference}\n`);
    expect(helpMd).toContain(howTo);
    expect(helpMd).toContain(reference);
    // The writing style keeps colons and semicolons out of the prose.
    expect(`${howTo} ${reference}`).not.toMatch(/[;:] /);
  });
});

describe('the help on the prompt capture move', () => {
  const paragraph =
    "Each profile reads your prompt on its own. Vosh moves the capture trigger that `#prompt` made into each profile that draws your own prompt, turns the trigger off, and tells you once at launch. Profiles that draw nothing then show the game's prompt. On The Forsaken Lands the moved pattern switches to your prompt codes the first time the game shows them, when you log in or when you type `prompt`. From then on Vosh follows each prompt you set in the game and keeps your design and the draw switch as they are. When a color code runs into a code in that prompt, or when the pattern fills a value under a name no prompt code fills, such as `health`, the pattern stays and `#prompt` says why. A pattern you set with `#prompt {regex}` never switches. An older version of Vosh shows the game's prompt in every profile until you turn `prompt-capture` on again under Automation. Back in this version, Vosh moves the capture into your profiles again and turns the trigger off.";

  it('says what the move does, when the pattern switches, and what an older version shows, in both places', () => {
    const profiles = HELP_TOPICS.find((t) => t.number === '7.1');
    expect(profiles?.body).toContain(paragraph);
    expect(helpMd).toContain(`${paragraph}\n`);
  });
});

describe('the help on forgetting passwords in the session log', () => {
  const logsParagraph =
    'Type `#logs forget-passwords` to count the lines that hold a password. Vosh says how many it found and in how many sessions, and it never shows the lines themselves. Type `#logs forget-passwords now` to blank them. Each one then reads `> (hidden)`, and Vosh rewrites `logs.sqlite` so the old text is gone from the disk too. On a large log this takes a few seconds, and new game text waits until it finishes. The rewrite needs free disk space about the size of `logs.sqlite`. When Vosh cannot finish it, the lines stay blanked, Vosh says so, and the next `#logs forget-passwords now` finishes the rewrite. A backup of your disk, like Time Machine, keeps its own copy of the old file.';
  const referenceBullet =
    '- `#logs forget-passwords` counts the lines in your session log where you sent a password, and `#logs forget-passwords now` blanks them.';
  const howToBullet =
    '- Clear old passwords out of your session log with `#logs forget-passwords`, then `#logs forget-passwords now`.';

  /** The topic with `id`, which must exist. */
  function topic(id: string) {
    const found = HELP_TOPICS.find((t) => t.id === id);
    if (!found) throw new Error(`no help topic ${id}`);
    return found;
  }

  it('says how to count and blank them in the session logs topic', () => {
    expect(body('characters-and-data.search-logs')).toContain(logsParagraph);
    expect(helpMd).toContain(logsParagraph);
  });

  it('does not send you to the log from before Vosh took its name', () => {
    // The builds that wrote that copy logged game output only, so it
    // holds no line you sent.
    for (const text of [body('characters-and-data.search-logs'), helpMd]) {
      expect(text).not.toContain('took its name');
    }
  });

  it('lists the command with every slash command', () => {
    const { number, title, body: text } = topic('reference.slash-commands');
    expect(text).toContain(referenceBullet);
    expect(helpMd).toContain(`### ${number} ${title}\n\n${text}\n`);
  });

  it('lists the command where slash commands are taught', () => {
    const { number, title, body: text } = topic('automate.slash-commands');
    expect(text).toContain(howToBullet);
    expect(helpMd).toContain(`### ${number} ${title}\n\n${text}\n`);
  });
});

describe('the help on the chat pane', () => {
  it('says how a message prints and where its color comes from', () => {
    const text = body('shape.chat-pane');
    expect(text).toContain('Read a line as `[tell] Tolliver: meet at the bank`.');
    expect(text).toContain('Wrapped lines hang two cells in');
    expect(text).toContain("from your theme's terminal colors");
    expect(text).toContain('Point at a message to see when it arrived.');
    expect(text).not.toContain('tab per channel');
    expect(text).not.toContain('`visible/total`');
  });

  it('says how to recolor a channel from the pane menu', () => {
    expect(body('shape.chat-pane')).toContain(
      "- Recolor a channel from the pane's menu. Choose `Channel colors`, then the channel, then `Default` or one of your theme's 16 terminal colors. The pane follows at once, each profile keeps its own picks, and a theme switch carries them along. `Reset all` gives every channel its default again.",
    );
  });

  it('says Vosh catches the tells you send with a preset it turns on once', () => {
    const text = body('shape.chat-pane');
    expect(text).toContain(
      '- See the tells you send. The game sends no GMCP for them, so the `Tells you send` preset routes the line the game prints for each one. Vosh turns it on for every profile, once, unless you had turned every preset off.',
    );
    expect(text).toContain('Each one reads `[tell] to Tolliver: text`');
    expect(text).toContain(
      'The pane skips the `You tell your group` line, because your gtell already arrives over GMCP.',
    );
    expect(text).toContain('Turn the preset off in Settings under Automation, then Presets.');
    // No help still asks you to build the trigger yourself.
    expect(text).not.toContain('A trigger on `^You (tell|project to) `');
    expect(helpMd).not.toContain('A trigger on `^You (tell|project to) `');
  });

  it('matches HELP.md word for word', () => {
    const topic = HELP_TOPICS.find((t) => t.id === 'shape.chat-pane');
    if (!topic) throw new Error('no help topic shape.chat-pane');
    expect(helpMd).toContain(`### ${topic.number} ${topic.title}\n\n${topic.body}\n`);
  });
});

describe('the help on where your prompt shows', () => {
  const topic = () => {
    const found = HELP_TOPICS.find((t) => t.id === 'shape.prompt-show');
    if (!found) throw new Error('no help topic shape.prompt-show');
    return found;
  };

  it('names the row and each place it offers', () => {
    const text = topic().body;
    expect(topic().section).toBe('Shape the window');
    expect(text).toContain(
      'Open Settings, choose Input, and pick a place under `Where your prompt shows` in the Prompt section.',
    );
    expect(text).not.toContain('`Advanced`');
    expect(text).toContain('- `In the text` shows each prompt where the game sends it.');
    expect(text).toContain('- `Lifted` keeps every prompt in the text on a raised band');
    expect(text).toContain('- `Pinned` takes your prompts out of the text');
    expect(text).toContain('Every prompt still reaches the session log and your Prompts triggers.');
  });

  it('says what the choice needs and where it saves', () => {
    const text = topic().body;
    expect(text).toContain('The choice needs Vosh to read your prompt.');
    expect(text).toContain('`#prompt show lifted`');
    expect(text).toContain('The choice saves in the `[prompt]` table of your profile as `show`.');
    expect(text).toContain(
      'An older version of Vosh ignores it and shows your prompt in the text.',
    );
  });

  it('says where xterm shows a lifted prompt plain', () => {
    expect(topic().body).toContain(
      'With the xterm renderer, the newest 1000 prompts keep their bands and older ones show plain.',
    );
  });

  it('lists #prompt show with the slash commands', () => {
    expect(body('automate.slash-commands')).toContain('`#prompt show text|lifted|pinned`');
    expect(body('reference.slash-commands')).toContain('`#prompt show text|lifted|pinned`');
  });

  it('matches HELP.md word for word', () => {
    for (const id of ['shape.prompt-show', 'automate.slash-commands', 'reference.slash-commands']) {
      const found = HELP_TOPICS.find((t) => t.id === id);
      if (!found) throw new Error(`no help topic ${id}`);
      expect(helpMd).toContain(`### ${found.number} ${found.title}\n\n${found.body}\n`);
    }
  });
});

describe('the help on prompt design codes', () => {
  // Section 7.1 of the prompt build spec: each row a code and one
  // sentence.
  const ROWS: [string[], string][] = [
    [['%hp', '%mana', '%move'], 'Your current Health, Mana or Moves.'],
    [['%maxhp', '%maxmana', '%maxmove'], 'The most you can have.'],
    [['%pct_hp'], 'Health as a percent with no sign. Add %% for the sign.'],
    [['%hp_bar:10:auto'], 'A bar ten cells wide, colored by how full it is.'],
    [['%{gold:grouped}'], 'Any value from the picker, in any of its forms.'],
    [['%{gold:thousands}'], 'Gold in thousands with one decimal, as 12.3K.'],
    [['%{hour:ampm}'], 'The game hour as 3PM, with 12AM for midnight and 12PM for noon.'],
    [['%{tick:since}'], 'The seconds since the last tick, as 16s.'],
    [['%c_green', '%c_hp'], "A theme color, or Health's color by how full it is."],
    [
      ['%{c:hp:steps}'],
      'Colors by how full Health is in eleven steps from red to green, one for each tenth.',
    ],
    [
      ['%{c:#80c8ff}', '%{c:128,200,255}'],
      'Any color you choose, as hex or as red, green and blue.',
    ],
    [['%bg_blue', '%{bg:#3b4252}'], 'The ground behind the text, in any form a text color takes.'],
    [['%c_default'], 'Back to the terminal text color. Bold and italic stay on.'],
    [['%c_reset'], 'Back to plain text with every color and style off.'],
    [['%s_italic', '%s_bold', '%s_underline', '%s_off'], 'Turns a style on, or every style off.'],
    [
      ['%s_strike', '%s_dim', '%s_inverse'],
      'Strikes the text through, dims it, or swaps its color and ground.',
    ],
    [['%s_blink'], 'Makes the text blink.'],
    [
      ['%s_double', '%s_curly', '%s_dotted', '%s_dashed'],
      'Underlines with two lines, a wave, dots or dashes.',
    ],
    [['%{ul:#bf616a}', '%{ul:default}'], 'Colors the underline, or gives it the text color again.'],

    [['%nl'], 'Starts a new line.'],
    [['%{right}'], 'Pushes the rest of its line to the right edge of the terminal.'],
    [
      ['%{if:fight}', '%{ifnot:fight}', '%{end}'],
      'Shows what sits between them only in a fight, or only out of one.',
    ],
    [['%{raw}'], 'Your prompt exactly as the game sent it.'],
    [['%%'], 'A percent sign.'],
  ];

  const topic = () => {
    const found = HELP_TOPICS.find((t) => t.id === 'reference.prompt-codes');
    if (!found) throw new Error('no prompt codes topic');
    return found;
  };

  it('lists every code with its one sentence', () => {
    expect(PROMPT_DESIGN_CODES.map((row) => [row.codes, row.text])).toEqual(ROWS);
  });

  it('draws them as a table under Reference', () => {
    const { number, title, section, body: text } = topic();
    expect([number, title, section]).toEqual(['9.3', 'Prompt design codes', 'Reference']);
    expect(text).toMatch(/^\| Code +\| What it does +\|\n\| -+ \| -+ \|$/m);
    expect(text).toMatch(
      /^\| `%hp` `%mana` `%move` +\| Your current Health, Mana or Moves\. +\|$/m,
    );
    expect(text).toMatch(/^\| `%%` +\| A percent sign\. +\|$/m);
    const blocks = parseHelpBody(text);
    const table = blocks.find((b) => b.kind === 'table');
    expect(table).toEqual({
      kind: 'table',
      head: ['Code', 'What it does'],
      rows: ROWS.map(([codes, sentence]) => [codes.map((c) => `\`${c}\``).join(' '), sentence]),
    });
    // The writing style keeps colons and semicolons out of the prose.
    for (const block of blocks) {
      if (block.kind === 'paragraph') expect(block.text).not.toMatch(/[;:] /);
    }
  });

  it('matches HELP.md word for word', () => {
    const { number, title, body: text } = topic();
    expect(helpMd).toContain(`### ${number} ${title}\n\n${text}\n`);
  });
});

describe('HELP.md', () => {
  // HELP.md mirrors the catalog word for word, every topic under its
  // section, so the file and the Help window never tell two stories.
  it('holds every topic word for word', () => {
    for (const t of HELP_TOPICS) {
      expect(helpMd, `${t.number} ${t.title}`).toContain(
        `### ${t.number} ${t.title}\n\n${t.body}\n`,
      );
    }
  });

  it('holds no topic the catalog does not', () => {
    const headings = [...helpMd.matchAll(/^### (.+)$/gm)].map((m) => m[1]);
    expect(headings).toEqual(HELP_TOPICS.map((t) => `${t.number} ${t.title}`));
    const sections = [...helpMd.matchAll(/^## (.+)$/gm)].map((m) => m[1]);
    expect(sections).toEqual(HELP_SECTIONS);
  });

  it('names the Help window, not the old top bar button', () => {
    expect(helpMd).not.toContain('top bar');
    expect(helpMd).toContain('Help window');
  });
});

describe('the help on the one window', () => {
  // The help describes the window as it is: the title band, the session
  // button, the panel and its panes, the status line, and Settings by
  // its groups. None of the Ember chrome it replaced.
  const all = () => HELP_TOPICS.map((t) => `${t.title}\n${t.body}`).join('\n');

  it('never sends you to chrome that is gone', () => {
    for (const gone of [
      'top bar',
      'session chip',
      'status bar',
      'room strip',
      'the well',
      'gear button',
      'left rail',
      'settings window',
      'combat pane',
      'Profile scope',
      'zone select',
      'Panel layout, where',
      'clear buffer',
      'search scrollback',
      'open splits',
      'input bar',
      'input row',
      'Char.Worth',
    ]) {
      expect(all(), gone).not.toContain(gone);
    }
  });

  it('opens the session from the title band', () => {
    const text = body('get-connected.connect');
    expect(text).toContain('You connect from the session button, centered in the title band');
    expect(text).toContain('choose `New connection…` instead');
    expect(body('fix-it.reconnect')).toContain('Choose `Edit connection…` to check the address.');
  });

  it('arranges panes in the panel itself', () => {
    const text = body('shape.arrange-panels');
    expect(text).toContain('Add a pane with `Add a pane`, the plus button in the title band.');
    expect(text).toContain('`Split right` and `Split down`');
    expect(text).toContain('`Show here instead`');
    expect(text).toContain('from 200 to 800 points');
  });

  it('keeps the room and its people under the map', () => {
    const text = body('shape.use-the-map');
    expect(text).toContain('The first names the room you stand in and its exits.');
    expect(HELP_TOPICS.some((t) => t.id === 'shape.room-strip')).toBe(false);
    expect(HELP_TOPICS.some((t) => t.id === 'shape.split-the-well')).toBe(false);
  });

  it('lists #help with words with the slash commands', () => {
    expect(body('reference.slash-commands')).toContain(
      '`#help <words>` opens Help on those words.',
    );
    expect(body('reference.keyboard-shortcuts')).toContain('`Cmd+/` opens Help.');
  });

  it('keeps every topic number unique and in order within its section', () => {
    const numbers = HELP_TOPICS.map((t) => t.number);
    expect(new Set(numbers).size).toBe(numbers.length);
    HELP_SECTIONS.forEach((section, s) => {
      HELP_TOPICS.filter((t) => t.section === section).forEach((t, i) => {
        expect(t.number).toBe(`${s + 1}.${i + 1}`);
      });
    });
  });
});

describe('the help on Room triggers', () => {
  it('says what Room matches and how Vosh finds those lines', () => {
    expect(body('automate.first-trigger')).toContain(
      '- Pick `Room` in `Match` to match only the things and people a room lists after its exits line. The game sends a `Room.Chars` packet with each look, and Vosh counts the people lines from it, so a say or an arrival after the look stays a plain line.',
    );
  });

  it('matches HELP.md word for word', () => {
    const found = HELP_TOPICS.find((t) => t.id === 'automate.first-trigger');
    if (!found) throw new Error('no trigger topic');
    expect(helpMd).toContain(`### ${found.number} ${found.title}\n\n${found.body}\n`);
  });
});

describe('the help body format', () => {
  it('reads paragraphs, lists and tables', () => {
    expect(
      parseHelpBody('One line.\n\n- a\n- b\n\n| A | B |\n|---|---|\n| `x` | y. |\n| z | w |'),
    ).toEqual([
      { kind: 'paragraph', text: 'One line.' },
      { kind: 'list', items: ['a', 'b'] },
      {
        kind: 'table',
        head: ['A', 'B'],
        rows: [
          ['`x`', 'y.'],
          ['z', 'w'],
        ],
      },
    ]);
  });
});
