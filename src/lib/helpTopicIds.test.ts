import { describe, expect, it, vi } from 'vitest';
import golden from '../../fixtures/links/help-topics.json';
import { HELP_TOPICS } from './helpContent';
import { helpOpensOn } from './helpLink';
import { landingOf, resolveHelpTarget } from './helpNav';

// Every link into Help names a topic by its id or its number: the book
// buttons in Settings, `#help` with an id or a number, and the topic
// Help opens on again from `vosh.help.topic`. A move that renames a
// topic, or reorders the catalog so a number changes, breaks those
// links with no error anywhere. The golden file lists every id and
// number in rail order, so any such change shows up in its diff.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({ emit: vi.fn(() => Promise.resolve()) }));

const pinned = Object.entries(golden.topics);

describe('help topic ids', () => {
  it('match the golden file, with their numbers, in rail order', () => {
    expect(HELP_TOPICS.map((t) => [t.id, t.number])).toEqual(pinned);
  });

  it('name one topic each, by id and by number', () => {
    const ids = HELP_TOPICS.map((t) => t.id);
    const numbers = HELP_TOPICS.map((t) => t.number);
    expect(new Set(ids).size).toBe(ids.length);
    expect(new Set(numbers).size).toBe(numbers.length);
  });

  it('open their own topic from a link, by id and by number', () => {
    for (const [id, number] of pinned) {
      for (const link of [id, number]) {
        const target = resolveHelpTarget(link);
        expect(target?.kind === 'topic' ? target.topic.id : null, link).toBe(id);
        if (target) expect(landingOf(target).topicId, link).toBe(id);
        expect(helpOpensOn(link), link).toBe(true);
      }
    }
  });
});
