import { afterEach, describe, expect, it, vi } from 'vitest';
import { THEME_PAINT_KEY, type ThemePaint } from './theme/themePaint';
import mainSource from './main.tsx?raw';
import prepaintSource from './prepaint.ts?raw';

// Startup paints the cached theme before React renders, so a window
// opening on a light theme never shows the dark stylesheet defaults.

const { render } = vi.hoisted(() => ({ render: vi.fn() }));

vi.mock('react-dom/client', () => {
  const createRoot = () => ({ render });
  return { default: { createRoot }, createRoot };
});
vi.mock('./shell/MainWindow', () => ({ default: () => null }));
vi.mock('./settings/SettingsWindow', () => ({ SettingsWindow: () => null }));
vi.mock('./help/HelpWindow', () => ({ HelpWindow: () => null }));
vi.mock('./shell/SnoopWindow', () => ({ SnoopWindow: () => null }));

const paint: ThemePaint = {
  v: 1,
  follow: true,
  light: {
    id: 'rubric',
    appearance: 'light',
    vars: { '--bg': '#f4efe4', '--xterm-bg': '#f4efe4' },
  },
  dark: {
    id: 'tokyo-night',
    appearance: 'dark',
    vars: { '--bg': '#1a1b26', '--xterm-bg': '#1a1b26' },
  },
};

afterEach(() => {
  vi.unstubAllGlobals();
  vi.resetModules();
  render.mockReset();
});

/** Stub the page a window loads at `search`, with the cached paint and
 *  a light OS, and hand back what the startup paint set. */
function stubPage(search: string) {
  const attrs: Record<string, string> = {};
  const vars: Record<string, string> = {};
  const documentElement = {
    dataset: {} as Record<string, string>,
    setAttribute: (name: string, value: string) => {
      attrs[name] = value;
    },
    style: {
      setProperty: (name: string, value: string) => {
        vars[name] = value;
      },
    },
  };
  vi.stubGlobal('document', {
    documentElement,
    getElementById: () => ({}),
  });
  const stored: Record<string, string> = { [THEME_PAINT_KEY]: JSON.stringify(paint) };
  vi.stubGlobal('window', {
    addEventListener: () => {},
    location: { search },
    // A light OS.
    matchMedia: (query: string) => ({ matches: !query.includes('dark') }),
    localStorage: {
      getItem: (key: string) => stored[key] ?? null,
      setItem: (key: string, value: string) => {
        stored[key] = value;
      },
    },
  });
  vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (Macintosh; Mac OS X 15_0)' });

  return { attrs, vars };
}

describe('startup', () => {
  it('paints the cached theme before React renders', async () => {
    const { attrs, vars } = stubPage('?view=settings');
    let atRender: { appearance?: string; theme?: string; bg?: string } | null = null;
    render.mockImplementation(() => {
      atRender = {
        appearance: attrs['data-appearance'],
        theme: attrs['data-theme'],
        bg: vars['--bg'],
      };
    });

    await import('./main');

    expect(render).toHaveBeenCalledTimes(1);
    expect(atRender).toEqual({ appearance: 'light', theme: 'rubric', bg: '#f4efe4' });
  });

  it('imports the startup paint before anything else', () => {
    const firstImport = mainSource.split('\n').find((line) => line.startsWith('import '));
    expect(firstImport).toBe("import './prepaint';");
    expect(prepaintSource).toMatch(/^prepaintTheme\(\);$/m);
  });

  it('renders the snoop window for the session its address names', async () => {
    stubPage('?view=snoop&session=3');
    await import('./main');
    const { SnoopWindow } = await import('./shell/SnoopWindow');
    expect(render).toHaveBeenCalledTimes(1);
    const shown = render.mock.calls[0][0] as { type: unknown; props: { session: number } };
    expect(shown.type).toBe(SnoopWindow);
    expect(shown.props.session).toBe(3);
  });
});
