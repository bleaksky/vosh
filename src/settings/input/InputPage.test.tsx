import { act, createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { normalizeUiConfig, type UiConfig, type UiFields } from '../../ipc/uiConfig';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../../test/fakeDom';
import { findTheme } from '../../theme/themes';
import { InputPage } from './InputPage';

/** The terminal colors of the theme the page draws in. */
const EMBER = findTheme('obsidian-ember').xterm;

// The page saves each row through the one field writer, which these
// tests stand in for, so a test reads what each row hands it.
const saves = vi.hoisted(() => [] as UiFields[]);
vi.mock('../useSettingsAutoSave', () => ({
  useSettingsAutoSave: () => ({ update: (patch: UiFields) => saves.push(patch) }),
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

function configWith(fields: Partial<UiConfig> = {}): UiConfig {
  return {
    ...normalizeUiConfig({
      theme: 'obsidian-ember',
      auto_update: false,
      font_family: 'Menlo',
      font_size: 14,
      tracked_affects: [],
      enabled_presets: [],
    }),
    ...fields,
  };
}

function draw(fields: Partial<UiConfig> = {}): string {
  return renderToStaticMarkup(
    <InputPage
      target={{ group: 'input' }}
      navSeq={0}
      config={configWith(fields)}
      setConfig={() => undefined}
      onError={() => undefined}
      pathB={false}
      navigate={() => undefined}
      setLeaveGuard={() => undefined}
    />,
  );
}

const between = (html: string, from: string, to: string) =>
  html.slice(html.indexOf(from), html.indexOf(to));
const labels = (html: string) =>
  [...html.matchAll(/class="st-row-label"[^>]*>([^<]*)</g)].map((m) => m[1]);
const segments = (html: string) =>
  [...html.matchAll(/class="st-seg-item"[^>]*>([^<]*)</g)].map((m) => m[1]);
const pressed = (html: string) =>
  [...html.matchAll(/aria-pressed="true"[^>]*>([^<]*)</g)].map((m) => m[1]);

describe('InputPage', () => {
  it('splits Sent commands from Command line, in the order the board draws', () => {
    const html = draw();
    const sent = between(html, 'data-st-anchor="sent"', 'data-st-anchor="command-line"');
    expect(labels(sent)).toEqual([
      'Mark before your commands',
      'Mark color',
      'Command color',
      'Dim sent commands',
      'Use the same mark in the command line',
      'Show the commands your macros send',
    ]);
    const line = between(html, 'data-st-anchor="command-line"', 'data-st-anchor="writing"');
    expect(labels(line)).toEqual([
      'Caret shape',
      'Caret blinks',
      'Caret color',
      'Text color',
      'Background',
      'Size',
      'Color commands as you type',
      'Keep last command',
      'Check spelling when you chat',
    ]);
    expect(sent).toContain('Vosh leaves it out after a prompt that already ends in &gt;.');
    expect(sent).toContain('Your commands draw faint, so the game’s lines stand out.');
    expect(sent).toContain('The line you type in starts with your mark.');
  });

  it('offers Off, ›, > and Your own, with the field only for your own', () => {
    const chevron = draw();
    const mark = between(chevron, 'data-st-anchor="mark-commands"', 'data-st-anchor="mark-color"');
    expect(segments(mark)).toEqual(['Off', '›', '&gt;', 'Your own']);
    expect(pressed(mark)).toEqual(['›']);
    expect(mark).not.toContain('aria-label="Your own mark"');

    const own = draw({ input_echo_mark: 'own', input_echo_mark_text: 'you:' });
    const ownMark = between(own, 'data-st-anchor="mark-commands"', 'data-st-anchor="mark-color"');
    expect(pressed(ownMark)).toEqual(['Your own']);
    expect(ownMark).toMatch(
      /aria-label="Your own mark"[^>]*value="you:"|value="you:"[^>]*aria-label="Your own mark"/,
    );
  });

  it('shows the theme’s bright black as the mark’s Theme default', () => {
    const html = draw();
    const color = between(html, 'data-st-anchor="mark-color"', 'data-st-anchor="sent-color"');
    expect(color).toContain('placeholder="Theme default"');
    expect(color).toContain('#5f5a55');
  });

  it('draws dim off and the line mark on by default', () => {
    const html = draw();
    const checked = (anchor: string, next: string) =>
      between(html, `data-st-anchor="${anchor}"`, `data-st-anchor="${next}"`).includes(
        'checked=""',
      );
    expect(checked('sent-dim', 'mark-line')).toBe(false);
    expect(checked('mark-line', 'echo-macros')).toBe(true);
  });

  it('draws the caret blinking in the accent and the text in the terminal text', () => {
    const html = draw();
    expect(between(html, 'data-st-anchor="caret-blink"', 'data-st-anchor="caret-color"')).toContain(
      'checked=""',
    );
    const caret = between(html, 'data-st-anchor="caret-color"', 'data-st-anchor="line-color"');
    expect(caret).toContain('placeholder="Theme accent"');
    expect(caret).toContain('var(--accent)');
    const text = between(html, 'data-st-anchor="line-color"', 'data-st-anchor="line-bg"');
    expect(text).toContain('placeholder="Theme default"');
    expect(text).toContain(`background:${EMBER.foreground}`);
  });

  it('says what Slight tint does only on tint, and asks your color only on your own', () => {
    const tint = 'A touch of your theme’s accent, so the line stands apart from the game.';
    const own = 'aria-label="Your own background"';
    const row = (html: string) =>
      between(html, 'data-st-anchor="line-bg"', 'data-st-anchor="line-size"');

    const theme = row(draw());
    expect(segments(theme)).toEqual(['Theme', 'Slight tint', 'Your own']);
    expect(pressed(theme)).toEqual(['Theme']);
    expect(theme).not.toContain(tint);
    expect(theme).not.toContain(own);

    const tinted = row(draw({ input_line_background: 'tint' }));
    expect(pressed(tinted)).toEqual(['Slight tint']);
    expect(tinted).toContain(tint);
    expect(tinted).not.toContain(own);

    const yours = row(
      draw({ input_line_background: 'own', input_line_background_color: '#0f1a22' }),
    );
    expect(pressed(yours)).toEqual(['Your own']);
    expect(yours).not.toContain(tint);
    expect(yours).toMatch(/aria-label="Your own background"[^>]*value="#0f1a22"/);
    expect(yours).toContain('width:110px');
  });

  it('shows the four colors only while coloring is on, each on its theme color', () => {
    const line = (html: string) =>
      between(html, 'data-st-anchor="command-line"', 'data-st-anchor="writing"');
    const off = line(draw());
    expect(
      between(off, 'data-st-anchor="type-colors"', 'data-st-anchor="keep-last"'),
    ).not.toContain('checked=""');
    expect(off).toContain(
      'Aliases, Vosh commands, and chat each take a color, and a # command Vosh doesn’t know turns red.',
    );
    expect(labels(off)).not.toContain('Aliases');

    const on = line(draw({ input_type_colors: true }));
    const four = between(on, 'data-st-anchor="type-colors"', 'data-st-anchor="keep-last"');
    expect(four).toContain('checked=""');
    expect(labels(four)).toEqual([
      'Color commands as you type',
      'Aliases',
      'Vosh commands',
      'Chat',
      'A # command Vosh doesn’t know',
    ]);
    expect(four).toContain('Commands that start with #, like #walk.');
    expect(four).toContain('Say, tell, reply, and the channels.');
    const swatches = [...four.matchAll(/st-color-swatch" style="background:([^"]*)"/g)].map(
      (m) => m[1],
    );
    expect(swatches).toEqual([EMBER.cyan, EMBER.magenta, EMBER.yellow, 'var(--danger-text)']);
    expect(four.match(/placeholder="Theme default"/g)).toHaveLength(4);
  });

  it('offers Same as terminal first, then the sizes', () => {
    const size = between(draw(), 'data-st-anchor="line-size"', 'data-st-anchor="keep-last"');
    const options = [...size.matchAll(/<option[^>]*>([^<]*)</g)].map((m) => m[1]);
    expect(options).toEqual([
      'Same as terminal',
      '11 pt',
      '11.5 pt',
      '12 pt',
      '12.5 pt',
      '13 pt',
      '13.5 pt',
      '14 pt',
      '14.5 pt',
      '15 pt',
      '15.5 pt',
      '16 pt',
      '16.5 pt',
      '17 pt',
      '17.5 pt',
      '18 pt',
    ]);
    expect(size).toContain('width:180px');
    expect(size).toMatch(/<option value="0" selected="">/);
  });
});

describe('InputPage saves', () => {
  const doc = new FakeDocument();
  let createRoot: typeof import('react-dom/client').createRoot;
  const unmounts: (() => void)[] = [];

  beforeAll(async () => {
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    vi.stubGlobal('document', doc);
    vi.stubGlobal('window', {
      document: doc,
      location: { protocol: 'about:' },
      HTMLIFrameElement: class {},
      addEventListener() {},
      removeEventListener() {},
    });
    vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
    vi.stubGlobal('Node', FakeNode);
    vi.stubGlobal('Element', FakeElement);
    vi.stubGlobal('HTMLElement', FakeElement);
    ({ createRoot } = await import('react-dom/client'));
  });

  afterEach(() => {
    for (const unmount of unmounts.splice(0)) act(() => unmount());
    saves.length = 0;
  });

  afterAll(() => {
    vi.unstubAllGlobals();
  });

  /** Mount the page and hand back a way to work each row's control. */
  function mount(fields: Partial<UiConfig> = {}) {
    const container = doc.createElement('div');
    const root = createRoot(container as unknown as HTMLElement);
    act(() =>
      root.render(
        createElement(InputPage, {
          target: { group: 'input' },
          navSeq: 0,
          config: configWith(fields),
          setConfig: () => undefined,
          onError: () => undefined,
          pathB: false,
          navigate: () => undefined,
          setLeaveGuard: () => undefined,
        }),
      ),
    );
    unmounts.push(() => root.unmount());
    const inRow = (anchor: string, match: (el: FakeElement) => boolean) => {
      // A row with no anchor is found by its label.
      const row = findAll(
        container,
        (el) =>
          el.getAttribute('data-st-anchor') === anchor ||
          (el.getAttribute('class') === 'st-row' &&
            findAll(el, (label) => label.getAttribute('class') === 'st-row-label')[0]
              ?.textContent === anchor),
      )[0];
      const el = findAll(row, match)[0];
      const key = Object.keys(el).find((k) => k.startsWith('__reactProps$')) ?? '';
      return (el as unknown as Record<string, Record<string, (e: unknown) => void>>)[key];
    };
    return {
      flip: (anchor: string, on: boolean) =>
        act(() =>
          inRow(anchor, (el) => el.getAttribute('role') === 'switch').onChange({
            target: { checked: on },
          }),
        ),
      color: (anchor: string, hex: string) =>
        act(() =>
          inRow(anchor, (el) => el.getAttribute('type') === 'color').onChange({
            target: { value: hex },
          }),
        ),
      press: (anchor: string, label: string) =>
        act(() => inRow(anchor, (el) => el.textContent === label).onClick({})),
      choose: (anchor: string, value: string) =>
        act(() => inRow(anchor, (el) => el.tagName === 'SELECT').onChange({ target: { value } })),
    };
  }

  it('saves each look row to its own field', () => {
    const page = mount({ input_line_background: 'own' });
    page.flip('caret-blink', false);
    page.color('caret-color', '#c6a46a');
    page.color('line-color', '#d8dee9');
    page.press('line-bg', 'Slight tint');
    page.color('line-bg', '#0f1a22');
    page.choose('line-size', '16');
    expect(saves).toEqual([
      { input_caret_blink: false },
      { input_caret_color: '#c6a46a' },
      { input_line_color: '#d8dee9' },
      { input_line_background: 'tint' },
      { input_line_background_color: '#0f1a22' },
      { input_line_size: 16 },
    ]);
  });

  it('saves coloring and each of its colors to its own field', () => {
    const page = mount({ input_type_colors: true });
    page.flip('type-colors', false);
    page.color('Aliases', '#8abeb7');
    page.color('Vosh commands', '#b294bb');
    page.color('Chat', '#f0c674');
    page.color('A # command Vosh doesn’t know', '#cc6666');
    expect(saves).toEqual([
      { input_type_colors: false },
      { input_type_alias_color: '#8abeb7' },
      { input_type_hash_color: '#b294bb' },
      { input_type_chat_color: '#f0c674' },
      { input_type_unknown_color: '#cc6666' },
    ]);
  });
});
