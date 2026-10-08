import { readFileSync } from 'node:fs';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { readHelp } from './src/help/readHelp';

const host = process.env.TAURI_DEV_HOST;
const pkg: { version: string } = JSON.parse(
  readFileSync(new URL('./package.json', import.meta.url), 'utf8'),
);

export default defineConfig(async () => ({
  plugins: [
    react(),
    // The Help window reads HELP.md as it loads. Reading it here first
    // turns a topic with no id line, or an id used twice, into a failed
    // build instead of a Help window that throws.
    {
      name: 'vosh-help-check',
      buildStart() {
        readHelp(readFileSync(new URL('./HELP.md', import.meta.url), 'utf8'));
      },
    },
    // Dev-only diagnostics sink. WKWebView gives a dev run no console,
    // so a page or a scripted check can POST text here and it lands in
    // the tauri dev log where it can actually be read.
    {
      name: 'vosh-dbg-sink',
      configureServer(server: {
        middlewares: {
          use: (
            path: string,
            fn: (
              req: { on: (ev: string, cb: (c?: unknown) => void) => void },
              res: { statusCode: number; end: () => void },
            ) => void,
          ) => void;
        };
      }) {
        server.middlewares.use('/__vosh-dbg', (req, res) => {
          let body = '';
          req.on('data', (c) => {
            body += String(c);
          });
          req.on('end', () => {
            // eslint-disable-next-line no-console
            console.log(`[vosh-dbg] ${body.slice(0, 600)}`);
            res.statusCode = 204;
            res.end();
          });
        });
      },
    },
  ],
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  // Agent worktrees live under .claude/ inside the repo. Their copies
  // of the tests are not this checkout's tests.
  test: {
    exclude: ['**/node_modules/**', '**/dist/**', '.claude/**'],
    // Vitest hands back CSS as an empty string unless a pattern here
    // matches it. Tests read the sheets under src/styles as text, so
    // this one pattern returns every sheet whole, a new one included.
    css: {
      include: [/src\/styles\//],
    },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: 'ws',
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  build: {
    target: process.env.TAURI_ENV_PLATFORM === 'windows' ? 'chrome105' : 'safari13',
    minify: !process.env.TAURI_ENV_DEBUG ? 'esbuild' : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
}));
