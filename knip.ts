import type { KnipConfig } from 'knip';

// Knip reads no CSS on its own. This turns each @import line into a script
// import, so the font package that src/styles/index.css pulls in counts as used.
const cssImport = /@import\s+(['"][^'"]+['"])/g;

const config: KnipConfig = {
  compilers: {
    css: (text: string) =>
      Array.from(text.matchAll(cssImport), ([, spec]) => `import ${spec};`).join('\n'),
  },
};

export default config;
