import { readFileSync } from 'node:fs';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

const host = process.env.TAURI_DEV_HOST;
const pkg: { version: string } = JSON.parse(
  readFileSync(new URL('./package.json', import.meta.url), 'utf8'),
);

export default defineConfig(async () => ({
  plugins: [
    react(),
    // Dev-only diagnostics sink for the WKWebView blank-chrome wedge:
    // the webview keeps running JS while painting nothing, so the page
    // POSTs its state here and it lands in the tauri dev log where it
    // can actually be read.
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
