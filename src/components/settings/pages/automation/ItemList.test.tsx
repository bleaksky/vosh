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

function render(warnNames?: ReadonlySet<string>): string {
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

  it('says why a trigger carries it in plain sentences', () => {
    expect(HIDES_PROMPT_NOTE).toMatch(/^This trigger hides your prompt/);
    expect(HIDES_PROMPT_NOTE).not.toMatch(/[;:–—]| - /);
    expect(HIDES_PROMPT_NOTE).toMatch(/in Customize prompt\.$/);
  });
});
