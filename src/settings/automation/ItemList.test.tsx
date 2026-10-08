import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { buildSections, sectionKeyOf, type ListEntry } from '../../automation/automationList';
import settingsCss from '../../styles/settings.css?raw';
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

function render(warnNotes?: ReadonlyMap<string, string>): string {
  return renderList({
    sections: buildSections([
      entry('a', 'my-capture', true),
      entry('b', 'off-capture', false),
      entry('c', 'flee', true),
    ]),
    warnNotes,
  });
}

/** The class of the row button for `uid`. */
function rowClass(html: string, uid: string): string | null {
  const row = new RegExp(`<button[^>]*data-uid="${uid}"[^>]*>`).exec(html);
  return row ? (/class="([^"]*)"/.exec(row[0])?.[1] ?? null) : null;
}

describe('the warn ring in the Automation list', () => {
  it('rings a named row only while it is on', () => {
    const html = render(
      new Map([
        ['my-capture', HIDES_PROMPT_NOTE],
        ['off-capture', HIDES_PROMPT_NOTE],
      ]),
    );
    expect(rowClass(html, 'a')).toBe('st-auto-row is-warn');
    expect(rowClass(html, 'b')).toBe('st-auto-row');
    expect(rowClass(html, 'c')).toBe('st-auto-row');
  });

  it('rings nothing without names', () => {
    const html = render();
    expect(html).not.toContain('is-warn');
  });

  it('tells a reader why a ringed row carries it, as its description', () => {
    const html = render(new Map([['my-capture', HIDES_PROMPT_NOTE]]));
    const row = /<button[^>]*data-uid="a"[^>]*>/.exec(html)?.[0] ?? '';
    const described = /aria-describedby="([^"]+)"/.exec(row)?.[1];
    expect(described).toBeTruthy();
    expect(html).toMatch(new RegExp(`id="${described}"[^>]*>This trigger hides your prompt`));
    // A row with no ring has no description.
    const flee = /<button[^>]*data-uid="c"[^>]*>/.exec(html)?.[0] ?? '';
    expect(flee).not.toContain('aria-describedby');
  });

  it('gives each ringed row its own note', () => {
    const html = render(
      new Map([
        ['my-capture', 'First note.'],
        ['flee', 'Second note.'],
      ]),
    );
    const noteOf = (uid: string) => {
      const row = new RegExp(`<button[^>]*data-uid="${uid}"[^>]*>`).exec(html)?.[0] ?? '';
      const id = /aria-describedby="([^"]+)"/.exec(row)?.[1] ?? '';
      return new RegExp(`id="${id}"[^>]*>([^<]*)<`).exec(html)?.[1];
    };
    expect(noteOf('a')).toBe('First note.');
    expect(noteOf('c')).toBe('Second note.');
  });

  // Presets board 4: a fix that changed a row you edited rings the row
  // on or off, and its note wins over the prompt note.
  it('rings a row a fix flagged while it is off too', () => {
    const fix = 'A fix to Disarms and fading buffs changed Then send, a row you edited.';
    const html = renderList({
      sections: buildSections([
        { ...entry('a', 'disarm.secondary', false), warn: fix },
        { ...entry('b', 'my-capture', true), warn: fix },
      ]),
      warnNotes: new Map([['my-capture', HIDES_PROMPT_NOTE]]),
    });
    expect(rowClass(html, 'a')).toBe('st-auto-row is-warn');
    expect(rowClass(html, 'b')).toBe('st-auto-row is-warn');
    expect(html).toContain(`>${fix}<`);
    expect(html).not.toContain('This trigger hides your prompt');
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

describe('the switch on a group heading', () => {
  const sections = buildSections([
    entry('a', 'Echo my deaths', true),
    entry('b1', 'Flee below 20 percent', true, 'combat'),
    entry('c1', 'Sleep when mana is low', true, 'idle'),
    { ...entry('p1', 'room.target', true), preset: true },
  ]);
  const turned: [string, boolean][] = [];
  const switches = {
    byName: new Map([
      ['combat', { name: 'combat', enabled: true }],
      ['idle', { name: 'idle', enabled: false, loadouts: { on: false, by: ['Healer'] } }],
      ['loot', { name: 'loot', enabled: true }],
    ]),
    turn: (group: string, enabled: boolean) => void turned.push([group, enabled]),
  };
  /** The switch input after the heading of `group`. */
  const switchOf = (html: string, group: string) =>
    new RegExp(`<input[^>]*data-group-switch="${group}"[^>]*>`).exec(html)?.[0] ?? null;

  it('sits after each group heading the store knows, and never on the rest', () => {
    const html = renderList({ sections, groupSwitches: switches });
    expect(switchOf(html, 'combat')).toMatch(/role="switch"/);
    expect(switchOf(html, 'combat')).toMatch(/aria-label="combat group"/);
    expect(switchOf(html, 'combat')).toMatch(/checked=""/);
    expect(switchOf(html, 'idle')).not.toMatch(/checked=""/);
    // The switch follows the heading's h2, inside the heading row.
    expect(html).toMatch(
      /<h2 class="st-auto-heading">(?:(?!<\/h2>).)*<\/h2><span class="st-toggle st-auto-groupswitch"><input[^>]*data-group-switch="combat"/,
    );
    // No switch for the ungrouped items, the presets, or a list with none.
    expect(html.match(/data-group-switch=/g)).toHaveLength(2);
    expect(renderList({ sections })).not.toContain('data-group-switch');
  });

  it('still turns a group the loadouts decide, and says which', () => {
    const html = renderList({ sections, groupSwitches: switches });
    const idle = switchOf(html, 'idle') ?? '';
    expect(idle).not.toMatch(/disabled/);
    const note = /aria-describedby="([^"]+)"/.exec(idle)?.[1];
    expect(note).toBeTruthy();
    expect(html).toContain(
      `<p id="${note}" class="st-auto-groupnote">The Healer loadout leaves this group off.</p>`,
    );
    // The heading carries the same note, so you hear it on either stop.
    const heading = /<button[^>]*data-fold="g:idle"[^>]*>/.exec(html)?.[0] ?? '';
    expect(heading).toContain(`aria-describedby="${note}"`);
    expect(switchOf(html, 'combat')).not.toMatch(/disabled|aria-describedby/);
  });

  it('says when the loadouts turn back a group that #group turned', () => {
    // #group idle on while Healer leaves idle off. The switch shows the
    // group as it is, and the note says the loadouts turn it back.
    const turned = {
      ...switches,
      byName: new Map([
        ['idle', { name: 'idle', enabled: true, loadouts: { on: false, by: ['Healer'] } }],
      ]),
    };
    const html = renderList({ sections, groupSwitches: turned });
    const idle = switchOf(html, 'idle') ?? '';
    expect(idle).toMatch(/checked=""/);
    expect(idle).not.toMatch(/disabled/);
    expect(idle).toMatch(/aria-describedby="[^"]+"/);
    expect(html).toContain(
      'class="st-auto-groupnote">The Healer loadout turns this group off again when you next launch Vosh, switch profiles, or save Loadouts.</p>',
    );
  });

  it('takes Tab only from its heading, so the list keeps one stop from outside', () => {
    const tabbable = (html: string) =>
      [...html.matchAll(/<(?:button|input)[^>]*tabindex="0"[^>]*>/g)].map(
        (m) => /data-(?:uid|fold|group-switch)="([^"]+)"/.exec(m[0])?.[1],
      );
    expect(tabbable(renderList({ sections, groupSwitches: switches, selected: 'b1' }))).toEqual([
      'b1',
    ]);
    // With the selection in a folded group its heading holds the stop, and
    // its switch follows it.
    expect(
      tabbable(
        renderList({
          sections,
          groupSwitches: switches,
          selected: 'b1',
          folded: new Set(['g:combat']),
        }),
      ),
    ).toEqual(['g:combat', 'combat']);
    // A switch the loadouts decide takes Tab from its heading the same way.
    expect(
      tabbable(
        renderList({
          sections,
          groupSwitches: switches,
          selected: 'c1',
          folded: new Set(['g:idle']),
        }),
      ),
    ).toEqual(['g:idle', 'idle']);
  });
});

// First Run board 4: a suggested preset that is off wears the accent ring
// where the off ring sits, and a reader hears Suggested, off.
describe('the suggested ring in the Automation list', () => {
  const html = renderList({
    sections: buildSections([
      { ...entry('a', 'Cures and heals', false), dot: 'suggested' },
      { ...entry('b', 'Herb labels', false), anchor: 'presets:herb_labels' },
      { ...entry('c', 'Your damage verbs', true), dot: 'suggested' },
    ]),
  });
  const dot = (uid: string) => {
    const row = new RegExp(`data-uid="${uid}"[^]*?</button>`).exec(html)?.[0] ?? '';
    return {
      dot: /class="(st-auto-dot[^"]*)"/.exec(row)?.[1],
      heard: />(Enabled|Off|Suggested, off)</.exec(row)?.[1],
    };
  };

  it('rings a suggested row only while it is off', () => {
    expect(dot('a')).toEqual({ dot: 'st-auto-dot is-off is-suggested', heard: 'Suggested, off' });
    expect(dot('b')).toEqual({ dot: 'st-auto-dot is-off', heard: 'Off' });
    expect(dot('c')).toEqual({ dot: 'st-auto-dot', heard: 'Enabled' });
  });

  it('draws the ring in the accent where the off ring sits', () => {
    expect(settingsCss).toMatch(
      /\.st-auto-dot\.is-off\.is-suggested \{\s*box-shadow: inset 0 0 0 1\.25px var\(--accent\);\s*\}/,
    );
  });

  it('puts a row anchor on the row a link opens', () => {
    expect(html).toMatch(/data-uid="b"[^>]*data-st-anchor="presets:herb_labels"/);
    expect(html).not.toMatch(/data-uid="a"[^>]*data-st-anchor/);
  });
});

// Presets Q6: a preset you edited wears a 12 px pencil just before its
// dot, and a reader hears edited after its name.
describe('the pencil of an edited preset', () => {
  const html = renderList({
    sections: buildSections([
      { ...entry('a', 'Disarms and fading buffs', true), edited: true },
      { ...entry('b', 'Herb labels', true) },
    ]),
  });
  const row = (uid: string) => new RegExp(`data-uid="${uid}"[^]*?</button>`).exec(html)?.[0] ?? '';

  it('draws the pencil before the dot and names the row edited', () => {
    expect(row('a')).toMatch(
      /Disarms and fading buffs<span class="st-visually-hidden">, edited<\/span><\/span><svg width="12" height="12"[^>]*class="st-auto-mark"[^]*?<\/svg><span class="st-auto-dot"/,
    );
    expect(row('b')).not.toMatch(/st-auto-mark|edited/);
  });

  it('sets the pencil in the tertiary color right before the dot', () => {
    expect(settingsCss).toMatch(
      /\.st-auto-mark \{\s*flex: none;\s*margin-left: auto;\s*color: var\(--tertiary\);\s*\}\s*\.st-auto-mark \+ \.st-auto-dot \{\s*margin-left: 0;\s*\}/,
    );
  });
});
