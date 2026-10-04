import { describe, expect, it } from 'vitest';
import baseCss from '../styles.css?raw';
import frameCss from '../styles/frame.css?raw';
import helpCss from '../styles/help.css?raw';
import overlaysCss from '../styles/overlays.css?raw';
import panelCss from '../styles/panel.css?raw';
import promptCss from '../styles/prompt.css?raw';
import settingsCss from '../styles/settings.css?raw';
import tokensCss from '../styles/tokens.css?raw';
import {
  normalizePanelFont,
  PANEL_FONT_SYSTEM,
  PANEL_FONT_TERMINAL,
  panelFontFamily,
  panelFontList,
} from './panelFont';

describe('normalizePanelFont', () => {
  it('reads anything but a string as the terminal font', () => {
    for (const value of [undefined, null, 0, true, {}, []]) {
      expect(normalizePanelFont(value)).toBe(PANEL_FONT_TERMINAL);
    }
    expect(normalizePanelFont('')).toBe('');
    expect(normalizePanelFont('   ')).toBe('');
  });

  it('spells the system font one way, as Rust saves it', () => {
    expect(normalizePanelFont('system')).toBe(PANEL_FONT_SYSTEM);
    expect(normalizePanelFont(' System ')).toBe(PANEL_FONT_SYSTEM);
  });

  it('keeps a font list as written, a family named system included', () => {
    expect(normalizePanelFont(' "Iosevka", Menlo, monospace ')).toBe('"Iosevka", Menlo, monospace');
    expect(normalizePanelFont('"system", Menlo, monospace')).toBe('"system", Menlo, monospace');
  });
});

describe('the face a window writes', () => {
  it('follows the terminal face until you pick another', () => {
    expect(panelFontFamily('')).toBe('var(--font-mud)');
    expect(panelFontFamily(undefined)).toBe('var(--font-mud)');
    expect(panelFontList('')).toBeNull();
  });

  it('takes the system face the menus use', () => {
    expect(panelFontFamily('system')).toBe('var(--font-ui)');
    expect(panelFontList('system')).toBeNull();
  });

  it('takes a font you pick as the page renders the terminal font', () => {
    const iosevka = '"Iosevka", Menlo, monospace';
    expect(panelFontFamily(iosevka)).toBe(iosevka);
    expect(panelFontList(iosevka)).toBe(iosevka);
    // A Berkeley Mono list falls back to the bundled face, as the
    // terminal does.
    expect(panelFontFamily('"Berkeley Mono", monospace')).toBe(
      '"Berkeley Mono", "JetBrainsMono Bundled", monospace',
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

const SHEETS: Readonly<Record<string, string>> = {
  'styles.css': baseCss,
  'styles/tokens.css': tokensCss,
  'styles/frame.css': frameCss,
  'styles/panel.css': panelCss,
  'styles/overlays.css': overlaysCss,
  'styles/settings.css': settingsCss,
  'styles/prompt.css': promptCss,
  'styles/help.css': helpCss,
};

/** A rule that draws inside a pane or the status line: a pane, the
 *  panel, the vitals, the map's drawing, or the status line. The pane
 *  menus float over the window and keep the system face. */
function drawsPaneText(selector: string): boolean {
  if (/\.pane-menu/.test(selector)) return false;
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

describe('one panel face in the stylesheets', () => {
  it('finds the pane and status line rules in the sheets', () => {
    expect(PANE_RULES.length).toBeGreaterThan(150);
    const sheets = new Set(PANE_RULES.map((r) => r.sheet));
    for (const sheet of ['styles.css', 'styles/frame.css', 'styles/panel.css']) {
      expect(sheets.has(sheet), sheet).toBe(true);
    }
  });

  it('defines the face from what the window writes, the terminal face until then', () => {
    expect(declared(tokensCss, ':root', '--font-panel')).toBe(
      'var(--panel-font-family, var(--font-mud))',
    );
  });

  it('sets the panes and the status line in the panel face', () => {
    expect(declared(panelCss, '.panel-host', 'font-family')).toBe('var(--font-panel)');
    expect(declared(frameCss, '.shell-statusline', 'font-family')).toBe('var(--font-panel)');
  });

  it('names no other face in any pane or status line rule', () => {
    const named = PANE_RULES.flatMap(({ sheet, selector, body }) =>
      [
        ...body.matchAll(
          /(?:--font-ui|--font-mud(?!-px)|--font-mono|--font-chrome|--app-font-family)\b/g,
        ),
      ].map((m) => `${sheet} ${selector}: ${m[0]}`),
    );
    expect(named).toEqual([]);
  });

  it('gives each pane rule the panel face or the face around it, never a family', () => {
    const faces = PANE_RULES.flatMap(({ sheet, selector, body }) =>
      [...body.matchAll(/(?:^|;)\s*(font-family|font)\s*:\s*([^;]+);/g)]
        .map((m) => ({ property: m[1], value: m[2].trim() }))
        .filter(({ value }) => value !== 'var(--font-panel)' && value !== 'inherit')
        .map(({ property, value }) => `${sheet} ${selector}: ${property}: ${value}`),
    );
    expect(faces).toEqual([]);
  });

  it('keeps the pane menus in the system face, like every other menu', () => {
    expect(declared(panelCss, '.pane-menu', 'font-family')).toBe('var(--font-ui)');
  });
});
