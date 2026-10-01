import { afterEach, describe, expect, it, vi } from 'vitest';
import { THEME_PAINT_KEY, type ThemePaint } from './lib/themePaint';
import mainSource from './main.tsx?raw';
import prepaintSource from './prepaint.ts?raw';

// Startup paints the cached theme before React renders, so a window
// opening on a light theme never shows the dark stylesheet defaults.

const { render } = vi.hoisted(() => ({ render: vi.fn() }));

vi.mock('react-dom/client', () => {
  const createRoot = () => ({ render });
  return { default: { createRoot }, createRoot };
});
vi.mock('./App', () => ({ default: () => null }));
vi.mock('./SettingsApp', () => ({ SettingsApp: () => null }));
vi.mock('./HelpApp', () => ({ HelpApp: () => null }));

const paint: ThemePaint = {
  v: 1,
  follow: true,
  light: {
    id: 'vellum',
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

describe('startup', () => {
  it('paints the cached theme before React renders', async () => {
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
      location: { search: '?view=settings' },
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
    expect(atRender).toEqual({ appearance: 'light', theme: 'vellum', bg: '#f4efe4' });
  });

  it('imports the startup paint before anything else', () => {
    const firstImport = mainSource.split('\n').find((line) => line.startsWith('import '));
    expect(firstImport).toBe("import './prepaint';");
    expect(prepaintSource).toMatch(/^prepaintTheme\(\);$/m);
  });
});
