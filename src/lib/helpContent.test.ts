import { describe, expect, it } from 'vitest';
import helpMd from '../../HELP.md?raw';
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
    expect(text).toContain('Every other setting stays with its profile.');
    expect(text).not.toMatch(/run the migration wizard again/);
    expect(text).toContain('The migration wizard runs once');
    expect(text).toContain('while `profiles/legacy/` holds the copies from an earlier run');
    expect(text).toContain('or while an earlier run waits to finish at the next launch');
    // Moving the legacy folder out alone left the catalog to be built
    // from files without their items.
    expect(text).toContain(
      'copy the files in `profiles/legacy/` back over the ones in `profiles/`',
    );
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
