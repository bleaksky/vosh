import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { PANEL_WIDTH_MIN, PANEL_WIDTH_MIN_FRAMELESS } from '../../lib/paneLayout';
import { AppShell } from './AppShell';

// The width the frame draws the panel at. On Windows and Linux the
// title band carries the window controls, so the panel draws at least
// PANEL_WIDTH_MIN_FRAMELESS wide to keep them over it. A narrower saved
// width stays saved and draws at that floor. macOS draws it as saved.

type Platform = 'macos' | 'windows' | 'linux';

/** The frame as it draws on `platform` with a saved panel width. */
function draw(platform: Platform, panelWidth: number, panelOpen = true): string {
  vi.stubGlobal('document', { documentElement: { dataset: { platform } } });
  try {
    return renderToStaticMarkup(
      <AppShell
        panelOpen={panelOpen}
        panelWidth={panelWidth}
        onPanelWidth={() => undefined}
        titleBand={null}
        terminal={null}
        input={null}
        statusLine={null}
        panel={null}
      />,
    );
  } finally {
    vi.unstubAllGlobals();
  }
}

/** The custom properties the root publishes. */
function vars(html: string): Record<string, string> {
  const style = html.match(/<main [^>]*style="([^"]*)"/)?.[1] ?? '';
  return Object.fromEntries(
    style
      .split(';')
      .filter(Boolean)
      .map((d) => {
        const at = d.indexOf(':');
        return [d.slice(0, at), d.slice(at + 1)];
      }),
  );
}

/** The panel edge's width handle values. */
function handle(html: string): { min: number; now: number } {
  const tag = html.match(/<div role="separator"[^>]*>/)?.[0] ?? '';
  const read = (name: string) => Number(tag.match(new RegExp(` ${name}="([^"]*)"`))?.[1]);
  return { min: read('aria-valuemin'), now: read('aria-valuenow') };
}

const column = (px: number) => `min(${px}px, calc(100vw - 320px))`;

describe('the panel width the frame draws', () => {
  it('raises a narrower saved width to the floor on Windows and Linux', () => {
    expect(PANEL_WIDTH_MIN_FRAMELESS).toBe(248);
    for (const platform of ['windows', 'linux'] as const) {
      for (const saved of [PANEL_WIDTH_MIN, 220, 247]) {
        const html = draw(platform, saved);
        expect(vars(html), `${platform} ${saved}`).toEqual({
          '--panel-w': '248px',
          '--panel-col': column(248),
        });
        expect(handle(html)).toEqual({ min: 248, now: 248 });
      }
    }
  });

  it('draws the narrowest saved width as saved on macOS', () => {
    const html = draw('macos', PANEL_WIDTH_MIN);
    expect(vars(html)).toEqual({ '--panel-w': '200px', '--panel-col': column(200) });
    expect(handle(html)).toEqual({ min: PANEL_WIDTH_MIN, now: PANEL_WIDTH_MIN });
  });

  it('draws a width at or over the floor as saved on every platform', () => {
    for (const platform of ['macos', 'windows', 'linux'] as const) {
      for (const saved of [248, 300, 520]) {
        const html = draw(platform, saved);
        expect(vars(html), `${platform} ${saved}`).toEqual({
          '--panel-w': `${saved}px`,
          '--panel-col': column(saved),
        });
        expect(handle(html).now).toBe(saved);
      }
    }
  });

  it('keeps the floor in --panel-w while the panel is hidden', () => {
    expect(vars(draw('windows', PANEL_WIDTH_MIN, false))).toEqual({
      '--panel-w': '248px',
      '--panel-col': '0px',
    });
  });
});
