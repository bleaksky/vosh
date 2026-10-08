import { describe, expect, it } from 'vitest';
import { HELP_SECTIONS, HELP_TOPICS, parseHelpBody } from './helpContent';
import { readHelp } from './readHelp';
import { ALERT_PRESETS } from '../automation/alertPresets';
import { SETTINGS_MENU } from '../terminal/settingsMenu';

// The help text lives in HELP.md and nowhere else. These tests hold its
// format and the few places it names something the code defines. They
// leave the prose to HELP.md.

function body(id: string): string {
  const topic = HELP_TOPICS.find((t) => t.id === id);
  if (!topic) throw new Error(`no help topic ${id}`);
  return topic.body;
}

describe('readHelp', () => {
  const md = [
    '# Vosh help',
    '',
    'Words for readers of the file.',
    '',
    '## First',
    '',
    '### 1.1 Open a door',
    '',
    '<!-- id: first.door -->',
    '',
    'Push it.',
    '',
    '```lua',
    '## not a section',
    '### 9.9 not a topic',
    '```',
    '',
    '## Second',
    '',
    '### 2.1 Close it',
    '',
    '<!-- id: second.close -->',
    '',
    'Pull it.',
    '',
  ].join('\n');

  it('returns the sections in order', () => {
    expect(readHelp(md).sections).toEqual(['First', 'Second']);
  });

  it('reads a heading as number, title, section and id', () => {
    const [door] = readHelp(md).topics;
    expect([door?.id, door?.number, door?.title, door?.section]).toEqual([
      'first.door',
      '1.1',
      'Open a door',
      'First',
    ]);
  });

  it('leaves the id line out of the body and trims it', () => {
    expect(readHelp(md).topics[0]?.body.startsWith('Push it.')).toBe(true);
  });

  it('keeps a fenced block that holds headings inside its body', () => {
    const { topics } = readHelp(md);
    expect(topics.map((t) => t.id)).toEqual(['first.door', 'second.close']);
    expect(topics[0]?.body).toBe('Push it.\n\n```lua\n## not a section\n### 9.9 not a topic\n```');
  });

  it('runs the last topic to the end of the file', () => {
    expect(readHelp(md).topics[1]?.body).toBe('Pull it.');
  });

  it('throws on a topic with no id line', () => {
    expect(() => readHelp(md.replace('<!-- id: second.close -->', ''))).toThrow(
      /2\.1 Close it has no id line/,
    );
  });

  it('throws on an id used twice', () => {
    expect(() => readHelp(md.replace('second.close', 'first.door'))).toThrow(
      /first\.door is used twice/,
    );
  });
});

describe('the help catalog', () => {
  it('keeps every topic number unique and in order within its section', () => {
    const numbers = HELP_TOPICS.map((t) => t.number);
    expect(new Set(numbers).size).toBe(numbers.length);
    HELP_SECTIONS.forEach((section, s) => {
      HELP_TOPICS.filter((t) => t.section === section).forEach((t, i) => {
        expect(t.number).toBe(`${s + 1}.${i + 1}`);
      });
    });
  });

  it('draws every Vosh link as a button, never as stray text', () => {
    const links = HELP_TOPICS.flatMap((t) =>
      [...t.body.matchAll(/\[([^\]\n]+)\]\(vosh:([^)\n]*)\)/g)].map((m) => ({ t, m })),
    );
    expect(links.length).toBeGreaterThan(0);
    for (const { t, m } of links) {
      expect(parseHelpBody(t.body), m[0]).toContainEqual({
        kind: 'action',
        label: m[1],
        action: m[2],
      });
    }
  });

  it('names a topic by its own title wherever a body points to its number', () => {
    for (const t of HELP_TOPICS) {
      for (const m of t.body.matchAll(/ at (\d+\.\d+)\b/g)) {
        const target = HELP_TOPICS.find((o) => o.number === m[1]);
        expect(target, `${t.number} points to ${m[1]}`).toBeDefined();
        const before = t.body.slice(0, m.index);
        expect(before.endsWith(` ${target?.title}`), `${t.number} points to ${m[1]}`).toBe(true);
      }
    }
  });
});

describe('the help body format', () => {
  it('reads a lone Vosh link as a button and any other link as text', () => {
    expect(parseHelpBody('[Open Get started](vosh:get-started)')).toEqual([
      { kind: 'action', label: 'Open Get started', action: 'get-started' },
    ]);
    for (const text of ['[Open it](vosh:nowhere)', 'See [Open it](vosh:get-started) here.']) {
      expect(parseHelpBody(text)).toEqual([{ kind: 'paragraph', text }]);
    }
  });

  it('reads paragraphs, lists and tables', () => {
    expect(
      parseHelpBody('One line.\n\n- a\n- b\n\n| A | B |\n|---|---|\n| `x` | y. |\n| z | w |'),
    ).toEqual([
      { kind: 'paragraph', text: 'One line.' },
      { kind: 'list', items: ['a', 'b'] },
      {
        kind: 'table',
        head: ['A', 'B'],
        rows: [
          ['`x`', 'y.'],
          ['z', 'w'],
        ],
      },
    ]);
  });
});

describe('the help code block', () => {
  it('reads a fenced block whole, blank lines and all', () => {
    expect(parseHelpBody('Say it.\n\n```lua\nlocal a = 1\n\nend\n```\n\n- after')).toEqual([
      { kind: 'paragraph', text: 'Say it.' },
      { kind: 'code', lang: 'lua', text: 'local a = 1\n\nend' },
      { kind: 'list', items: ['after'] },
    ]);
  });
});

describe('the help on prompt design codes', () => {
  it('draws them as a table under Reference', () => {
    const topic = HELP_TOPICS.find((t) => t.id === 'reference.prompt-codes');
    expect([topic?.number, topic?.title, topic?.section]).toEqual([
      '9.3',
      'Prompt design codes',
      'Reference',
    ]);
    const blocks = parseHelpBody(topic?.body ?? '');
    const tables = blocks.filter((b) => b.kind === 'table');
    expect(tables).toHaveLength(1);
    const [table] = tables;
    if (table?.kind !== 'table') throw new Error('no prompt codes table');
    expect(table.head).toEqual(['Code', 'What it does']);
    expect(table.rows.length).toBeGreaterThan(0);
    for (const [codes, sentence] of table.rows) {
      // Each row is one or more codes in backticks and one sentence.
      expect(codes).toMatch(/^`%[^`]+`( `%[^`]+`)*$/);
      expect(sentence).toMatch(/^[A-Z].*\.$/);
    }
    // The writing style keeps colons and semicolons out of the prose.
    for (const block of blocks) {
      if (block.kind === 'paragraph') expect(block.text).not.toMatch(/[;:] /);
    }
  });
});

// The two tests below tie the help to lists the code defines, so a
// renamed Settings tab or alert preset shows here.
describe('the help on lists the code defines', () => {
  it('names every row of the Settings list as the menu shows it', () => {
    const text = body('play.right-click-menu');
    for (const row of SETTINGS_MENU.flat()) {
      expect(text, row.label).toContain(`\`${row.label}\``);
    }
  });

  it('names each preset as the Alerts category lists it', () => {
    const text = body('automate.alerts');
    for (const preset of ALERT_PRESETS) expect(text).toContain(`\`${preset.name}\``);
  });
});
