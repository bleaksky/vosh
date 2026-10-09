import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { HELP_SECTIONS, HELP_TOPICS, type HelpTopic } from './helpContent';
import { helpSearchKey, rankTopics } from './helpNav';
import { HelpSidebar } from './HelpSidebar';
import { HELP_SECTION_ICONS } from './sectionIcons';

function topic(id: string): HelpTopic {
  const found = HELP_TOPICS.find((t) => t.id === id);
  if (!found) throw new Error(`no help topic ${id}`);
  return found;
}

function draw(query: string, shown = topic('reference.prompt-codes')): string {
  return renderToStaticMarkup(
    <HelpSidebar
      topic={shown}
      openSection={shown.section}
      onToggleSection={() => undefined}
      onOpenTopic={() => undefined}
      query={query}
      onQuery={() => undefined}
      results={rankTopics(query)}
      active={0}
      onActive={() => undefined}
      onStep={() => undefined}
      inputRef={{ current: null }}
      mac
    />,
  );
}

describe('the help sidebar', () => {
  it('lists the sections, the open one with its topics and the one you read', () => {
    const html = draw('');
    expect(html).toContain('aria-label="Help topics"');
    expect(html.match(/class="st-nav-item hp-section"/g)).toHaveLength(HELP_SECTIONS.length);
    expect(html.match(/aria-expanded="true"/g)).toHaveLength(1);
    expect(html).toMatch(/aria-current="page"[^>]*><span class="st-nav-label">Prompt design codes/);
    expect(html.match(/class="st-nav-item hp-topic"/g)).toHaveLength(3);
    // The Cmd+F keycaps sit in the empty search.
    expect(html).toContain('<kbd class="keycap is-glyph">⌘</kbd>');
    // The search names its keys apart, and the caps stay out of its name.
    expect(html).toMatch(/role="combobox"[^>]*aria-keyshortcuts="Meta\+F"/);
    expect(html).toContain('<span class="keys st-search-keys" aria-hidden="true">');
  });

  it('swaps the nav for the results while the search holds words', () => {
    const html = draw('prompt');
    expect(html).not.toContain('aria-label="Help topics"');
    expect(html).toContain('role="listbox"');
    expect(html.match(/role="option"/g)).toHaveLength(rankTopics('prompt').length);
    expect(html).toMatch(
      /aria-selected="true"[^>]*>.*?<span class="st-nav-label">Choose where your prompt shows<\/span><span[^>]*>, in Shape the window<\/span>/,
    );
    // The keycaps leave while the search holds text.
    expect(html).not.toContain('keycap');
  });

  it('says so when nothing matches', () => {
    expect(draw('zzyzx')).toContain('No help matches.');
  });

  it('is a landmark named Sidebar', () => {
    expect(draw('')).toMatch(/^<aside class="st-sidebar" aria-label="Sidebar">/);
  });

  it('gives every section an icon', () => {
    for (const section of HELP_SECTIONS) expect(HELP_SECTION_ICONS[section], section).toBeDefined();
  });
});

describe('a key in the help search', () => {
  const state = { active: 1, results: 4, query: 'prompt' };

  it('moves through the results with Up and Down', () => {
    expect(helpSearchKey('ArrowDown', false, state)).toEqual({ kind: 'active', index: 2 });
    expect(helpSearchKey('ArrowUp', false, state)).toEqual({ kind: 'active', index: 0 });
    expect(helpSearchKey('ArrowDown', false, { ...state, active: 3 })).toEqual({
      kind: 'active',
      index: 3,
    });
  });

  it('steps through the matches with Enter, back with Shift', () => {
    expect(helpSearchKey('Enter', false, state)).toEqual({ kind: 'step', by: 1 });
    expect(helpSearchKey('Enter', true, state)).toEqual({ kind: 'step', by: -1 });
    expect(helpSearchKey('Enter', false, { ...state, results: 0 })).toBeNull();
  });

  it('clears the words with Escape, then leaves the field', () => {
    expect(helpSearchKey('Escape', false, state)).toEqual({ kind: 'clear' });
    expect(helpSearchKey('Escape', false, { ...state, query: '' })).toEqual({ kind: 'blur' });
    expect(helpSearchKey('a', false, state)).toBeNull();
  });
});
