import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { buildSections, type ListEntry } from '../../../../lib/automationList';
import { ItemList } from './ItemList';
import { HIDES_PROMPT_NOTE } from './TriggersEditor';

const entry = (uid: string, name: string, enabled: boolean): ListEntry => ({
  uid,
  name,
  group: '',
  enabled,
  text: name,
});

function render(warnNames?: ReadonlySet<string>, warnNote?: string): string {
  return renderToStaticMarkup(
    <ItemList
      noun={{ one: 'trigger', many: 'triggers' }}
      filterLabel="Filter triggers"
      filter=""
      onFilter={() => {}}
      sections={buildSections([
        entry('a', 'my-capture', true),
        entry('b', 'off-capture', false),
        entry('c', 'flee', true),
      ])}
      hasItems
      emptyText=""
      pinned={null}
      selected="a"
      onSelect={() => {}}
      revealSeq={0}
      monoName={false}
      monoMeta={false}
      warnNames={warnNames}
      {...(warnNote ? { warnNote } : {})}
    />,
  );
}

/** The class of the row button for `uid`. */
function rowClass(html: string, uid: string): string | null {
  const row = new RegExp(`<button[^>]*data-uid="${uid}"[^>]*>`).exec(html);
  return row ? (/class="([^"]*)"/.exec(row[0])?.[1] ?? null) : null;
}

describe('the warn ring in the Automation list', () => {
  it('rings a named row only while it is on', () => {
    const html = render(new Set(['my-capture', 'off-capture']));
    expect(rowClass(html, 'a')).toBe('st-auto-row is-warn');
    expect(rowClass(html, 'b')).toBe('st-auto-row');
    expect(rowClass(html, 'c')).toBe('st-auto-row');
  });

  it('rings nothing without names', () => {
    const html = render();
    expect(html).not.toContain('is-warn');
  });

  it('tells a reader why a ringed row carries it, as its description', () => {
    const html = render(new Set(['my-capture']), HIDES_PROMPT_NOTE);
    const row = /<button[^>]*data-uid="a"[^>]*>/.exec(html)?.[0] ?? '';
    const described = /aria-describedby="([^"]+)"/.exec(row)?.[1];
    expect(described).toBeTruthy();
    expect(html).toMatch(new RegExp(`id="${described}"[^>]*>This trigger hides your prompt`));
    // A row with no ring has no description.
    const flee = /<button[^>]*data-uid="c"[^>]*>/.exec(html)?.[0] ?? '';
    expect(flee).not.toContain('aria-describedby');
  });

  it('says why a trigger carries it in plain sentences', () => {
    expect(HIDES_PROMPT_NOTE).toMatch(/^This trigger hides your prompt/);
    expect(HIDES_PROMPT_NOTE).not.toMatch(/[;:–—]| - /);
    expect(HIDES_PROMPT_NOTE).toMatch(/in Customize prompt\.$/);
  });
});
