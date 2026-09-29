import { describe, expect, it } from 'vitest';
import { HELP_TOPICS } from './helpContent';

function body(id: string): string {
  const topic = HELP_TOPICS.find((t) => t.id === id);
  if (!topic) throw new Error(`no help topic ${id}`);
  return topic.body;
}

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
  });

  it('says where the items live in loadout mode', () => {
    const text = body('fix-it.data-on-disk');
    expect(text).toContain(
      'In loadout mode the aliases, triggers, and macros live in `catalog.toml` instead',
    );
    expect(text).toContain('`profiles/legacy/`');
  });
});
