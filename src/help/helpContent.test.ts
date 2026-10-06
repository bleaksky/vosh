import { describe, expect, it } from 'vitest';
import helpMd from '../../HELP.md?raw';
import { HELP_SECTIONS, HELP_TOPICS, parseHelpBody, PROMPT_DESIGN_CODES } from './helpContent';
import { ALERT_PRESETS } from '../automation/alertPresets';
import { SETTINGS_MENU } from '../terminal/settingsMenu';

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
      '- Leave `Hide vitals while your prompt is pinned` on and the panel drops its vitals while `Where your prompt shows` is `Pinned`, so the panes take their room. In a fight your opponent keeps its row at the foot of the panel. Turn it off to keep them, or pick another place for your prompt, and they come back at once.',
    );
    expect(body('shape.prompt-show')).toContain(
      "While your prompt is pinned, the panel hides its vitals and gives their room to the panes, all but your opponent's row in a fight. Turn off `Hide vitals while your prompt is pinned` under Layout, then Vitals, to keep them.",
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

  it('describes the four styles, the markers, the gauge, and the tint', () => {
    const text = topic().body;
    expect(text).toContain(
      'Pick how the affects pane draws in Settings under Layout, then Affects',
    );
    expect(text).toContain('`Countdown` lists every affect by the hours it has left.');
    expect(text).toContain('`Grouped chips` puts what to recast first.');
    expect(text).toContain(
      '`Draining chips` does the same and colors only the hours a chip has left.',
    );
    expect(text).toContain('fills only the share that matches the hours it has left');
    expect(text).toContain('A missing affect is a dotted red chip.');
    expect(text).not.toContain('dashed');
    expect(text).toContain('Both chip styles always mark what to recast.');
    expect(text).toContain('the fill drains from the left as its hours run down');
    expect(text).toContain('the most hours Vosh has seen for it since you last cast it');
    expect(text).toContain('Pick a dot, a square, plus and minus, or none.');
    expect(text).toContain('Turn on `Tint what to recast`');
    // The writing style keeps colons and semicolons out of the prose.
    expect(text).not.toMatch(/[;:] /);
  });

  it('says when affects warn and turn red, and how to change it', () => {
    const text = topic().body;
    expect(text).toContain(
      'Unless you change them, that is two hours or fewer for yellow and one hour or none for red.',
    );
    expect(text).toContain(
      'Set `Running out at` and `Almost gone at` in Settings under Layout, then Affects, or choose `Change when affects warn…` in the pane',
    );
    expect(text).toContain('almost gone never goes over running out');
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

describe('the help on searching the logs', () => {
  // Session means a tab in the sidebar, so the log rows call one
  // logged connection a log (Q21).
  it('calls one logged connection a log, in both places', () => {
    const topic = HELP_TOPICS.find((t) => t.id === 'characters-and-data.search-logs');
    expect(topic?.number).toBe('7.4');
    const text = body('characters-and-data.search-logs');
    for (const line of [
      'A log is the record of one connection, so a session that connects three times saves three.',
      'The row counts your saved logs and lines.',
      'Pick a log in the menu at the right to search only that one. `All logs` searches everything.',
      'With one log picked, the copy button beside the menu copies that whole log to your clipboard as plain text.',
    ]) {
      expect(text).toContain(line);
      expect(helpMd).toContain(line);
    }
    expect(helpMd).toContain(`### 7.4 ${topic?.title}\n\n${text}\n`);
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

  it('says a log from an older version can still hold a password, and how to clear it', () => {
    // Earlier builds wrote every sent line to logs.sqlite in full, and
    // the game shows some passwords as you type them, so the help must
    // not read as if none exist. #logs forget-passwords clears them
    // without deleting every saved log.
    const older =
      'Older versions of Vosh saved those lines in full, so a log saved before you updated can still show your password after a `> `. The game also shows two kinds of password as you type them, the one you set for a new character and any you give a command like `password <old> <new>`, and the log saves those in full in every version.';
    const changeIt = 'If you copied or shared one of those logs, change your password in the game.';
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
    'Type `#logs forget-passwords` to count the lines that hold a password. Vosh says how many it found and in how many logs, and it never shows the lines themselves. Type `#logs forget-passwords now` to blank them. Each one then reads `> (hidden)`, and Vosh rewrites `logs.sqlite` so the old text is gone from the disk too. On a large log this takes a few seconds, and new game text waits until it finishes. The rewrite needs free disk space about the size of `logs.sqlite`. When Vosh cannot finish it, the lines stay blanked, Vosh says so, and the next `#logs forget-passwords now` finishes the rewrite. A backup of your disk, like Time Machine, keeps its own copy of the old file.';
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

describe('the help on the right click menu', () => {
  it('names every row of the Settings list as the menu shows it', () => {
    const text = body('play.right-click-menu');
    expect(text).toContain('- `Settings` opens a list beside the menu.');
    for (const row of SETTINGS_MENU.flat()) {
      expect(text, row.label).toContain(`\`${row.label}\``);
    }
    expect(text).toContain('open Settings under Automation on that list.');
    expect(text).toContain('`Help` opens the Help window.');
  });

  it('says how the keys reach the list and how Esc leaves it', () => {
    const text = body('play.right-click-menu');
    expect(text).toContain(
      '`ArrowRight` or `Enter` on `Settings` opens its list on the first row, and `ArrowLeft` steps back out.',
    );
    expect(text).toContain('`Esc` closes the list first, then the menu.');
    expect(text).toContain('Near the right edge the Settings list opens on the left of the menu');
    expect(body('reference.keyboard-shortcuts')).toContain(
      'In the terminal menu. `ArrowUp` and `ArrowDown` move through the items, `Enter` picks one, `ArrowRight` opens the Settings list, `ArrowLeft` steps back out of it, and `Escape` closes the list, then the menu.',
    );
  });

  it('opens Help from the terminal menu in HELP.md too', () => {
    expect(helpMd).toContain('from Settings in the terminal right click menu');
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

  it('names the button in Customize prompt that picks the same places', () => {
    const text = topic().body;
    expect(text).toContain(
      'At the foot of Customize prompt, the button beside `Draw your prompt` names where your prompt shows now. Click it and pick another place, and Customize prompt moves with your prompt.',
    );
    expect(text).not.toContain('the card');
    // Customize prompt asks for your prompt before its foot shows, so
    // only the row says what to do first.
    expect(text).toContain('Until it does, the row stays off and says what to do first,');
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
    [
      ['%{hp:pct:game}'],
      'Health as a percent with no sign, rounded down as the game does, so 37.5 reads 37.',
    ],
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
    expect(text).toContain('choose `New session…` instead');
    expect(text).not.toContain('New connection');
    expect(body('fix-it.reconnect')).toContain(
      'Choose `Edit connection…` to check the address of this session.',
    );
    expect(body('fix-it.reconnect')).not.toContain('New connection');
  });

  it('walks through the New session form, its Profile row and the note on the play port', () => {
    const text = body('get-connected.sessions');
    expect(text).toContain('Vosh adds a row that reads `New session`');
    expect(text).toContain('The caret waits in `Port`');
    expect(text).toContain(
      '`Profile` starts on a profile pinned to that host and port, then one that claims the host on any port, then the profile you were playing.',
    );
    expect(text).toContain('`Build is pinned to The Forsaken Lands 1825.`');
    expect(text).toContain(
      "`Tolliver's session plays Default too. An edit in either reaches both.`",
    );
    expect(text).toContain(
      "While another session is connected to the world's own port, such as `1848` for The Forsaken Lands, and you dial that port too",
    );
    expect(text).toContain(
      '`Tolliver is connected to this world. HELP MULTI lists “Having more than one character logged on at once.”` The build port, `1825`, never shows it.',
    );
    expect(text).toContain(
      'Click `Cancel` or press `Escape` to close the new row. Vosh writes nothing.',
    );
  });

  it('reads the same in HELP.md for each topic that opens a session', () => {
    for (const id of ['get-connected.connect', 'get-connected.sessions', 'fix-it.reconnect']) {
      const found = HELP_TOPICS.find((t) => t.id === id);
      if (!found) throw new Error(`no help topic ${id}`);
      expect(helpMd).toContain(`### ${found.number} ${found.title}\n\n${found.body}\n`);
    }
  });

  it('arranges panes in the panel itself', () => {
    const text = body('shape.arrange-panels');
    expect(text).toContain('Add a pane with `Add a pane`, the plus button in the title band.');
    expect(text).toContain('`Split right` and `Split down`');
    expect(text).toContain('`Show here instead`');
    expect(text).toContain('from 200 to 800 points');
  });

  it('tells you how to open Settings on every platform, the gear first', () => {
    // Windows and Linux have no menu bar, so the title band's gear and
    // Ctrl+, are how you find Settings there.
    const found = HELP_TOPICS.find((t) => t.id === 'shape.arrange-panels');
    if (!found) throw new Error('no panel topic');
    expect(found.body).toContain(
      'Open Settings with the gear at the right end of the title band, after the panel button, or press `Cmd+,` on macOS or `Ctrl+,` elsewhere.',
    );
    expect(found.body).toContain(
      '`Open settings` in the palette and the `Settings` list in the terminal right click menu reach it too.',
    );
    expect(helpMd).toContain(`### ${found.number} ${found.title}\n\n${found.body}\n`);
  });

  it('says which theme shows for a theme that left Vosh', () => {
    expect(body('make-it-yours.switch-themes')).toContain(
      'If you chose one, Vosh shows the theme that took its place until you pick another, One Half Dark for One Dark, Rubric for Vellum, and Melange Light for Everforest Light.',
    );
  });

  it('keeps the room and its people under the map', () => {
    const text = body('shape.use-the-map');
    expect(text).toContain('The first names the room you stand in. The name takes the color');
    expect(text).toContain(
      'The second row names the terrain and the region, like `Inside` and `Coastal North`, with the exits at its right.',
    );
    expect(text).toContain(
      'A short pane gives up rows of people first, then the terrain row, and keeps the room, with its exits beside the name.',
    );
    expect(HELP_TOPICS.some((t) => t.id === 'shape.room-strip')).toBe(false);
    expect(HELP_TOPICS.some((t) => t.id === 'shape.split-the-well')).toBe(false);
  });

  it('names the 3D style, its floors and view, the zoom gesture and bent exits', () => {
    const text = body('shape.use-the-map');
    expect(text).toContain('Pick `Squares`, `Glyphs`, `Tileset`, or `3D`');
    expect(text).toContain('a short tick out of a room marks an exit that leads past the room');
    expect(text).toContain('Scroll or pinch over the map to zoom it, in any style');
    expect(text).not.toContain('hold `Cmd` or `Ctrl`');
    expect(text).toContain('drag the map to turn and tilt it');
    expect(text).toContain('`Reset view` to put north back at the top');
    expect(text).toContain('`Your floor`, `One floor up and down`, or `Every floor`');
    expect(text).toContain('Turn on `Terrain sprites`');
    expect(text).toContain('Vosh remembers the style, the zoom, the 3D view, and the tileset.');
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
      '- Pick `Room` in `Match` to match only the armies, things and people a room lists after its exits line. The game sends `Room.Chars` and `Room.Items` packets with each look, and Vosh counts the lines from them, so a say or an arrival after the look stays a plain line.',
    );
  });

  it('says what Your target matches and how Vosh finds that line', () => {
    expect(body('automate.first-trigger')).toContain(
      '- Pick `Your target` in `Match` to match only the line of the one you target with `tar`, when a room lists them. Vosh finds that line by where your target stands in the room, the place `tar` marks with `>`, so `tar 3` finds the third person even when their line words the name another way. When more than one person in the room fits what you gave `tar`, the first of them is your target, so one line matches.',
    );
  });

  it('tells you where your target turns red, in the help on targets too', () => {
    const found = HELP_TOPICS.find((t) => t.id === 'tick.track-target');
    if (!found) throw new Error('no target topic');
    expect(found.body).toContain(
      '- Look at the room. With the `Room, time and weather colors` preset on, the line of your target turns bright red while the room lists them.',
    );
    expect(helpMd).toContain(`### ${found.number} ${found.title}\n\n${found.body}\n`);
  });

  it('matches HELP.md word for word', () => {
    const found = HELP_TOPICS.find((t) => t.id === 'automate.first-trigger');
    if (!found) throw new Error('no trigger topic');
    expect(helpMd).toContain(`### ${found.number} ${found.title}\n\n${found.body}\n`);
  });
});

describe('the help on the Room, time and weather colors preset', () => {
  it('names each color and says it follows your theme', () => {
    const text = body('automate.highlight-lines');
    expect(text).toContain(
      'The `Room, time and weather colors` preset colors a room look, the clock and the weather.',
    );
    expect(text).toContain(
      'The exits line turns green, the armies, things and people the room lists turn yellow, the day and night messages turn blue, and the WiZNET tag turns bold magenta.',
    );
    expect(text).toContain('Each one is a terminal color from your theme');
    expect(text).toContain(
      'The exits, room and target colors fill only the text the game left uncolored, so an aura, a red `[AFK]` and the red `+` of a trap you see keep their own colors.',
    );
    expect(text).toContain(
      'The one you target with `tar` turns bright red when the room lists them, so your target stands out from the rest of the room. That red is the `room.target` trigger, so give it a group in Triggers and turn the group off to keep your target yellow.',
    );
    expect(text).toContain(
      'The magenta covers the WiZNET tag alone, so the message after it keeps its colors too.',
    );
    expect(text).toContain(
      'A plain highlight restyles the matched words, and the rest of the line keeps the colors the game sent.',
    );
  });

  it('names the weather blue and says Vosh keeps it readable', () => {
    const text = body('automate.highlight-lines');
    expect(text).toContain(
      'A change in the weather, such as `It starts to rain.` or `A thick fog rolls in, shrouding the area.`, turns pale blue.',
    );
    expect(text).toContain(
      'That blue is `#8fa7d9`, a color of its own that stays apart from the blue and cyan of your theme. It holds on every built in dark theme, and `Keep highlight colors readable` darkens it on a light theme until it reads.',
    );
  });

  it('matches HELP.md word for word', () => {
    const found = HELP_TOPICS.find((t) => t.id === 'automate.highlight-lines');
    if (!found) throw new Error('no highlight topic');
    expect(helpMd).toContain(`### ${found.number} ${found.title}\n\n${found.body}\n`);
  });
});

describe('the help on Mark your commands', () => {
  it('says how your commands echo and how to turn the caret off', () => {
    expect(body('play.send-commands')).toContain(
      'Each command you send echoes in the text after a grey `›`, so your commands stand apart from the lines the game sends. Turn off `Mark your commands` under Input, then Command line, in Settings, to echo them bare.',
    );
    expect(body('play.send-commands')).toContain(
      'Right after a prompt that ends in `>`, such as `Account name>` at login, a command echoes without the `›`, since the prompt marks it already.',
    );
    expect(body('make-it-yours.control-terminal-colors')).toContain(
      'recolors the local echo of every command you send, and the `›` before it stays grey.',
    );
  });

  it('matches HELP.md word for word', () => {
    for (const id of ['play.send-commands', 'make-it-yours.control-terminal-colors']) {
      const found = HELP_TOPICS.find((t) => t.id === id);
      if (!found) throw new Error(`no help topic ${id}`);
      expect(helpMd).toContain(`### ${found.number} ${found.title}\n\n${found.body}\n`);
    }
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

describe('the help code block', () => {
  it('reads a fenced block whole, blank lines and all', () => {
    expect(parseHelpBody('Say it.\n\n```lua\nlocal a = 1\n\nend\n```\n\n- after')).toEqual([
      { kind: 'paragraph', text: 'Say it.' },
      { kind: 'code', lang: 'lua', text: 'local a = 1\n\nend' },
      { kind: 'list', items: ['after'] },
    ]);
  });
});

describe('the help on Lua panes', () => {
  it('shows the weather pane script and every block mud.pane takes', () => {
    const blocks = parseHelpBody(body('automate.lua-panes'));
    expect(blocks.map((b) => b.kind)).toEqual(['paragraph', 'code', 'table']);
    const code = blocks[1];
    expect(code.kind === 'code' && code.text).toMatch(
      /^-- weather_pane\/main\.lua\nlocal pane = mud\.pane\("weather", "Weather"\)/,
    );
    const table = blocks[2];
    const calls = table.kind === 'table' ? table.rows.map((r) => r[0]) : [];
    expect(calls).toEqual([
      '`mud.pane(id, title)`',
      '`pane:set(blocks)`',
      '`{ row = { label, value } }`',
      '`{ gauge = { label, value, max } }`',
      '`{ line = text }`',
      '`{ rule = true }`',
      '`pane:meta(text)`',
    ]);
  });

  it('points from Script Vosh with Lua to the new topic', () => {
    expect(body('automate.lua-scripts')).toContain(
      'A plugin can draw a pane of its own with `mud.pane`, as Make a pane with Lua at 3.10 shows.',
    );
  });
});

describe('the help on folding groups in Automation', () => {
  it('says how a heading folds its group and what the list remembers', () => {
    const text = body('automate.first-alias');
    expect(text).toContain(
      'Triggers, Aliases, Macros, and Timers each list your items under a heading for every group, and Presets under a heading for each category.',
    );
    expect(text).toContain(
      'Click a heading to fold its group away, and click it again to open it.',
    );
    expect(text).toContain('a folded heading counts the items it holds');
    expect(text).toContain('`ArrowLeft` folds it and `ArrowRight` opens it');
    expect(text).toContain('Each list remembers the groups you fold.');
    expect(text).toContain(
      'Type in the filter and every folded group with a match opens until you clear it.',
    );
  });

  it('lists the keys with the other shortcuts', () => {
    expect(body('reference.keyboard-shortcuts')).toContain(
      'In an Automation list in Settings. `ArrowUp` and `ArrowDown` move through the group headings and items',
    );
  });

  it('keeps colons and semicolons out of the prose', () => {
    const paragraph =
      body('automate.first-alias')
        .split('\n\n')
        .find((p) => p.startsWith('Triggers, Aliases, Macros, and Timers')) ?? '';
    expect(paragraph).not.toBe('');
    expect(paragraph.replace(/`[^`]*`/g, '')).not.toMatch(/[:;–—]| - /);
  });
});

describe('the help on Lua', () => {
  it('says what the limits stop and what a stop leaves', () => {
    const text = body('automate.lua-scripts');
    expect(text).toContain(
      'It stops a call that runs past 100 ms, uses 32 MB more than it began with, or takes your scripts past 128 MB in all, and `pcall` cannot catch the stop.',
    );
    expect(text).toContain('A stopped call sends nothing it queued');
    expect(text).toContain(
      'A plugin then stays off until you save it under Scripts in Settings or restart Vosh, a script from `#script load` until `#script reload`, and a trigger or alias whose Lua ran away until you save it or restart Vosh.',
    );
    expect(text).toContain('The page of a plugin Vosh stopped says why above its editor.');
    expect(text).toContain('One call may queue 100 actions');
    expect(text).toContain(
      'The time limit reaches inside string patterns and the `table` functions too',
    );
    expect(text).toContain(
      'Vosh runs 100 `mud.input` lines at most for one game line, packet, timer, or line you type',
    );
  });

  it('says what the budget for one line, packet, replay or round of timers does', () => {
    const text = body('automate.lua-scripts');
    expect(text).toContain(
      'Each plugin and each script from `#script load` also gets 100 ms in all for one game line, one packet, the last packets its new handlers get, or one round of timers that fall due together.',
    );
    expect(text).toContain(
      'Once it has used them, Vosh skips the rest of its triggers and handlers for that line or packet, holds the rest of its timers a quarter second, and says so in a red `[lua]` line.',
    );
  });

  it('says what the sandbox takes away', () => {
    const text = body('automate.lua-scripts');
    expect(text).toContain(
      '`require`, `io`, `os.execute`, `os.getenv`, and `os.setlocale` are gone, and Vosh refuses a `__gc` method',
    );
    expect(text).toContain('`#script load` reads only from the `scripts` folder');
    expect(text).toContain(
      '`mud.input` cannot run `#script load`, `#script reload`, `#import-tintin`, or `#profile`, which run only when you type them, and it cannot set a quick key or the tick command to a `#` command.',
    );
    expect(text).toContain('Every Lua error and every `print` shows in the terminal');
  });

  it('says a reload reads every file again and takes back what each script registered', () => {
    const text = body('automate.lua-scripts');
    expect(text).toContain(
      'Vosh reads every loaded script and plugin from disk again and runs them in the order they first loaded, and an error in one stops none after it.',
    );
    expect(text).toContain('`combat` and `combat.lua` load one script');
    expect(text).toContain(
      'Loading it again, with `#script reload` or `#script load`, takes all of them back once it runs without an error, so nothing doubles',
    );
    expect(text).toContain('Variables it set and groups it turned on or off stay.');
    expect(text).toContain(
      'A new `mud.on_gmcp` handler runs at once on the last packet of its package',
    );
    expect(text).toContain(
      'A new `Comm.Channel` handler waits for the next message instead, since each chat packet is one message and not a state.',
    );
  });

  it('says what a plugin keeps to itself and how your Lua reaches it', () => {
    const text = body('automate.lua-scripts');
    expect(text).toContain('Each plugin runs in its own environment.');
    expect(text).toContain('a line it hands `mud.input` runs no `#` command but `#echo`');
    expect(text).toContain(
      'it reads the standard libraries such as `string` and `table` but cannot change them',
    );
    expect(text).toContain(
      'An alias a plugin makes lasts while the plugin runs, and Vosh never saves it.',
    );
    expect(text).toContain(
      'They reach the globals of a plugin through `plugins.<name>`, a view you can read but not change',
    );
    expect(text).toContain(
      'When you switch profiles, the plugins the next profile lists turn on and the others turn off as you play',
    );
  });

  it('turns plugins on under Scripts and runs Lua in its Console', () => {
    const text = body('automate.lua-scripts');
    expect(text).toContain('Open Settings and choose Scripts to see your plugins.');
    expect(text).toContain(
      'The switch on each row turns a plugin on or off for that profile, and the plugin starts or stops at once in each session that plays it.',
    );
    expect(text).toContain('A plugin Vosh stopped reads `Stopped` there.');
    expect(text).toContain(
      'A plugin folder you named by hand with other characters, like `weather-pane`, still loads and shows there, and its row asks you to rename the folder. Until you do, its switch only turns it off, and its page does not open.',
    );
    expect(text).toContain(
      'The Console under Scripts in Settings shows the same lines for the session in front, each with its time, and runs the Lua you type in its field in that session the way `#lua` does.',
    );
    // You no longer edit the profile file to turn a plugin on (Q29).
    expect(text).not.toContain('while Vosh is closed');
    expect(text).not.toContain('`[plugins]`');
  });

  it('makes a plugin with New plugin and edits it on its own page', () => {
    const text = body('automate.lua-scripts');
    expect(text).toContain(
      '`New plugin` asks for a name of letters, digits and underscores, makes a folder of that name in `plugins` under the app data directory with a `manifest.toml` and a `main.lua`, turns the plugin on for the profile of the session in front and opens its page.',
    );
    expect(text).toContain(
      '`Save and reload` writes both to the plugin folder and loads the plugin again at once in every session whose profile turns it on',
    );
    expect(text).toContain(
      '`Show in Finder` under `Manifest` opens the plugin folder, and reads `Show in Explorer` on Windows and `Show the folder` on Linux.',
    );
    expect(text).toContain(
      "A plugin's page shows the lines of that plugin under `Output`, and its field runs Lua inside the plugin",
    );
    // A plugin folder comes from New plugin now, not by hand.
    expect(text).not.toContain('`plugins/<slug>/`');
  });

  it('installs, exports and removes a plugin from the Scripts list', () => {
    const text = body('automate.lua-scripts');
    expect(text).toContain(
      '`Install…` takes a `.zip` a friend shared, and you can drop a plugin folder or a `.zip` on the Scripts list instead.',
    );
    expect(text).toContain('An install starts off for every profile.');
    expect(text).toContain(
      '`Export to Downloads` saves the plugin as a `.zip` in your Downloads folder for you to share',
    );
    expect(text).toContain('`Reload` reads the plugin from its folder again');
    expect(text).toContain('`Show in Finder` opens its folder.');
    expect(text).toContain(
      '`Remove…` asks first, then deletes the plugin folder and turns the plugin off in every profile.',
    );
  });

  it('matches HELP.md word for word', () => {
    const found = HELP_TOPICS.find((t) => t.id === 'automate.lua-scripts');
    if (!found) throw new Error('no Lua topic');
    expect(helpMd).toContain(`### ${found.number} ${found.title}\n\n${found.body}\n`);
  });

  it('keeps colons and semicolons out of the new prose', () => {
    for (const start of [
      'Each script and each plugin',
      'Loads from `#script load`',
      'Press a plugin under Scripts',
      'Each plugin row has a menu',
      '`Install…` takes',
      'Each plugin runs',
      'Every Lua error',
      'Lua runs between',
      'The sandbox strips',
    ]) {
      const paragraph =
        body('automate.lua-scripts')
          .split('\n\n')
          .find((p) => p.startsWith(start)) ?? '';
      expect(paragraph).not.toBe('');
      expect(paragraph.replace(/`[^`]*`/g, '')).not.toMatch(/[:;–—]| - /);
    }
  });
});

describe('the help on #walk', () => {
  const topic = () => {
    const found = HELP_TOPICS.find((t) => t.id === 'play.walk');
    if (!found) throw new Error('no walk topic');
    return found;
  };

  it('is Walk to a place, topic 2.9 under Play', () => {
    const { number, title, section } = topic();
    expect([number, title, section]).toEqual(['2.9', 'Walk to a place', 'Play']);
  });

  it('teaches the steps, the stops and walking from an alias', () => {
    const text = topic().body;
    expect(text).toContain('- Type `#walk 3n2e` to go north three times, then east twice.');
    expect(text).toContain(
      '- Put a count from 1 to 99 before a direction to repeat it. Spaces between parts are fine.',
    );
    expect(text).toContain(
      'Press `Esc` or type `#walk stop` to stop it yourself. Other `#` commands leave it going.',
    );
    expect(text).toContain(
      'Commands after `#walk` in the same alias wait until you arrive, and drop if the walk stops early.',
    );
    const table = parseHelpBody(text).find((b) => b.kind === 'table');
    expect(table?.kind === 'table' ? table.rows.map((r) => r[0]) : []).toEqual([
      '`#walk 3n2e`',
      '`#walk 2w u`',
      '`#walk ne`',
      '`#walk 3x`',
      '`#walk`',
    ]);
  });

  it('leaves click to walk out until the map offers it', () => {
    expect(topic().body).not.toContain('Click a room');
  });

  it('lists #walk with the slash commands, and Esc among the keys', () => {
    expect(body('reference.slash-commands')).toContain(
      '- `#walk <steps>` walks a string of directions like `3n2e` one room at a time, `#walk` says how many steps are left, and `#walk stop` stops the walk.',
    );
    expect(body('reference.keyboard-shortcuts')).toContain(
      '- `Escape` cancels an in flight paste burst, stops a walk, closes the scrollback split, and snaps the terminal to its tail.',
    );
  });

  it('matches HELP.md word for word', () => {
    for (const id of ['play.walk', 'reference.slash-commands', 'reference.keyboard-shortcuts']) {
      const found = HELP_TOPICS.find((t) => t.id === id);
      if (!found) throw new Error(`no help topic ${id}`);
      expect(helpMd).toContain(`### ${found.number} ${found.title}\n\n${found.body}\n`);
    }
  });
});

describe('the help on group switches and timer groups', () => {
  const switches =
    'The switch after a group heading turns the whole group on and off at once, the same as `#group`, and each item keeps its own `Enabled`.';

  it('says what the switch on a heading does, and when it waits', () => {
    const text = body('automate.first-alias');
    expect(text).toContain(switches);
    expect(text).toContain('It acts as you flip it, with no `Save`');
    expect(text).toContain('a group you just named gets its switch once you save it');
    expect(text).toContain('`Tab` from a heading reaches its switch, and `Space` flips it.');
    expect(text).toContain(
      'In loadout mode, while an active loadout lists groups or while you keep the catalog dormant, the loadouts decide each group of triggers, aliases, and macros.',
    );
    expect(text).toContain(
      'Its switch waits, and a note under the heading names the loadouts that decide it, or says every loadout is off.',
    );
    expect(text).toContain(
      '`#group` still turns such a group, and the note then says when the loadouts turn it back.',
    );
    expect(text).toContain(
      'with the switch on the heading of their group or with `#group <name> on|off`',
    );
    const paragraph = text.split('\n\n').find((p) => p.startsWith('The switch after')) ?? '';
    expect(paragraph.replace(/`[^`]*`/g, '')).not.toMatch(/[:;–—]| - /);
  });

  it('says timers take groups, and that no loadout turns them', () => {
    expect(body('automate.first-alias')).toContain(
      'A timer takes a `Group` too, and a timer in a group that is off waits, then starts a whole interval once the group comes back on.',
    );
    expect(body('automate.macros')).toContain(
      'along with matching alias, trigger, and timer groups.',
    );
    expect(body('characters-and-data.loadouts')).toContain(
      'Timers stay with each profile, so no loadout turns a timer group on or off.',
    );
    expect(body('reference.slash-commands')).toContain(
      '`#group <name> on|off` turns a group of triggers, aliases, macros, and timers on or off',
    );
    expect(body('reference.keyboard-shortcuts')).toContain(
      '`Tab` from a heading reaches its group switch, and `Space` flips it.',
    );
  });

  it('says a dormant catalog holds every switch too', () => {
    const loadouts = body('characters-and-data.loadouts');
    expect(loadouts).toContain(
      'When no active loadout declares any enabled groups, the loadouts impose nothing and each group stays on or off as you left it, unless you keep the catalog dormant.',
    );
    expect(loadouts).toContain(
      'While they impose, and while the catalog is dormant, the switch on each catalog group in Automation waits, with a note that names the loadouts that decide it or says every loadout is off.',
    );
    const paragraph =
      loadouts.split('\n\n').find((p) => p.startsWith('When no active loadout')) ?? '';
    expect(paragraph).not.toBe('');
    expect(paragraph.replace(/`[^`]*`/g, '')).not.toMatch(/[:;–—]| - /);
  });

  it('reads the same in HELP.md', () => {
    for (const id of [
      'automate.first-alias',
      'automate.macros',
      'characters-and-data.loadouts',
      'reference.slash-commands',
      'reference.keyboard-shortcuts',
    ]) {
      const found = HELP_TOPICS.find((t) => t.id === id);
      if (!found) throw new Error(`no help topic ${id}`);
      expect(helpMd).toContain(`### ${found.number} ${found.title}\n\n${found.body}\n`);
    }
  });
});

describe('the help on auto reconnect and Lua alerts', () => {
  it('says when Vosh dials again and when it never does', () => {
    const text = body('get-connected.reconnect');
    expect(text).toContain('then 6, 12, 24, 48 and 60 seconds after each try before, 8 tries');
    expect(text).toContain('Vosh never dials again after your `Disconnect`, a `quit` you typed');
    expect(text).toContain('add `reconnect = false` to its profile file');
  });

  it('says what a Mac waits for and that only macOS takes banners back', () => {
    const text = body('automate.lua-scripts');
    expect(text).toContain('on macOS turning a plugin off takes back the banners it posted');
    expect(text).toContain(
      'Vosh asks for that the first time you turn on a `Banner` in Settings, as Get alerts at 3.9 shows.',
    );
    expect(text).not.toContain('yet to land');
    expect(text).toContain(
      'It also needs a signed Vosh, so a dev build you run from the source shows none there.',
    );
  });

  it('says how Vosh asks to post banners and how to turn them back on', () => {
    const text = body('automate.alerts');
    expect(text).toContain('Vosh asks before macOS does');
    expect(text).toContain(
      '`Not now` keeps `Banner` on and asks no more until you close Settings.',
    );
    expect(text).toContain('`Banner` wears a warning ring on every `Alert` row');
    expect(text).toContain('`Open notification settings`');
    expect(text).toContain('A dev build you run from the source shows no banners');
  });

  it('reads the same in HELP.md', () => {
    for (const id of ['get-connected.reconnect', 'automate.lua-scripts', 'automate.alerts']) {
      const found = HELP_TOPICS.find((t) => t.id === id);
      if (!found) throw new Error(`no help topic ${id}`);
      expect(helpMd).toContain(`### ${found.number} ${found.title}\n\n${found.body}\n`);
    }
  });
});

describe('the help on Color vision', () => {
  const id = 'make-it-yours.control-terminal-colors';

  it('says Color vision swaps the colors your eyes confuse', () => {
    const text = body(id);
    expect(text).toContain(
      '`Color vision` swaps the colors your eyes confuse for colors they tell apart, the way color blind modes in games do.',
    );
    expect(text).toContain('Deuteranopia and Protanopia turn tells in green blue');
    expect(text).toContain(
      'Reds lean toward orange and blues toward violet where the theme allows.',
    );
    expect(text).toContain('Tritanopia turns blues purple and magentas pink');
    expect(text).toContain('with Fit game colors on or off, Solarized Dark included.');
    expect(text).toContain(
      'Solarized Dark stays as published in play too, since its soft text is what the scheme is, until you pick a Color vision other than Typical.',
    );
  });

  // The swap keeps every two channels apart, so a color keeps its own
  // hue where turning it would run it into another, and the window moves
  // its status colors where a tritanope sees them near.
  it('says where a color keeps its hue and what the window does under tritanopia', () => {
    const text = body(id);
    expect(text).toContain(
      'Two channels you told apart never run together for your vision, newbie chat and immortal talk included.',
    );
    expect(text).toContain('such as Tokyo Night, tells keep their green.');
    expect(text).toContain('makes them lighter or darker where danger sits near warn or success');
    for (const old of ['run together for colors', 'which you already tell apart', 'reds orange']) {
      expect(text, old).not.toContain(old);
    }
  });

  // The fit before the swap turned red and green only as far as they
  // kept their names.
  it('drops the hue limits of the fit before the swap', () => {
    for (const old of ['orange red', 'aquamarine', 'dodger blue', 'toward teal', '30 degrees']) {
      expect(body(id), old).not.toContain(old);
      expect(helpMd, old).not.toContain(old);
    }
  });

  it('matches HELP.md word for word', () => {
    const topic = HELP_TOPICS.find((t) => t.id === id);
    if (!topic) throw new Error(`no help topic ${id}`);
    expect(helpMd).toContain(`### ${topic.number} ${topic.title}\n\n${topic.body}\n`);
  });
});

// Sessions Q5, Q22, Q24 to Q26, Q28, Q30 and Q31. What each session keeps
// apart and what the sessions on one profile share, in topic 1.4 and in
// each topic that described one session.
describe('the help on what each session keeps and what its profile shares', () => {
  it('lists in 1.4 what a session keeps and what its profile shares', () => {
    const text = body('get-connected.sessions');
    expect(text).toContain('Each session keeps these of its own.');
    expect(text).toContain(
      '- Its Lua, with the plugins its profile turns on, the scripts you load with `#script load` and the aliases its plugins make. When Vosh stops the Lua of a trigger or an alias, it stays off in that session alone.',
    );
    expect(text).toContain(
      'The sessions on one profile share everything the profile holds, its aliases, triggers, macros and timers, its groups, its profile variables, its tick settings, its prompt design, its loadouts and its panes.',
    );
    expect(text).toContain('`#tick reset` restarts the count of its own session alone.');
    expect(text).toContain(
      'When you open Vosh again, your sessions come back in their order with their names, none of them connected',
    );
  });

  it('says what a profile load or reset reaches and what the other sessions print', () => {
    expect(body('get-connected.profile-save')).toContain(
      'Both reach every session that plays the profile, and each of the others prints a line that names the session you typed it in, such as `Tolliver loaded this profile from its file.`',
    );
    expect(body('reference.slash-commands')).toContain(
      'a load or a reset reaches every session on the profile',
    );
  });

  it('says the tick settings belong to the profile and each count to its session', () => {
    const text = body('tick.tick-timer');
    expect(text).toContain(
      'While another session on the profile is connected, a new connection keeps the switch as that session has it.',
    );
    expect(text).toContain(
      'Each session keeps its own count, and `#tick reset` restarts only the count of the session you type it in.',
    );
    expect(body('tick.track-target')).toContain(
      'Each session keeps its own target and its own quick keys.',
    );
  });

  it('says where variables, groups, the recorder and the Lua stops live', () => {
    expect(body('automate.variables')).toContain(
      '`#unvar` takes the name out of both scopes, so the profile value goes for every session on the profile.',
    );
    expect(body('automate.first-alias')).toContain(
      'A group is on or off for its whole profile, so the switch and `#group` reach every session that plays the profile.',
    );
    expect(body('automate.macros')).toContain('It captures the commands you type in its session');
    expect(body('automate.lua-scripts')).toContain(
      'Each stays off only in the session where Vosh stopped it, and every other session on the profile keeps running it.',
    );
  });

  it('says each profile keeps its loadouts and each session its scrollback file', () => {
    expect(body('characters-and-data.loadouts')).toContain(
      'Which loadouts are on belongs to the profile.',
    );
    expect(body('fix-it.data-on-disk')).toContain(
      'each later session keeps its own in a file with its number, such as `scrollback-2.txt`. Closing a session deletes its file.',
    );
    expect(body('characters-and-data.profiles')).toContain(
      "`Default plays in Tolliver's session, Build in Orla's.`",
    );
  });

  it('matches HELP.md word for word', () => {
    for (const id of [
      'get-connected.profile-save',
      'get-connected.sessions',
      'automate.first-alias',
      'automate.variables',
      'automate.macros',
      'automate.lua-scripts',
      'tick.tick-timer',
      'tick.track-target',
      'characters-and-data.profiles',
      'characters-and-data.loadouts',
      'fix-it.data-on-disk',
      'reference.slash-commands',
    ]) {
      const topic = HELP_TOPICS.find((t) => t.id === id);
      if (!topic) throw new Error(`no help topic ${id}`);
      expect(helpMd, id).toContain(`### ${topic.number} ${topic.title}\n\n${topic.body}\n`);
    }
  });
});

describe('the help on importing a profile', () => {
  // Board 5 of the Scripts design, Scripts Q9 and Q10.
  it('imports a profile under Characters as a new one or over one you have', () => {
    const text = body('characters-and-data.profiles');
    expect(text).toContain(
      'To bring in a profile, click `Import…` beside `New profile` and pick a Vosh profile export.',
    );
    expect(text).toContain(
      '`Replace a profile` lays it over the profile you pick, which keeps its own world and characters.',
    );
    expect(text).toContain(
      'Plugins the file turns on come in off, so you turn each one on under Scripts.',
    );
    expect(text).toContain('Vosh names each one under a warning');
  });

  it('says where loadout mode puts the items, and that a clash keeps yours', () => {
    // Scripts Q26 and the loadout mode note of board 5.
    const text = body('characters-and-data.profiles');
    expect(text).toContain(
      'In loadout mode the triggers, aliases and macros in the file join the shared catalog in a group named after the file, like `Healer profile`, and never the profile file.',
    );
    expect(text).toContain(
      'When the catalog already has one of the same name, or a macro of yours on the same key, yours stays, and the line under the list says so.',
    );
    // A preset macro waits for yours on its key, and the presets on in
    // loadout mode add their own, so the file's stay out (B2 chunk 3).
    expect(text).toContain(
      'The macros a preset added in the file stay out, since the presets you turn on in loadout mode add their own.',
    );
  });

  it('says how characters come with the file, and that an export names them only by choice', () => {
    const text = body('characters-and-data.profiles');
    expect(text).toContain('A character no other profile has starts on and joins the new profile.');
    expect(text).toContain('A new profile with no character starts with its login off.');
    expect(text).toContain(
      'Each starts off, so a profile you share names your characters only when you turn them on.',
    );
  });

  it('sends a Vosh export from the importers to Characters', () => {
    expect(body('characters-and-data.tintin-import')).toMatch(
      /A Vosh profile export goes in under Characters, with `Import…` beside `New profile`\.$/,
    );
  });

  it('keeps colons and semicolons out of the new prose', () => {
    const paragraphs = body('characters-and-data.profiles').split('\n\n');
    for (const start of [
      '`Export to Downloads`',
      'To bring in',
      'In loadout mode',
      'Plugins the file',
      'A new profile',
    ]) {
      const paragraph = paragraphs.find((p) => p.startsWith(start)) ?? '';
      expect(paragraph, start).not.toBe('');
      expect(paragraph, start).not.toMatch(/[;:] /);
    }
  });
});

describe('the help on Numpad movement', () => {
  // Scripts board 7, Q12 and Q13.
  it('names each key and what it sends, and says a key of yours stays yours', () => {
    const text = body('automate.macros');
    expect(text).toContain(
      '`Numpad8` sends `n`, `Numpad6` sends `e`, `Numpad2` sends `s`, `Numpad4` sends `w`, `Numpad9` sends `u`, and `Numpad3` sends `d`.',
    );
    expect(text).toContain('`Numpad7`, `Numpad1` and `Numpad5` stay free.');
    // The keys go by event.code (src/automation/macroKeys.ts).
    expect(text).toContain(
      'Vosh reads the key itself, so NumLock does not matter and the digit row still types.',
    );
    // The Macros list of board 7 (B2 chunk 5).
    expect(text).toContain(
      'It adds six macros under `From presets` in Macros, where only their group changes.',
    );
    expect(text).toContain(
      "A key one of your macros uses stays yours, and the preset's macro on it waits.",
    );
    expect(text).toContain('The direction takes the key once you move or delete your macro.');
    expect(text).toContain('Turning the preset off removes its six and none of yours.');
  });

  it('names the six keys among the shortcuts', () => {
    expect(body('reference.keyboard-shortcuts')).toContain(
      'While the `Numpad movement` preset is on, `Numpad8`, `Numpad6`, `Numpad2` and `Numpad4` walk north, east, south and west, and `Numpad9` and `Numpad3` go up and down.',
    );
  });

  it('matches HELP.md word for word', () => {
    const found = HELP_TOPICS.find((t) => t.id === 'automate.macros');
    if (!found) throw new Error('no macros topic');
    expect(helpMd).toContain(`### ${found.number} ${found.title}\n\n${found.body}\n`);
  });
});

describe('the help on the alert presets', () => {
  it('names each preset as the Alerts category lists it', () => {
    const text = body('automate.alerts');
    for (const preset of ALERT_PRESETS) expect(text).toContain(`\`${preset.name}\``);
    expect(text).toContain('All five start off.');
    expect(text).toContain('at most once in 10 seconds');
  });

  it('points there from Reconnect and Create a trigger', () => {
    expect(body('get-connected.reconnect')).toContain('in Get alerts at 3.9');
    expect(body('automate.first-trigger')).toContain('as Get alerts at 3.9 shows');
  });
});
