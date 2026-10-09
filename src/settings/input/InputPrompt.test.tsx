import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { BandEnv } from '../../terminal/bandCells';
import { previewOptions, previewRows } from '../../prompt/promptSettings';
import type { PromptCheckRead } from '../../ipc/prompt';
import { DrawRow } from './InputPrompt';
import { CodesMetaLine, CodesText, LineRow, PointRow } from './PromptGame';
import { PreviewView } from './PromptPreview';

// The section reaches the Tauri bridge through its stores. The pieces
// under test draw from the values they are handed.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

// Nord's terminal colors.
const NORD: BandEnv = {
  palette: [
    '#3b4252',
    '#bf616a',
    '#a3be8c',
    '#ebcb8b',
    '#81a1c1',
    '#b48ead',
    '#88c0d0',
    '#e5e9f0',
    '#4c566a',
    '#bf616a',
    '#a3be8c',
    '#ebcb8b',
    '#81a1c1',
    '#b48ead',
    '#8fbcbb',
    '#eceff4',
  ],
  fg: '#e5e9f0',
  bg: '#2e3440',
  selection: '#4c566a',
  selectionText: '#eceff4',
  renderer: 'xterm',
  brightBold: false,
};

const FL = 'Your prompt setting in The Forsaken Lands. Vosh reads its codes.';
const text = (html: string) => html.replace(/<[^>]+>/g, '').replace(/&#x27;/g, "'");

describe('the game prompt block', () => {
  it('shows the codes the game sent as text, with no field (P12)', () => {
    const html = renderToStaticMarkup(
      <CodesText
        prompt="%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c"
        fprompt=""
        description={FL}
        meta={{
          tone: 'normal',
          text: 'The game sent it when you logged in. Matches your last 14 prompts.',
          fixes: [],
        }}
      />,
    );
    expect(html).toContain('data-st-anchor="prompt-game"');
    expect(html).toContain('>Your game&#x27;s prompt</span>');
    expect(html).toContain(`<span class="st-row-desc">${FL}</span>`);
    expect(html).toContain('<dt>Prompt</dt><dd class="st-prompt-code">%n%P%C[%h/%Hhp');
    expect(html).toContain('<dt>Fight prompt</dt><dd class="st-prompt-code is-none">None set</dd>');
    expect(html).not.toContain('<input');
    expect(html).toContain(
      '<p class="st-prompt-meta">The game sent it when you logged in. Matches your last 14 prompts.</p>',
    );
  });

  it('keeps the trailing space the game stores (P13)', () => {
    const html = renderToStaticMarkup(
      <CodesText
        prompt="<%h%m %vmv> "
        fprompt="%h fight "
        description={FL}
        meta={{ tone: 'normal', text: 'The game sent it when you logged in.', fixes: [] }}
      />,
    );
    expect(html).toContain('<dd class="st-prompt-code">&lt;%h%m %vmv&gt; </dd>');
    expect(html).toContain('<dd class="st-prompt-code">%h fight </dd>');
  });

  it('puts the warning and the command with Copy in place of the meta (P0)', () => {
    const html = renderToStaticMarkup(
      <CodesMetaLine
        meta={{
          tone: 'warn',
          text: 'Vosh cannot tell where Health ends and Mana begins. Put a space between them in the game.',
          fixes: ['prompt <%h %m %vmv>'],
        }}
      />,
    );
    expect(html).toContain('class="st-prompt-meta is-warn"');
    expect(html).toContain('<span class="st-warn-dot dot is-warn" aria-hidden="true"></span>');
    expect(html).toContain('class="pc-command st-prompt-command"');
    expect(html).toContain('prompt &lt;%h %m %vmv&gt;</span>');
    expect(html).toMatch(/<button[^>]*>Copy<\/button>/);
  });

  it('draws nothing for an empty meta', () => {
    expect(
      renderToStaticMarkup(<CodesMetaLine meta={{ tone: 'normal', text: '', fixes: [] }} />),
    ).toBe('');
  });

  it('shows the line you pointed at with its values marked, and More', () => {
    const read: PromptCheckRead = {
      id: 3,
      raw: '<1020/1020hp 800/800m>',
      plain: '<1020/1020hp 800/800m>',
      at_ms: 0,
      fight: false,
      marks: [
        { line: 0, start: 1, end: 5, field: 'hp', label: 'Health', warn: false },
        { line: 0, start: 6, end: 10, field: 'maxhp', label: 'Max', warn: false },
        { line: 0, start: 13, end: 16, field: 'mana', label: 'Mana', warn: false },
      ],
    };
    const html = renderToStaticMarkup(
      <LineRow
        read={read}
        lastRead="Last read at 8:42"
        emptyText={null}
        onPoint={() => undefined}
        onForget={() => undefined}
      />,
    );
    expect(html).toContain(
      '<span class="st-prompt-line">&lt;<span class="st-prompt-mark">1020</span>/<span class="st-prompt-mark">1020</span>hp <span class="st-prompt-mark">800</span>/800m&gt;</span>',
    );
    expect(html).toContain('<span class="st-prompt-line-meta">Last read at 8:42</span>');
    expect(html).toContain('aria-label="Prompt options"');
    expect(html).toContain('aria-haspopup="menu"');
  });

  it('says no prompt matched in warn, with Point at it again… beside More (P14)', () => {
    const read: PromptCheckRead = {
      id: 3,
      raw: '<1020/1020hp 800/800m>',
      plain: '<1020/1020hp 800/800m>',
      at_ms: 0,
      fight: false,
      marks: [{ line: 0, start: 1, end: 5, field: 'hp', label: 'Health', warn: false }],
    };
    const line =
      'No prompt has matched since 8:12. If you changed it in the game, point at it again.';
    const html = renderToStaticMarkup(
      <LineRow
        read={read}
        lastRead="Last read at 8:12"
        emptyText={null}
        notMatching={line}
        onPoint={() => undefined}
        onForget={() => undefined}
      />,
    );
    expect(html).toContain(
      `<span class="st-prompt-line-meta is-warn" role="status"><span class="st-warn-dot dot is-warn" aria-hidden="true"></span><span>${line}</span></span>`,
    );
    expect(html).not.toContain('Last read at');
    expect(text(html)).toContain(`${line}Point at it again…`);
    expect(html).toContain('aria-label="Prompt options"');
    // Matching, the row keeps Last read at and no button.
    const matching = renderToStaticMarkup(
      <LineRow
        read={read}
        lastRead="Last read at 8:12"
        emptyText={null}
        notMatching={null}
        onPoint={() => undefined}
        onForget={() => undefined}
      />,
    );
    expect(matching).not.toContain('Point at it again');
    expect(matching).toContain('Last read at 8:12');
  });

  it('says why no line shows before the pattern reads a prompt', () => {
    const html = renderToStaticMarkup(
      <LineRow
        read={null}
        lastRead={null}
        emptyText="Vosh has not seen your prompt since you connected. Send a command and Vosh checks again."
        onPoint={() => undefined}
        onForget={() => undefined}
      />,
    );
    expect(html).not.toContain('st-prompt-line"');
    expect(html).toContain('Vosh has not seen your prompt since you connected.');
  });

  it('tells you where to point on another game', () => {
    const html = renderToStaticMarkup(<PointRow />);
    expect(text(html)).toContain(
      "Your game's promptPoint at it in Customize prompt and Vosh reads its numbers.",
    );
    expect(html).toContain('data-st-anchor="prompt-game"');
  });
});

describe('Draw your own prompt', () => {
  const draw = (capture: boolean, on: boolean, description: string) =>
    renderToStaticMarkup(
      <DrawRow
        capture={capture}
        draw={on}
        description={description}
        onCustomize={() => undefined}
        onDraw={() => undefined}
      />,
    );
  const switchOf = (html: string) => /<input(?=[^>]*role="switch")[^>]*>/.exec(html)?.[0] ?? '';

  it('holds Customize… and the switch (P12)', () => {
    const html = draw(true, true, 'It takes the place of the prompt The Forsaken Lands sends.');
    expect(html).toContain('class="st-row st-draw-row"');
    expect(html).toMatch(/<button[^>]*>Customize…<\/button>/);
    expect(switchOf(html)).toContain('checked=""');
    expect(switchOf(html)).not.toContain('disabled');
  });

  it('waits on a capture with Customize… still open to you (P13)', () => {
    const html = draw(false, true, 'Customize your prompt first.');
    expect(html).toContain('class="st-row st-draw-row is-waiting"');
    expect(html).toContain('Customize your prompt first.');
    expect(html).not.toMatch(/<button[^>]*disabled[^>]*>Customize…/);
    expect(switchOf(html)).toContain('disabled=""');
    expect(switchOf(html)).not.toContain('checked');
  });
});

describe('the preview', () => {
  const view = (ansi: string, band = false, forsaken = true) =>
    renderToStaticMarkup(
      <PreviewView
        rows={previewRows(ansi)}
        preview="now"
        options={previewOptions(forsaken)}
        onPreview={() => undefined}
        band={band}
        env={NORD}
        cellW={7.8}
        meta="Right click your prompt in the terminal to change it there."
      />,
    );

  it('draws one line 28 tall under the Segmented (P12)', () => {
    const html = view('\x1b[32m1020\x1b[39m/1020hp ');
    expect(html).toContain('aria-label="Your prompt as Vosh draws it"');
    expect(html).toContain('height:28px');
    expect([...html.matchAll(/class="st-seg-item"[^>]*>([^<]*)</g)].map((m) => m[1])).toEqual([
      'Now',
      'Low health',
      'Fight',
      'Lament',
    ]);
    // ANSI 2 resolves through the theme, Nord's green.
    expect(html).toContain('color:#a3be8c');
    expect(html).toContain('Right click your prompt in the terminal to change it there.');
    expect(html).not.toContain('prompt-band');
  });

  it('rings a part your prompt no longer feeds (P14)', () => {
    const html = renderToStaticMarkup(
      <PreviewView
        rows={previewRows('Tester: Tank health\r\n[1020/1020hp]')}
        preview="fight"
        options={previewOptions(true)}
        onPreview={() => undefined}
        band={false}
        env={NORD}
        cellW={7.8}
        meta="Right click your prompt in the terminal to change it there."
        rings={[{ left: 72.4, top: 5.25, width: 85.8, height: 17.5 }]}
      />,
    );
    expect(html).toContain(
      'class="st-prompt-preview-warn" aria-hidden="true" style="left:72.4px;top:5.25px;width:85.8px;height:17.5px"',
    );
    expect(view('[1020/1020hp]')).not.toContain('st-prompt-preview-warn');
  });

  it('grows 17.5 for each line past the first', () => {
    const html = view('Tester: \x1b[32m█████████\x1b[90m░\x1b[0m\r\n1020/1020hp ');
    expect(html).toContain('height:45.5px');
    expect([...html.matchAll(/st-prompt-preview-row/g)]).toHaveLength(2);
  });

  it('leaves Lament out on another game', () => {
    expect(view('x', false, false)).not.toContain('Lament');
  });

  it('draws on the band while your prompt shows lifted or pinned', () => {
    const html = view('1020/1020hp ', true);
    // 4 left of the text at 10, 2 above the row at 5.25, as wide as 11
    // cells plus 4 each side, the trailing space left out.
    const band = /class="prompt-band"[^>]*style="([^"]*)"/.exec(html)?.[1] ?? '';
    expect(band).toContain('left:6px');
    expect(band).toContain('top:3.25px');
    expect(band).toContain('width:93.8px');
    expect(band).toContain('height:21.5px');
  });
});
