import { renderToStaticMarkup } from 'react-dom/server';
import { isValidElement, type ReactElement, type ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { describe, expect, it, vi } from 'vitest';
import { HELP_TOPICS, parseHelpBody, type HelpTopic } from './helpContent';
import { countMatches, outlineFor } from './helpNav';
import { HelpArticle } from './HelpArticle';
import helpCss from '../styles/help.css?raw';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));

function topic(id: string): HelpTopic {
  const found = HELP_TOPICS.find((t) => t.id === id);
  if (!found) throw new Error(`no help topic ${id}`);
  return found;
}

function draw(shown: HelpTopic, query = '', current = 0): string {
  return renderToStaticMarkup(
    <HelpArticle
      topic={shown}
      query={query}
      current={current}
      outline={outlineFor(shown)}
      markColors={{ fill: 'rgba(196, 178, 138, 0.28)', current: 'rgba(196, 178, 138, 0.6)' }}
    />,
  );
}

const luaTopic: HelpTopic = {
  id: 'test.lua',
  number: '0.1',
  title: 'A pane',
  section: 'Automate',
  body: 'Read the weather.\n\n```lua\n-- weather\nlocal pane = mud.pane("weather", "Weather")\n```',
};

describe('a help topic', () => {
  it('draws the prompt codes as a card with a head and a row per code', () => {
    const html = draw(topic('reference.prompt-codes'));
    expect(html).toContain('<h1 id="hp-title" class="hp-title">Prompt design codes</h1>');
    expect(html).toMatch(/<table class="hp-table"><thead><tr><th scope="col">Code<\/th>/);
    const body = /<tbody>(.*)<\/tbody>/.exec(html)?.[1] ?? '';
    const table = parseHelpBody(topic('reference.prompt-codes').body).find(
      (b) => b.kind === 'table',
    );
    expect(body.match(/<tr>/g)).toHaveLength(table?.kind === 'table' ? table.rows.length : 0);
    expect(body.match(/<tr>/g)).toHaveLength(25);
    // The code column is a row of mono codes, not chips.
    expect(body).toContain(
      '<span class="hp-codes"><code>%hp</code><code>%mana</code><code>%move</code></span>',
    );
  });

  it('draws a label in SF 600, a command as a chip and a key as keycaps', () => {
    const prompt = draw(topic('reference.prompt-codes'));
    expect(prompt).toContain('<strong class="hp-label">Edit as text</strong>');
    expect(prompt).toContain('<strong class="hp-label">Insert value…</strong>');
    const show = draw(topic('shape.prompt-show'));
    expect(show).toContain('<code class="hp-chip">#prompt show lifted</code>');
    expect(show).toMatch(/<ul><li>/);
    const send = draw(topic('play.send-commands'));
    expect(send).toContain(
      '<kbd class="keys"><kbd class="keycap">Shift</kbd><kbd class="keycap">Enter</kbd></kbd>',
    );
  });

  it('draws a Lua block in the colors of the code editor', () => {
    const html = draw(luaTopic);
    expect(html).toContain(
      '<pre class="hp-codeblock"><code><span class="tok-comment">-- weather</span>\n<span class="tok-keyword">local</span>',
    );
    expect(html).toContain('<span class="tok-string">&quot;Weather&quot;</span>');
    expect(helpCss).toMatch(/\.hp-codeblock \.tok-keyword \{\s*color: var\(--accent\);/);
  });

  it('marks a match inside a code block as it counts it', () => {
    const html = draw(luaTopic, 'weather');
    expect(html.match(/<mark class="hp-mark"/g)).toHaveLength(countMatches(luaTopic, 'weather'));
    expect(countMatches(luaTopic, 'weather')).toBe(4);
  });

  it('marks every match, and fills the one you are on stronger', () => {
    const shown = topic('shape.prompt-show');
    const html = draw(shown, 'prompt');
    const marks = html.match(/<mark class="hp-mark"/g) ?? [];
    expect(marks).toHaveLength(countMatches(shown, 'prompt'));
    expect(marks.length).toBeGreaterThan(10);
    // The first match is in the title, and it is the one you are on.
    expect(html).toMatch(
      /<h1[^>]*>Choose where your <mark class="hp-mark" data-match="0" data-current="">prompt<\/mark> shows<\/h1>/,
    );
    expect(html.match(/data-current=""/g)).toHaveLength(1);
    const third = draw(shown, 'prompt', 2);
    expect(third).toMatch(/data-match="2" data-current=""/);
  });

  it('fills the match you are on in yellow at 60 with no ring', () => {
    const html = draw(topic('shape.prompt-show'), 'prompt');
    expect(html).toContain('--hp-mark:rgba(196, 178, 138, 0.28)');
    expect(html).toContain('--hp-mark-current:rgba(196, 178, 138, 0.6)');
    expect(html).not.toContain('--hp-mark-ring');
    const rule = /\.hp-mark\[data-current\]\s*\{([^}]*)\}/.exec(helpCss)?.[1] ?? '';
    expect(rule).toContain('background: var(--hp-mark-current');
    expect(rule).not.toContain('box-shadow');
  });

  it('marks nothing without words', () => {
    expect(draw(topic('shape.prompt-show'))).not.toContain('<mark');
  });

  it('gives each outlined item an id to scroll to', () => {
    const html = draw(topic('reference.slash-commands'));
    const ids = html.match(/<li id="hp-item-\d+-\d+">/g) ?? [];
    expect(ids).toHaveLength(outlineFor(topic('reference.slash-commands'))?.length ?? -1);
  });
});

describe('the help article stylesheet', () => {
  const rule = (selector: string) => {
    const at = helpCss.indexOf(`${selector} {`);
    return at < 0 ? '' : helpCss.slice(at, helpCss.indexOf('}', at));
  };

  it('draws list bullets in the tertiary tone, as the boards do', () => {
    expect(rule('.hp-article li::marker')).toMatch(/color:\s*var\(--tertiary\)/);
  });
});

describe('the Get started topic', () => {
  it('draws Open Get started as a primary button between its paragraphs', () => {
    const html = draw(topic('get-connected.get-started'));
    expect(html).toContain(
      'again here.</p><div class="hp-actions"><button type="button" class="btn is-primary">Open Get started</button></div><p>A new install',
    );
    expect(html).toContain('<th scope="col">Where it lives</th>');
    expect(helpCss).toMatch(/\.hp-actions \{[^}]*margin: 16px 0 0;/);
  });

  it('marks the button when you search for its words', () => {
    const shown = topic('get-connected.get-started');
    expect(draw(shown, 'open get')).toContain(
      '<button type="button" class="btn is-primary"><mark class="hp-mark" data-match="0" data-current="">Open Get</mark> started</button>',
    );
    expect(countMatches(shown, 'open get')).toBe(1);
  });

  it('raises the main window and opens the card from the button', () => {
    const shown = topic('get-connected.get-started');
    const tree = (
      HelpArticle as unknown as { render: (p: unknown, r: null) => ReactElement }
    ).render({ topic: shown, query: '', current: 0, outline: null, markColors: null }, null);
    const clicks: (() => void)[] = [];
    const walk = (node: ReactNode) => {
      if (Array.isArray(node)) node.forEach(walk);
      else if (isValidElement<{ children?: ReactNode; onClick?: () => void }>(node)) {
        if (node.props.onClick) clicks.push(node.props.onClick);
        walk(node.props.children);
      }
    };
    walk(tree);
    expect(clicks).toHaveLength(1);
    clicks[0]();
    expect(invoke).toHaveBeenCalledWith('open_get_started');
  });
});
