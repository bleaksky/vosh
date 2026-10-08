import { isValidElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { BandEnv } from '../terminal/bandCells';
import { menuPosition } from './cardRules';
import { MORE_STYLES_HEIGHT, MORE_STYLES_PLACE } from './PromptPiece';
import type { PromptCheckRead } from '../ipc/prompt';
import { CandidateBox, MatchRow } from './PromptCandidate';
import { CodesEntry } from './PromptCodes';
import { DrawOff } from './PromptStarts';
import { NameGroup } from './PromptPoint';

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

const widths: Record<string, number> = { Wizi: 21.6, Incog: 27.6, Health: 33.5 };
const measure = (label: string) => widths[label] ?? label.length * 6;

const read: PromptCheckRead = {
  id: 7,
  raw: '\x1b[38;5;240m(Wizi \x1b[0m60\x1b[38;5;240m)\x1b[0m [1020/1020hp]',
  plain: '(Wizi 60) [1020/1020hp]',
  at_ms: 0,
  fight: false,
  marks: [
    { line: 0, start: 6, end: 8, field: 'wizi', label: 'Wizi', warn: false },
    { line: 0, start: 11, end: 15, field: 'hp', label: 'Health', warn: false },
  ],
};

const px = (css: string, prop: string) =>
  Number(new RegExp(`${prop}:\\s*([\\d.-]+)px`).exec(css)?.[1]);

describe('the candidate box', () => {
  it('marks each value in the selection token and names it under its first character', () => {
    const html = renderToStaticMarkup(
      <CandidateBox
        read={read}
        env={NORD}
        cellW={7.8}
        measure={measure}
        label="Your newest prompt"
      />,
    );
    expect(html).toContain('aria-label="Your newest prompt"');
    // 10 + 17.5 + 4 + 15 + 10.5, one row of names.
    expect(html).toContain('height:57px');
    const tokens = [...html.matchAll(/class="pc-cells-token" style="([^"]*)"/g)].map((m) => [
      Number(px(m[1], 'left').toFixed(1)),
      Number(px(m[1], 'width').toFixed(1)),
    ]);
    expect(tokens).toEqual([
      [46.8, 15.6],
      [85.8, 31.2],
    ]);
    const names = [...html.matchAll(/class="pc-box-name" style="([^"]*)">([^<]*)</g)].map((m) => [
      m[2],
      Number(px(m[1], 'left').toFixed(1)),
      px(m[1], 'top'),
    ]);
    expect(names).toEqual([
      ['Wizi', 56.8, 31.5],
      ['Health', 95.8, 31.5],
    ]);
  });

  it('draws a marked value the game draws too dim in the text color', () => {
    const html = renderToStaticMarkup(
      <CandidateBox
        read={read}
        env={NORD}
        cellW={7.8}
        measure={measure}
        label="Your newest prompt"
      />,
    );
    const glyph = (ch: string, left: number) =>
      new RegExp(`left:${left}px[^"]*color:([^;"]*)[^>]*>${ch}<`).exec(html)?.[1];
    // The W of the prefix keeps 256 color 240, the 6 under the token
    // takes the text color.
    expect(glyph('W', 7.8)).toBe('#585858');
    expect(glyph('6', 46.8)).toBe('#e5e9f0');
  });
});

describe('the match line', () => {
  const check = {
    matched: 14,
    total: 14,
    fight_matched: 3,
    false_matches: 0,
    text: 'Matches your last 14 prompts and no other line. 3 of them are from a fight.',
    reads: Array.from({ length: 14 }, (_, i) => ({ ...read, id: i + 1 })),
  };

  it('puts the fight count on its own line beside the stepper', () => {
    const html = renderToStaticMarkup(<MatchRow check={check} index={5} onStep={() => {}} />);
    expect(html).toContain(
      '<span>Matches your last 14 prompts and no other line.</span><span><br/>3 of them are from a fight.</span>',
    );
    expect(html).toContain('6 of 14');
    expect(html).toContain('aria-label="Newer prompt"');
    expect(html).toContain('aria-label="Older prompt"');
    expect(html).toContain('pc-match-check');
  });

  it('says so with no stepper before the first prompt', () => {
    const html = renderToStaticMarkup(
      <MatchRow
        check={{
          ...check,
          matched: 0,
          total: 0,
          fight_matched: 0,
          text: 'Vosh has not seen.',
          reads: [],
        }}
        index={0}
        onStep={() => {}}
      />,
    );
    expect(html).not.toContain('pc-stepper');
    expect(html).not.toContain('pc-match-check');
    expect(html).not.toContain('pc-warn-dot');
  });

  it('gives codes that run together its place, in the warn color', () => {
    const html = renderToStaticMarkup(
      <MatchRow
        check={check}
        index={0}
        onStep={() => {}}
        warning="Vosh cannot tell where Health ends and Mana begins. Put a space between them in the game."
      />,
    );
    expect(html).toContain('pc-match-text is-warn is-wrap');
    expect(html).toContain('Vosh cannot tell where Health ends');
    expect(html).toContain('1 of 14');
  });
});

describe('telling Vosh your prompt (P2)', () => {
  it('asks for your setting with the game default as a hint while Vosh has none', () => {
    const html = renderToStaticMarkup(
      <CodesEntry
        session={1}
        initial={null}
        onRead={() => {}}
        onPoint={() => {}}
        onGameSent={() => {}}
      />,
    );
    expect(html).toContain('What is your prompt setting?');
    expect(html).toContain(
      'Type prompt in the game and Vosh reads the answer. You can also paste it here.',
    );
    expect(html).toContain('placeholder="%n%P%C&lt;%hhp %mm %vmv&gt;"');
    expect(html).toContain('placeholder="None set"');
    expect(html).toContain(
      'The game uses your fight prompt while you fight, once you set one with fprompt.',
    );
    // Read these codes waits for your setting.
    expect(html).toMatch(/<button[^>]*disabled=""[^>]*>Read these codes<\/button>/);
    expect(html).toContain('>Point at the line instead</button>');
  });

  it('shows the codes the profile holds for Change codes…', () => {
    const html = renderToStaticMarkup(
      <CodesEntry
        session={1}
        initial={{ prompt: '<%hhp> ', fprompt: '' }}
        onRead={() => {}}
        onPoint={() => {}}
        onGameSent={() => {}}
      />,
    );
    expect(html).toContain('Is this your prompt setting?');
    expect(html).toContain('value="&lt;%hhp&gt; "');
    expect(html).not.toMatch(/<button[^>]*disabled=""[^>]*>Read these codes<\/button>/);
  });
});

describe('drawing off (P11)', () => {
  it('says the game prompt shows and offers to forget it', () => {
    const html = renderToStaticMarkup(
      <DrawOff name="Tester" other={false} confirming={false} onForget={() => {}} />,
    );
    expect(html).toContain('You see the game&#x27;s own prompt.');
    expect(html).toContain('Vosh keeps your design for Tester.');
    expect(html).toContain('st-button-danger');
    expect(html).toContain('Forget your game&#x27;s prompt');
    const other = renderToStaticMarkup(
      <DrawOff name="Tester" other confirming onForget={() => {}} />,
    );
    expect(other).toContain(
      'Vosh keeps your design for Tester and still reads the values in your prompt.',
    );
    expect(other).toContain('aria-expanded="true"');
  });
});

describe('where a card menu opens', () => {
  const viewport = { width: 1280, height: 800 };
  const button = { left: 492, top: 11, right: 520, bottom: 35 };

  it('opens More under its button with their right edges together', () => {
    expect(menuPosition(button, { width: 232, height: 85 }, 'below-end', viewport)).toEqual({
      left: 288,
      top: 39,
    });
  });

  it('opens Presets above its button with their left edges together', () => {
    const presets = { left: 100, top: 600, right: 190, bottom: 628 };
    expect(menuPosition(presets, { width: 468, height: 498.5 }, 'above-start', viewport)).toEqual({
      left: 100,
      top: 97.5,
    });
  });

  it('opens the Preview menu above its button with their right edges together', () => {
    // Preview: Low health before Done at the foot of a card at the
    // window's foot, its menu of four previews.
    const preview = { left: 360, top: 740, right: 492, bottom: 768 };
    expect(menuPosition(preview, { width: 160, height: 132 }, 'above-end', viewport)).toEqual({
      left: 332,
      top: 604,
    });
    // With no room above, it opens below, still on the button's right.
    const high = { left: 360, top: 40, right: 492, bottom: 68 };
    expect(menuPosition(high, { width: 160, height: 132 }, 'above-end', viewport)).toEqual({
      left: 332,
      top: 72,
    });
  });

  it('opens More styles above its button, clear of the Underline row below', () => {
    // Its four styles and four underline kinds at 30 px a row, the rule
    // between them, and the menu's padding.
    expect(MORE_STYLES_HEIGHT).toBe(8 * 30 + 13 + 12);
    // The Style row's More styles button, with the Underline row 36 px
    // under it while an underline is on.
    const more = { left: 360, top: 520, right: 452, bottom: 548 };
    const underlineRowTop = more.bottom + 8;
    const size = { width: 184, height: MORE_STYLES_HEIGHT };
    const menu = menuPosition(more, size, MORE_STYLES_PLACE, viewport);
    expect(menu.left).toBe(more.left);
    // Whole on screen, and all of it above the button.
    expect(menu.top).toBeGreaterThanOrEqual(8);
    expect(menu.top + MORE_STYLES_HEIGHT).toBeLessThanOrEqual(more.top);
    expect(menu.top + MORE_STYLES_HEIGHT).toBeLessThan(underlineRowTop);
  });

  it('opens on the other side of its button when its own has no room', () => {
    // At the window's foot, a menu meant to open below opens above, and
    // stays inside the window's right edge.
    const near = { left: 1200, top: 760, right: 1260, bottom: 790 };
    expect(menuPosition(near, { width: 208, height: 261 }, 'below-start', viewport)).toEqual({
      left: 1064,
      top: 495,
    });
  });

  it('opens More styles below its button in a window too short for it above', () => {
    // A 560 px window with the card scrolled, so the button sits 200 px
    // down. Above it the menu would cover the button, so it opens below.
    const short = { width: 1000, height: 560 };
    const more = { left: 360, top: 200, right: 452, bottom: 228 };
    const size = { width: 184, height: MORE_STYLES_HEIGHT };
    expect(menuPosition(more, size, MORE_STYLES_PLACE, short)).toEqual({ left: 360, top: 232 });
  });

  it('scrolls a menu with room on neither side, on the side with more', () => {
    // The window's least height, 360 px, with the button in the middle.
    const least = { width: 1000, height: 360 };
    const more = { left: 360, top: 140, right: 452, bottom: 168 };
    const size = { width: 184, height: MORE_STYLES_HEIGHT };
    const menu = menuPosition(more, size, MORE_STYLES_PLACE, least);
    // 180 px below against 128 above.
    expect(menu).toEqual({ left: 360, top: 172, maxHeight: 180 });
    // It never covers its button and ends 8 px from the window's foot.
    expect(menu.top).toBeGreaterThan(more.bottom);
    expect(menu.top + (menu.maxHeight ?? 0)).toBe(least.height - 8);
    // With more room above, it scrolls above and starts 8 px from the top.
    const low = { ...more, top: 220, bottom: 248 };
    expect(menuPosition(low, size, MORE_STYLES_PLACE, least)).toEqual({
      left: 360,
      top: 8,
      maxHeight: 208,
    });
  });
});

// The pointer and the arrow keys share one highlight in the name menu,
// so the row under the pointer takes the focus.
describe('the name menu', () => {
  it('gives the row under the pointer the focus', () => {
    type Props = { role?: string; onPointerMove?: (e: { currentTarget: unknown }) => void };
    const rows: Props[] = [];
    const walk = (node: ReactNode) => {
      if (Array.isArray(node)) node.forEach(walk);
      else if (isValidElement<Props & { children?: ReactNode }>(node)) {
        if (node.props.role?.startsWith('menuitem')) rows.push(node.props);
        walk(node.props.children);
      }
    };
    walk(
      NameGroup({
        first: true,
        choices: [{ name: 'hp', label: 'Health' }],
        current: '',
        onChoose: () => undefined,
      }),
    );
    expect(rows).toHaveLength(1);
    const doc: { activeElement: unknown } = { activeElement: null };
    vi.stubGlobal('document', doc);
    try {
      const row = {
        focus() {
          doc.activeElement = row;
        },
      };
      rows[0].onPointerMove?.({ currentTarget: row });
      expect(doc.activeElement).toBe(row);
    } finally {
      vi.unstubAllGlobals();
    }
  });
});
