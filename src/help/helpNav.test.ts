import { describe, expect, it } from 'vitest';
import { HELP_SECTIONS, HELP_TOPICS, type HelpTopic } from './helpContent';
import {
  countMatches,
  helpScrollKey,
  landingOf,
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

  it('reads a run of spaces in the words as one', () => {
    // Words typed or sent with #help may hold a doubled space.
    expect(rankTopics('tick  timer').length).toBeGreaterThan(0);
    expect(rankTopics('tick  timer')).toEqual(rankTopics('tick timer'));
    expect(rankTopics('tick \t timer')).toEqual(rankTopics('tick timer'));
    const shown = topic('reference.slash-commands');
    expect(countMatches(shown, 'tick  timer')).toBe(countMatches(shown, 'tick timer'));
    expect(matchRanges('the tick timer', 'tick  timer')).toEqual([[4, 14]]);
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
      '#lag',
      '#script',
      '#lua',
      '#profile',
      '#import-tintin',
      '#logs',
      '#record',
      '#qkey',
      '#target',
      '#walk',
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

  it('opens the section of the topic it lands on, with the search cleared', () => {
    // Every landing opens the section, even on the topic you read with
    // its section folded away.
    const shown = topic('shape.prompt-show');
    expect(landingOf({ kind: 'topic', topic: shown })).toEqual({
      topicId: shown.id,
      query: '',
      section: shown.section,
      focusSearch: false,
    });
  });

  it('opens the section of the best result for words, with the caret in the search', () => {
    const best = rankTopics('tick timer')[0];
    expect(landingOf({ kind: 'search', query: 'tick timer' })).toEqual({
      topicId: null,
      query: 'tick timer',
      section: best.section,
      focusSearch: true,
    });
    expect(landingOf({ kind: 'search', query: 'zzqx' }).section).toBeNull();
  });
});

describe('a key that scrolls the article', () => {
  it('pages from the search, where the other keys edit the words', () => {
    expect(helpScrollKey('PageDown', false, 'field')).toEqual({ kind: 'page', by: 1 });
    expect(helpScrollKey('PageUp', false, 'field')).toEqual({ kind: 'page', by: -1 });
    for (const key of ['ArrowDown', 'ArrowUp', 'Home', 'End', ' ']) {
      expect(helpScrollKey(key, false, 'field'), key).toBeNull();
    }
  });

  it('pages, steps and jumps from a sidebar control, and leaves Space to press it', () => {
    expect(helpScrollKey('PageDown', false, 'control')).toEqual({ kind: 'page', by: 1 });
    expect(helpScrollKey('ArrowDown', false, 'control')).toEqual({ kind: 'line', by: 1 });
    expect(helpScrollKey('ArrowUp', false, 'control')).toEqual({ kind: 'line', by: -1 });
    expect(helpScrollKey('Home', false, 'control')).toEqual({ kind: 'edge', to: 'top' });
    expect(helpScrollKey('End', false, 'control')).toEqual({ kind: 'edge', to: 'bottom' });
    expect(helpScrollKey(' ', false, 'control')).toBeNull();
  });

  it('takes Space too when nothing has focus', () => {
    expect(helpScrollKey(' ', false, 'none')).toEqual({ kind: 'page', by: 1 });
    expect(helpScrollKey(' ', true, 'none')).toEqual({ kind: 'page', by: -1 });
    expect(helpScrollKey('End', false, 'none')).toEqual({ kind: 'edge', to: 'bottom' });
  });

  it('leaves every key to the article once it has focus', () => {
    for (const key of ['PageDown', 'PageUp', 'ArrowDown', 'Home', 'End', ' ']) {
      expect(helpScrollKey(key, false, 'article'), key).toBeNull();
    }
    expect(helpScrollKey('a', false, 'none')).toBeNull();
  });
});
