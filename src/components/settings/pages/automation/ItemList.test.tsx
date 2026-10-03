import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { buildSections, sectionKeyOf, type ListEntry } from '../../../../lib/automationList';
import settingsCss from '../../../../styles/settings.css?raw';
import { ItemList, type ItemListProps } from './ItemList';
import { HIDES_PROMPT_NOTE } from './TriggersEditor';

const entry = (uid: string, name: string, enabled: boolean, group = ''): ListEntry => ({
  uid,
  name,
  group,
  enabled,
  text: name,
});

const NO_FOLDS: ReadonlySet<string> = new Set();

function renderList(props: Partial<ItemListProps> & Pick<ItemListProps, 'sections'>): string {
  return renderToStaticMarkup(
    <ItemList
      noun={{ one: 'trigger', many: 'triggers' }}
      filterLabel="Filter triggers"
      filter=""
      onFilter={() => {}}
      hasItems
      emptyText=""
      pinned={null}
      selected="a"
      onSelect={() => {}}
      revealSeq={0}
      monoName={false}
      monoMeta={false}
      folded={NO_FOLDS}
      onFold={() => {}}
      {...props}
    />,
  );
}

function render(warnNames?: ReadonlySet<string>, warnNote?: string): string {
  return renderList({
    sections: buildSections([
      entry('a', 'my-capture', true),
      entry('b', 'off-capture', false),
      entry('c', 'flee', true),
    ]),
    warnNames,
    ...(warnNote ? { warnNote } : {}),
  });
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

describe('collapsible groups in the Automation list', () => {
  const sections = buildSections([
    entry('u', 'Echo my deaths', true),
    entry('c1', 'Flee below 20 percent', true, 'combat'),
    entry('c2', 'Loot every kill', false, 'combat'),
    entry('i1', 'Sleep when mana is low', true, 'idle'),
  ]);
  const combat = sectionKeyOf({ group: 'combat' });

  /** The heading button for a group, by its fold key. */
  const heading = (html: string, key: string) =>
    new RegExp(`<button[^>]*data-fold="${key}"[^>]*>.*?</button>`).exec(html)?.[0] ?? '';

  it('makes each group heading a button that says whether it is open', () => {
    const html = renderList({ sections, selected: 'u' });
    const button = heading(html, 'g:combat');
    expect(button).toMatch(/^<button type="button" class="st-auto-fold" aria-expanded="true"/);
    expect(button).toContain('st-auto-fold-chevron');
    expect(button).toContain('<span class="st-auto-fold-name">combat</span>');
    // The heading stays a heading around its button.
    expect(html).toContain(`<h2 class="st-auto-heading">${button}</h2>`);
    // An open group names the rows it controls and shows no count.
    const controls = /aria-controls="([^"]+)"/.exec(button)?.[1];
    expect(controls).toBeTruthy();
    expect(html).toMatch(new RegExp(`<div id="${controls}" class="st-auto-group">.*data-uid="c1"`));
    expect(button).not.toContain('st-auto-fold-count');
  });

  it('hides the rows of a folded group and counts them on its heading', () => {
    const html = renderList({ sections, selected: 'u', folded: new Set([combat]) });
    const button = heading(html, 'g:combat');
    expect(button).toContain('aria-expanded="false"');
    expect(button).not.toContain('aria-controls');
    expect(button).toContain(
      '<span class="st-auto-fold-count">2<span class="st-visually-hidden"> triggers</span></span>',
    );
    expect(html).not.toContain('data-uid="c1"');
    expect(html).not.toContain('data-uid="c2"');
    // The other groups and the ungrouped items still show.
    expect(html).toContain('data-uid="u"');
    expect(html).toContain('data-uid="i1"');
  });

  it('counts one item in the singular', () => {
    const html = renderList({ sections, selected: 'u', folded: new Set(['g:idle']) });
    expect(heading(html, 'g:idle')).toContain('>1<span class="st-visually-hidden"> trigger</span>');
  });

  it('gives the ungrouped items at the top no heading to fold', () => {
    const html = renderList({
      sections,
      selected: 'u',
      folded: new Set([sectionKeyOf(entry('u', '', true))]),
    });
    expect([...html.matchAll(/data-fold="([^"]+)"/g)].map((m) => m[1])).toEqual([
      'g:combat',
      'g:idle',
    ]);
    expect(html).toContain('data-uid="u"');
  });

  it('gives Tab to the selected row, or to the heading of the folded group that hides it', () => {
    const tabbable = (html: string) =>
      [...html.matchAll(/<button[^>]*tabindex="0"[^>]*>/g)].map(
        (m) => /data-(?:uid|fold)="([^"]+)"/.exec(m[0])?.[1],
      );
    expect(tabbable(renderList({ sections, selected: 'c2' }))).toEqual(['c2']);
    expect(tabbable(renderList({ sections, selected: 'c2', folded: new Set([combat]) }))).toEqual([
      'g:combat',
    ]);
  });

  it('leaves room on the heading row for the group switch', () => {
    // With no ungrouped items the first heading sits right under the filter.
    const html = renderList({ sections: sections.slice(1), selected: 'c1' });
    expect(html).toMatch(/<div class="st-auto-headrow is-first"><h2 class="st-auto-heading">/);
    expect(html).toMatch(/<\/div><div class="st-auto-headrow"><h2 class="st-auto-heading">/);
    // The heading takes the row's free width and leaves the rest after it.
    const rule = /\.st-auto-headrow \{[^}]*\}/.exec(settingsCss)?.[0] ?? '';
    expect(rule).toContain('display: flex');
    const title = /\.st-auto-heading \{[^}]*\}/.exec(settingsCss)?.[0] ?? '';
    expect(title).toContain('flex: 1 1 auto');
    expect(title).toContain('min-width: 0');
  });

  it('turns the chevron down while the group is open', () => {
    expect(settingsCss).toMatch(
      /\.st-auto-fold\[aria-expanded='true'\] \.st-auto-fold-chevron \{\s*transform: rotate\(90deg\);/,
    );
  });
});
