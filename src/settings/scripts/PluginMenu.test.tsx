import type { ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { PluginMenu } from './PluginMenu';

// A plugin row's more menu as markup, its rows drawn in place with no
// page to portal into.

vi.mock('../../ui/MenuSurface', async (actual) => ({
  ...(await actual<typeof import('../../ui/MenuSurface')>()),
  MenuSurface: ({ label, children }: { label: string; children: ReactNode }) => (
    <menu aria-label={label}>{children}</menu>
  ),
}));

const none = () => undefined;

/** The menu's label and its rows, a separator as `—`, on `platform`. */
function rows(platform: string) {
  vi.stubGlobal('document', { documentElement: { dataset: { platform } } });
  const html = renderToStaticMarkup(
    <PluginMenu
      name="vitals_alert"
      at={{ x: 0, y: 0 }}
      anchor={null}
      onClose={none}
      onReload={none}
      onReveal={none}
      onExport={none}
      onRemove={none}
    />,
  );
  return {
    label: /<menu aria-label="([^"]*)">/.exec(html)?.[1],
    rows: [...html.matchAll(/class="menu-label">([^<]*)<|role="separator"/g)].map(
      ([, text]) => text ?? '—',
    ),
  };
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('the plugin menu', () => {
  it('reloads, shows, exports and, under a separator, removes the plugin', () => {
    expect(rows('macos')).toEqual({
      label: 'vitals_alert options',
      rows: ['Reload', 'Show in Finder', 'Export to Downloads', '—', 'Remove…'],
    });
  });

  it('names the file manager of each platform', () => {
    expect(rows('windows').rows[1]).toBe('Show in Explorer');
    expect(rows('linux').rows[1]).toBe('Show the folder');
  });
});
