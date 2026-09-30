import { describe, expect, it } from 'vitest';
import helpMd from '../../HELP.md?raw';
import { parseRoutedLine } from './chatStore';
import { HELP_TOPICS } from './helpContent';

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

  it('reads the same in HELP.md', () => {
    const { number, title, body: text } = topic();
    expect(helpMd).toContain(`### ${number} ${title}\n\n${text}\n`);
  });

  it('sends you to Characters for tracked affects everywhere in HELP.md', () => {
    expect(helpMd).not.toMatch(/[Tt]racked affects[^.\n]*`panels` tab/);
    expect(helpMd).toContain(
      'Tracked affects live in Settings under Characters, then Tracked affects.',
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

describe('the help on the prompt capture move', () => {
  const paragraph =
    "Each profile reads your prompt on its own. Vosh moves the capture trigger that `#prompt` made into each profile that draws your own prompt, turns the trigger off, and tells you once at launch. Profiles that draw nothing then show the game's prompt. An older version of Vosh shows the game's prompt in every profile until you turn `prompt-capture` on again under Automation. Back in this version, Vosh moves the capture into your profiles again and turns the trigger off.";

  it('says what the move does and what an older version shows, in both places', () => {
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
    expect(text).toContain('Read a line as `[tell] Selune: meet at the bank`.');
    expect(text).toContain('Wrapped lines hang two cells in');
    expect(text).toContain("from your theme's terminal colors");
    expect(text).toContain('Point at a message to see when it arrived.');
    expect(text).not.toContain('tab per channel');
    expect(text).not.toContain('`visible/total`');
  });

  // The recipe the help gives for the tells you send, run against every
  // line the game prints when you talk to one person or your group
  // (languages.c compose_tell and compose_grouptell_open).
  describe('the trigger it gives for the tells you send', () => {
    const recipe = /A trigger on `([^`]+)` with a route to `tell`/.exec(body('shape.chat-pane'));
    const pattern = new RegExp(recipe?.[1] ?? '(?!)');
    const caught = (text: string) =>
      pattern.test(text) ? parseRoutedLine({ pane: 'tell', text }) : undefined;

    it('is in the help', () => {
      expect(recipe).not.toBeNull();
      expect(body('shape.chat-pane')).toContain('makes each one read `[tell] to Selune: text`.');
    });

    it('catches a tell you speak or project, in any language', () => {
      for (const text of [
        "You tell Selune 'omw'",
        "You tell a city guard in Tol'khan 'it is me'",
        "You project to Selune 'omw'",
        "You project to Selune in Elvish 'omw'",
      ]) {
        expect(caught(text), text).toMatchObject({ direction: 'sent', text: expect.any(String) });
      }
    });

    it('puts no line for a group tell in the pane', () => {
      for (const text of [
        "You tell your group 'one tick, waiting on mana'",
        "You tell your group in Elvish 'one tick'",
        "You broadcast 'one tick'",
      ]) {
        expect(caught(text) ?? null, text).toBeNull();
      }
    });

    it('leaves the tells you receive to their packet', () => {
      for (const text of [
        "Selune tells you 'are you still at the bank?'",
        "[Selune] 'omw'",
        'You project your image away from your body.',
      ]) {
        expect(pattern.test(text), text).toBe(false);
      }
    });

    it('says why the pane skips a group tell', () => {
      expect(body('shape.chat-pane')).toContain(
        'The pane skips the `You tell your group` line the trigger also catches, because your gtell already arrives over GMCP.',
      );
    });
  });

  it('matches HELP.md word for word', () => {
    const topic = HELP_TOPICS.find((t) => t.id === 'shape.chat-pane');
    if (!topic) throw new Error('no help topic shape.chat-pane');
    expect(helpMd).toContain(`### ${topic.number} ${topic.title}\n\n${topic.body}\n`);
  });
});
