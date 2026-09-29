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
    // nothing removes those rows, so the help must not read as if none
    // exist.
    const older =
      'Older versions of Vosh saved those lines in full, so a session you logged before updating can still show your password after a `> `. To clear it, quit Vosh and delete `logs.sqlite` and its `-wal` and `-shm` files from the app data folder. That removes every saved session. If you copied or shared one of those sessions, change your password in the game.';
    expect(body('characters-and-data.search-logs')).toContain(older);
    expect(helpMd).toContain(older);
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
    expect(text).toContain('or while `profiles/legacy/` holds the copies from an earlier run');
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
