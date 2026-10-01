import { describe, expect, it } from 'vitest';
import { HELP_SECTIONS, HELP_TOPICS, type HelpTopic } from './helpContent';
import {
  countMatches,
  matchRanges,
  outlineFor,
  rankTopics,
  resolveHelpTarget,
  sectionTopics,
} from './helpNav';

function topic(key: string): HelpTopic {
  const found = HELP_TOPICS.find((t) => t.number === key || t.id === key);
  if (!found) throw new Error(`no help topic ${key}`);
  return found;
}

describe('the help search', () => {
  it('puts a title that matches first, then the most matches', () => {
    const ranked = rankTopics('prompt');
    const titled = ranked.filter((t) => t.title.toLowerCase().includes('prompt'));
    expect(ranked.slice(0, titled.length)).toEqual(titled);
    const rest = ranked.slice(titled.length);
    for (let i = 1; i < rest.length; i++) {
      expect(countMatches(rest[i - 1], 'prompt')).toBeGreaterThanOrEqual(
        countMatches(rest[i], 'prompt'),
      );
    }
    expect(ranked[0].id).toBe('shape.prompt-show');
    expect(ranked[1].id).toBe('reference.prompt-codes');
  });

  it('finds what the catalog search finds, and nothing for no words', () => {
    expect(rankTopics('prompt')).toHaveLength(
      HELP_TOPICS.filter((t) => `${t.title} ${t.body}`.toLowerCase().includes('prompt')).length,
    );
    expect(rankTopics('   ')).toEqual([]);
  });

  it('counts every match you would see, the title included', () => {
    const shown = topic('shape.prompt-show');
    const words = `${shown.title} ${shown.body.replace(/`/g, '')}`.toLowerCase();
    expect(countMatches(shown, 'prompt')).toBe(words.split('prompt').length - 1);
    expect(countMatches(shown, 'PROMPT')).toBe(countMatches(shown, 'prompt'));
  });

  it('marks matches that never overlap', () => {
    expect(matchRanges('aaaa', 'aa')).toEqual([
      [0, 2],
      [2, 4],
    ]);
    expect(matchRanges('Prompt prompt', 'prompt')).toEqual([
      [0, 6],
      [7, 13],
    ]);
    expect(matchRanges('text', '  ')).toEqual([]);
  });
});

describe('the outline of a topic', () => {
  it('lists every slash command, as many words as each needs', () => {
    const outline = outlineFor(topic('9.1'));
    expect(outline?.map((e) => e.label)).toEqual([
      '#help',
      '#alias',
      '#var',
      '#trigger',
      '#prompt game',
      '#prompt draw',
      '#prompt show',
      '#prompt default',
      '#group',
      '#tick',
      '#tick warn',
      '#script',
      '#lua',
      '#profile',
      '#import-tintin',
      '#logs',
      '#record',
      '#qkey',
      '#target',
      '#nativesurface',
    ]);
  });

  it('leaves prose and short lists without one', () => {
    for (const id of [
      'shape.prompt-show',
      'reference.prompt-codes',
      'tick.tick-timer',
      'get-connected.connect',
    ]) {
      expect(outlineFor(topic(id)), id).toBeNull();
    }
  });
});

describe('the help sections', () => {
  it('each hold their topics in catalog order', () => {
    expect(HELP_SECTIONS.flatMap((s) => sectionTopics(s))).toEqual(HELP_TOPICS);
  });
});

describe('a link into help', () => {
  it('lands on a topic by its id or its number', () => {
    expect(resolveHelpTarget('9.3')).toEqual({ kind: 'topic', topic: topic('9.3') });
    expect(resolveHelpTarget('reference.prompt-codes')).toEqual({
      kind: 'topic',
      topic: topic('9.3'),
    });
  });

  it('searches for anything else', () => {
    expect(resolveHelpTarget(' tick timer ')).toEqual({ kind: 'search', query: 'tick timer' });
    expect(resolveHelpTarget('  ')).toBeNull();
  });
});
