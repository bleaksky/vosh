import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import frameCss from '../styles/frame.css?raw';
import sessionsCss from '../styles/sessions.css?raw';
import { AppShell } from './AppShell';
import { SessionsToggle } from './SessionsToggle';

// The one sessions toggle (Sessions toggle T1 to T3). It holds one spot
// in the frame's top left corner, wears the panel toggle's recipe, and
// says what a press does in its label and tooltip with the key.

type Platform = 'macos' | 'windows' | 'linux';

/** Stub the platform tag `platform` while `fn` draws. */
function on<T>(platform: Platform, fn: () => T): T {
  vi.stubGlobal('document', { documentElement: { dataset: { platform } } });
  try {
    return fn();
  } finally {
    vi.unstubAllGlobals();
  }
}

/** The toggle's button tag, pressed or not, on `platform`. */
function toggle(platform: Platform, pressed: boolean): string {
  const html = on(platform, () =>
    renderToStaticMarkup(<SessionsToggle pressed={pressed} onToggle={() => undefined} />),
  );
  return html.match(/<button [^>]*>/)?.[0] ?? '';
}

const attr = (tag: string, name: string) => tag.match(new RegExp(` ${name}="([^"]*)"`))?.[1];

/** The declarations of one rule in a sheet. */
function rule(css: string, selector: string): string {
  const at = css.indexOf(`${selector} {`);
  expect(at, selector).toBeGreaterThanOrEqual(0);
  return css.slice(at, css.indexOf('}', at));
}

/** One px value from a rule, the first of a shorthand like padding. */
function px(css: string, selector: string, property: string, index = 0): number {
  const found = rule(css, selector).match(new RegExp(`\\n\\s*${property}:\\s*([^;]+);`));
  expect(found, `${selector} ${property}`).not.toBeNull();
  const parts = (found?.[1] ?? '').trim().split(/\s+/);
  return Number.parseFloat(parts[Math.min(index, parts.length - 1)]);
}

describe('the sessions toggle', () => {
  it('says what a press does, with its key, as the panel toggle does', () => {
    expect(attr(toggle('macos', true), 'aria-label')).toBe('Hide sessions');
    expect(attr(toggle('macos', true), 'title')).toBe('Hide sessions (⌃⌘S)');
    expect(attr(toggle('macos', false), 'aria-label')).toBe('Show sessions');
    expect(attr(toggle('macos', false), 'title')).toBe('Show sessions (⌃⌘S)');
    expect(attr(toggle('windows', true), 'title')).toBe('Hide sessions (Ctrl+Shift+S)');
    expect(attr(toggle('linux', false), 'title')).toBe('Show sessions (Ctrl+Shift+S)');
  });

  it('reads in the secondary tone while the sidebar shows and the tertiary one while it hides', () => {
    expect(attr(toggle('macos', true), 'class')).toBe('shell-icon-button');
    expect(attr(toggle('macos', false), 'class')).toBe('shell-icon-button is-quiet');
  });

  it('wears no pressed state on top of its label', () => {
    expect(toggle('macos', true)).not.toContain('aria-pressed');
    expect(toggle('macos', false)).not.toContain('aria-pressed');
  });

  it('draws the sidebar glyph', () => {
    const html = on('macos', () =>
      renderToStaticMarkup(<SessionsToggle pressed onToggle={() => undefined} />),
    );
    expect(html).toContain('<svg');
  });
});

describe('the sessions toggle in the frame', () => {
  function frame(sidebar: boolean, overlay = false): string {
    return on('macos', () =>
      renderToStaticMarkup(
        <AppShell
          panelOpen
          panelWidth={300}
          onPanelWidth={() => undefined}
          sessions={sidebar ? <aside>rows</aside> : null}
          sessionsOverlay={overlay ? <aside>over</aside> : null}
          sessionsToggle={<SessionsToggle pressed={sidebar} onToggle={() => undefined} />}
          titleBand={null}
          terminal={null}
          input={null}
          statusLine={null}
          panel={null}
        />,
      ),
    );
  }

  it('comes first in the frame, over the sidebar or at the band', () => {
    const shown = frame(true);
    expect(shown).toMatch(/^<main [^>]*data-lead="sidebar"[^>]*><div class="shell-lead"><button/);
    const hidden = frame(false);
    expect(hidden).toMatch(/^<main [^>]*data-lead="band"[^>]*><div class="shell-lead"><button/);
  });

  it('has no spot with no toggle, as with one session', () => {
    const html = on('macos', () =>
      renderToStaticMarkup(
        <AppShell
          panelOpen
          panelWidth={300}
          onPanelWidth={() => undefined}
          titleBand={null}
          terminal={null}
          input={null}
          statusLine={null}
          panel={null}
        />,
      ),
    );
    expect(html).not.toContain('data-lead');
    expect(html).not.toContain('shell-lead');
  });

  it('puts the sidebar over the terminal at its own width with its line', () => {
    const html = frame(false, true);
    expect(html).toContain('<div class="shell-sessions-overlay" style="width:221px"><aside>over');
  });

  it('sits 8 past the traffic lights on macOS and 10 in elsewhere', () => {
    // The lights take the first 66, as the snoop window's band clears them.
    expect(px(frameCss, "[data-platform='macos'] .shell-lead", 'left')).toBe(74);
    expect(px(frameCss, '.shell-lead', 'left')).toBe(10);
    expect(px(frameCss, '.shell-lead', 'left')).toBe(px(frameCss, '.shell-band-actions', 'right'));
  });

  it('shares a line with New session at the sidebar top', () => {
    expect(px(frameCss, '.shell-lead', 'top')).toBe(
      px(sessionsCss, '.shell-sessions-actions', 'top'),
    );
    expect(px(frameCss, '.shell-lead', 'height')).toBe(
      px(frameCss, '.shell-icon-button', 'height'),
    );
  });

  it('stays clear of New session at the narrowest sidebar', () => {
    const right = px(frameCss, "[data-platform='macos'] .shell-lead", 'left') + 28;
    // The narrowest sidebar is 180 with its 1 px line, and New session
    // ends 17 in from its right edge.
    const plus = 181 - px(sessionsCss, '.shell-sessions-actions', 'right') - 28;
    expect(right).toBeLessThan(plus);
  });

  it('keeps the session button clear of it at the band on macOS', () => {
    const inset = px(
      frameCss,
      "[data-platform='macos'] .shell[data-lead='band'] .shell-band-title",
      'padding',
      1,
    );
    expect(inset).toBeGreaterThanOrEqual(74 + 28);
    // Elsewhere the band's own inset already clears it.
    expect(px(frameCss, '.shell-band-title', 'padding', 1)).toBeGreaterThanOrEqual(10 + 28);
  });

  it('sits over the sidebar that slides in over the terminal', () => {
    expect(px(frameCss, '.shell-lead', 'z-index')).toBeGreaterThan(
      px(sessionsCss, '.shell-sessions-overlay', 'z-index'),
    );
  });

  it('slides the sidebar in unless you asked for reduced motion', () => {
    expect(rule(sessionsCss, '.shell-sessions-overlay')).toContain(
      'animation: shell-sessions-slide',
    );
    const reduced = sessionsCss.slice(sessionsCss.lastIndexOf('prefers-reduced-motion'));
    expect(reduced).toContain('.shell-sessions-overlay {\n    animation: none;');
  });
});
