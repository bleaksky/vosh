// Minimal SGR (ANSI Select Graphic Rendition) parser. Converts a byte
// stream that may contain ESC [ ... m sequences into a list of styled
// chunks suitable for rendering as React spans. Covers the codes MUDs
// actually use: bold, underline, the basic 8/16 fg+bg palette, the
// 256-color extension, and reset.

export interface AnsiStyle {
  fg?: string;
  bg?: string;
  bold?: boolean;
  underline?: boolean;
  inverse?: boolean;
}

export interface AnsiChunk {
  text: string;
  style: AnsiStyle;
}

const PALETTE_8: Record<number, string> = {
  0: '#0d1117',
  1: '#cf222e',
  2: '#1a7f37',
  3: '#9a6700',
  4: '#0969da',
  5: '#8250df',
  6: '#1b7c83',
  7: '#c6cdd5',
};

const PALETTE_BRIGHT: Record<number, string> = {
  0: '#57606a',
  1: '#ff8182',
  2: '#56d364',
  3: '#e3b341',
  4: '#79c0ff',
  5: '#d2a8ff',
  6: '#56d4dd',
  7: '#f0f6fc',
};

function rgbForAnsi256(n: number): string {
  if (n < 16) {
    const lookup = n < 8 ? PALETTE_8 : PALETTE_BRIGHT;
    return lookup[n - (n < 8 ? 0 : 8)] ?? '#c6cdd5';
  }
  if (n < 232) {
    const i = n - 16;
    const r = Math.floor(i / 36);
    const g = Math.floor((i % 36) / 6);
    const b = i % 6;
    const v = (c: number) => (c === 0 ? 0 : 55 + c * 40);
    return `rgb(${v(r)},${v(g)},${v(b)})`;
  }
  const gray = 8 + (n - 232) * 10;
  return `rgb(${gray},${gray},${gray})`;
}

function applyCodes(prev: AnsiStyle, codes: number[]): AnsiStyle {
  const next: AnsiStyle = { ...prev };
  let i = 0;
  while (i < codes.length) {
    const code = codes[i];
    if (code === undefined || code === 0) {
      Object.keys(next).forEach((key) => {
        delete (next as Record<string, unknown>)[key];
      });
      i += 1;
      continue;
    }
    if (code === 1) {
      next.bold = true;
      i += 1;
      continue;
    }
    if (code === 4) {
      next.underline = true;
      i += 1;
      continue;
    }
    if (code === 7) {
      next.inverse = true;
      i += 1;
      continue;
    }
    if (code === 22) {
      next.bold = false;
      i += 1;
      continue;
    }
    if (code === 24) {
      next.underline = false;
      i += 1;
      continue;
    }
    if (code === 27) {
      next.inverse = false;
      i += 1;
      continue;
    }
    if (code >= 30 && code <= 37) {
      next.fg = PALETTE_8[code - 30];
      i += 1;
      continue;
    }
    if (code === 39) {
      delete next.fg;
      i += 1;
      continue;
    }
    if (code >= 40 && code <= 47) {
      next.bg = PALETTE_8[code - 40];
      i += 1;
      continue;
    }
    if (code === 49) {
      delete next.bg;
      i += 1;
      continue;
    }
    if (code >= 90 && code <= 97) {
      next.fg = PALETTE_BRIGHT[code - 90];
      i += 1;
      continue;
    }
    if (code >= 100 && code <= 107) {
      next.bg = PALETTE_BRIGHT[code - 100];
      i += 1;
      continue;
    }
    if (code === 38 || code === 48) {
      const target: 'fg' | 'bg' = code === 38 ? 'fg' : 'bg';
      const mode = codes[i + 1];
      if (mode === 5 && codes[i + 2] !== undefined) {
        next[target] = rgbForAnsi256(codes[i + 2]!);
        i += 3;
        continue;
      }
      if (mode === 2 && codes[i + 4] !== undefined) {
        next[target] = `rgb(${codes[i + 2]},${codes[i + 3]},${codes[i + 4]})`;
        i += 5;
        continue;
      }
      i += 1;
      continue;
    }
    i += 1;
  }
  return next;
}

const decoder = new TextDecoder('utf-8', { fatal: false });

export function parseAnsi(input: Uint8Array | string): AnsiChunk[] {
  const text = typeof input === 'string' ? input : decoder.decode(input);
  const out: AnsiChunk[] = [];
  let style: AnsiStyle = {};
  let buf = '';
  let i = 0;
  while (i < text.length) {
    const ch = text[i];
    if (ch === '\x1b' && text[i + 1] === '[') {
      if (buf) {
        out.push({ text: buf, style: { ...style } });
        buf = '';
      }
      i += 2;
      let params = '';
      while (i < text.length) {
        const c = text[i];
        if (c === undefined) break;
        if (c >= '@' && c <= '~') {
          break;
        }
        params += c;
        i += 1;
      }
      const final = text[i];
      i += 1;
      if (final !== 'm') {
        // Non-SGR escape (cursor moves, line clears). Strip and continue.
        continue;
      }
      const codes = params
        .split(';')
        .map((s) => (s === '' ? 0 : Number(s)))
        .filter((n) => Number.isFinite(n));
      style = applyCodes(style, codes);
    } else {
      buf += ch;
      i += 1;
    }
  }
  if (buf) {
    out.push({ text: buf, style });
  }
  return out;
}

export function styleToCss(style: AnsiStyle): React.CSSProperties {
  const css: React.CSSProperties = {};
  if (style.inverse) {
    if (style.fg) css.background = style.fg;
    if (style.bg) css.color = style.bg;
  } else {
    if (style.fg) css.color = style.fg;
    if (style.bg) css.background = style.bg;
  }
  if (style.bold) css.fontWeight = 'bold';
  if (style.underline) css.textDecoration = 'underline';
  return css;
}
