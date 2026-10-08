import { describe, expect, it } from 'vitest';
import frameCss from '../styles/frame.css?raw';
import overlaysCss from '../styles/overlays.css?raw';
import panelCss from '../styles/panel.css?raw';
import tokensCss from '../styles/tokens.css?raw';
import {
  normalizePanelFont,
  PANEL_FONT_DESIGNED,
  PANEL_FONT_SYSTEM,
  PANEL_FONT_TERMINAL,
  panelFontFamily,
  panelFontList,
} from './panelFont';

describe('normalizePanelFont', () => {
  it('reads anything but a string as As designed', () => {
    for (const value of [undefined, null, 0, true, {}, []]) {
      expect(normalizePanelFont(value)).toBe(PANEL_FONT_DESIGNED);
    }
    expect(PANEL_FONT_DESIGNED).toBe('');
    expect(normalizePanelFont('')).toBe('');
    expect(normalizePanelFont('   ')).toBe('');
  });

  it('spells the terminal font and the system font one way each, as Rust saves them', () => {
    expect(normalizePanelFont('terminal')).toBe(PANEL_FONT_TERMINAL);
    expect(normalizePanelFont(' Terminal ')).toBe(PANEL_FONT_TERMINAL);
    expect(normalizePanelFont('system')).toBe(PANEL_FONT_SYSTEM);
    expect(normalizePanelFont(' System ')).toBe(PANEL_FONT_SYSTEM);
  });

  it('keeps a font list as written, a family named system or terminal included', () => {
    expect(normalizePanelFont(' "Iosevka", Menlo, monospace ')).toBe('"Iosevka", Menlo, monospace');
    expect(normalizePanelFont('"system", Menlo, monospace')).toBe('"system", Menlo, monospace');
    expect(normalizePanelFont('"Terminal", monospace')).toBe('"Terminal", monospace');
  });
});

describe('the face a window writes', () => {
  it('writes nothing under As designed, so each face keeps its own', () => {
    expect(panelFontFamily('')).toBeNull();
    expect(panelFontFamily(undefined)).toBeNull();
    expect(panelFontList('')).toBeNull();
  });

  it('takes the terminal face', () => {
    expect(panelFontFamily('terminal')).toBe('var(--font-mud)');
    expect(panelFontList('terminal')).toBeNull();
  });

  it('takes the system face the menus use', () => {
    expect(panelFontFamily('system')).toBe('var(--font-ui)');
    expect(panelFontList('system')).toBeNull();
  });

  it('takes a font you pick as the page renders the terminal font', () => {
    const iosevka = '"Iosevka", Menlo, monospace';
    expect(panelFontFamily(iosevka)).toBe(iosevka);
    expect(panelFontList(iosevka)).toBe(iosevka);
    // A retired bundled name draws in the bundled face, as the
    // terminal does.
    expect(panelFontFamily('"BerkeleyMono Bundled", monospace')).toBe(
      '"JetBrainsMono Bundled", monospace',
    );
  });
});

/** Every rule in `css` as its selector and body, comments dropped. A
 *  rule inside an at rule comes out on its own. */
function rules(css: string): { selector: string; body: string }[] {
  const bare = css.replace(/\/\*[\s\S]*?\*\//g, '');
  return [...bare.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
    selector: m[1].trim(),
    body: m[2],
  }));
}

/** Every sheet in src/styles by its path from src, in path order, so a
 *  new sheet joins the checks below without an edit here. */
const SHEETS: Readonly<Record<string, string>> = Object.fromEntries(
  Object.entries(
    import.meta.glob<string>('../styles/*.css', { query: '?raw', import: 'default', eager: true }),
  ).map(([path, css]) => [path.replace(/^\.\.\//, ''), css]),
);

/** A rule that draws inside a pane or the status line: a pane, the
 *  panel, the vitals, the map's drawing, or the status line. The menus
 *  float over the window in overlays.css and keep the system face. */
function drawsPaneText(selector: string): boolean {
  return /\.panel?\b|\.shell-status|\.map-|\.server-view/.test(selector);
}

const PANE_RULES = Object.entries(SHEETS).flatMap(([sheet, css]) =>
  rules(css)
    .filter((r) => drawsPaneText(r.selector))
    .map((r) => ({ ...r, sheet })),
);

/** The value of `property` in the first rule for exactly `selector`. */
function declared(css: string, selector: string, property: string): string | undefined {
  const rule = rules(css).find((r) => r.selector === selector);
  const m = rule && new RegExp(`(?:^|;)\\s*${property}\\s*:\\s*([^;]+);`).exec(rule.body);
  return m?.[1].trim();
}

/** The panel faces a pane or status line rule may name, each its own
 *  face under As designed, and all one face under any other pick. */
const PANEL_FACES = ['var(--font-panel)', 'var(--font-panel-game)', 'var(--font-panel-glyph)'];

/** The rules that name a face, as selector and face, in sheet order. */
function namedFaces(): string[] {
  return PANE_RULES.flatMap(({ selector, body }) =>
    [...body.matchAll(/(?:^|;)\s*font-family\s*:\s*([^;]+);/g)].map(
      (m) => `${selector}: ${m[1].trim()}`,
    ),
  );
}

describe('the panel faces in the stylesheets', () => {
  it('finds the pane and status line rules in the sheets', () => {
    expect(PANE_RULES.length).toBeGreaterThan(150);
    const sheets = new Set(PANE_RULES.map((r) => r.sheet));
    for (const sheet of ['styles/map.css', 'styles/frame.css', 'styles/panel.css']) {
      expect(sheets.has(sheet), sheet).toBe(true);
    }
  });

  it('defines each face from what the window writes, the face it was designed in without', () => {
    expect(declared(tokensCss, ':root', '--font-panel')).toBe(
      'var(--panel-font-family, var(--font-ui))',
    );
    expect(declared(tokensCss, ':root', '--font-panel-game')).toBe(
      'var(--panel-font-family, var(--font-mud))',
    );
    expect(declared(tokensCss, ':root', '--font-panel-glyph')).toBe(
      'var(--panel-font-family, var(--font-mono))',
    );
    expect(declared(tokensCss, ':root', '--font-panel-mark')).toBe(
      'var(--panel-font-family, monospace)',
    );
  });

  it('sets the panes and the status line in the panel face, and the game text in the game face', () => {
    expect(declared(panelCss, '.panel-host', 'font-family')).toBe('var(--font-panel)');
    expect(declared(frameCss, '.shell-statusline', 'font-family')).toBe('var(--font-panel)');
    // The faces one-window drew each in before the Panel font: the
    // system face from the panel, the terminal face for the game text,
    // and the bundled monospace face for the glyph map. The Text style
    // of the vitals draws your text in the game face too, in the footer
    // and on the status line.
    expect(namedFaces()).toEqual([
      '.pane-affect-hours: var(--font-panel-game)',
      '.pane-affect-name: var(--font-panel-game)',
      '.pane-countdown-line: var(--font-panel-game)',
      '.pane-chip: var(--font-panel-game)',
      '.shell-statusline: var(--font-panel)',
      '.shell-statusline > .shell-status-text: var(--font-panel-game)',
      '.map-glyph-grid: var(--font-panel-glyph)',
      '.panel-host: var(--font-panel)',
      '.pane-lua-line: var(--font-panel-game)',
      '.pane-chat-log: var(--font-panel-game)',
      '.panel-vitals-text: var(--font-panel-game)',
    ]);
  });

  it('names no other face in any pane or status line rule', () => {
    const named = PANE_RULES.flatMap(({ sheet, selector, body }) =>
      [...body.matchAll(/(?:--font-ui|--font-mud(?!-px)|--font-mono|--app-font-family)\b/g)].map(
        (m) => `${sheet} ${selector}: ${m[0]}`,
      ),
    );
    expect(named).toEqual([]);
  });

  it('gives each pane rule a panel face or the face around it, never a family', () => {
    const faces = PANE_RULES.flatMap(({ sheet, selector, body }) =>
      [...body.matchAll(/(?:^|;)\s*(font-family|font)\s*:\s*([^;]+);/g)]
        .map((m) => ({ property: m[1], value: m[2].trim() }))
        .filter(({ value }) => !PANEL_FACES.includes(value) && value !== 'inherit')
        .map(({ property, value }) => `${sheet} ${selector}: ${property}: ${value}`),
    );
    expect(faces).toEqual([]);
  });

  it('keeps the pane menus in the system face, like every other menu', () => {
    expect(declared(overlaysCss, '.menu', 'font-family')).toBe('var(--font-ui)');
  });
});
