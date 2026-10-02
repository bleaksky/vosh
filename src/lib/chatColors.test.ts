import { describe, expect, it } from 'vitest';
import { ANSI_SLOTS } from './baseAnsi';
import {
  CHAT_CHANNELS,
  CHAT_CONTRAST,
  CHAT_TAG_OPACITY,
  chatChannelColor,
  chatChannelSlot,
  chatColorChoices,
  chatInks,
  normalizeChatColors,
  sameChatColors,
} from './chatColors';
import { composite, contrast, parseHex, rgbToOklch, type Rgb } from './color';
import { BUILTIN_THEMES, findTheme, themeTokens } from './themes';

const kanso = findTheme('kanso-zen').xterm;
const vellum = findTheme('vellum').xterm;

const hex = (h: string): Rgb => {
  const c = parseHex(h);
  if (!c) throw new Error(`not hex ${h}`);
  return c;
};

describe('chatChannelSlot', () => {
  // The slot each channel's message prints in on Aabahran (comm.c color
  // codes). The game sends bold in the bright slot, so a bold code maps
  // to its bright slot.
  it('gives each of the eleven Comm.Channel channels the slot the game prints it in', () => {
    expect({
      say: chatChannelSlot('say'),
      tell: chatChannelSlot('tell'),
      gtell: chatChannelSlot('gtell'),
      yell: chatChannelSlot('yell'),
      pray: chatChannelSlot('pray'),
      cabal: chatChannelSlot('cabal'),
      clan: chatChannelSlot('clan'),
      faction: chatChannelSlot('faction'),
      newbie: chatChannelSlot('newbie'),
      immortal: chatChannelSlot('immortal'),
      imp: chatChannelSlot('imp'),
    }).toEqual({
      say: 'brightYellow',
      tell: 'green',
      gtell: 'brightMagenta',
      yell: 'cyan',
      pray: 'brightWhite',
      cabal: 'brightBlue',
      clan: 'brightCyan',
      faction: 'yellow',
      newbie: 'brightGreen',
      immortal: 'brightRed',
      imp: 'brightCyan',
    });
  });

  it('reads a channel in any case and with stray spaces', () => {
    expect(chatChannelSlot(' Tell ')).toBe('green');
    expect(chatChannelSlot('IMMORTAL')).toBe('brightRed');
  });

  it('reads the words other games use for the same channels', () => {
    expect(chatChannelSlot('tells')).toBe(chatChannelSlot('tell'));
    expect(chatChannelSlot('says')).toBe(chatChannelSlot('say'));
    expect(chatChannelSlot('group')).toBe(chatChannelSlot('gtell'));
    expect(chatChannelSlot('shout')).toBe(chatChannelSlot('yell'));
  });

  it('gives any other pane a colored slot of its own that stays put', () => {
    const quiet = ['black', 'white', 'brightBlack', 'brightWhite', 'red', 'brightRed'];
    for (const pane of ['chat', 'auction', 'ooc', 'loot', 'constructor', '__proto__', '']) {
      const slot = chatChannelSlot(pane);
      expect(quiet, pane).not.toContain(slot);
      expect(chatChannelSlot(pane), pane).toBe(slot);
    }
  });
});

describe('chatChannelColor', () => {
  it('paints each channel in the theme slot, not a fixed pastel', () => {
    expect(chatChannelColor('say', kanso)).toBe('#e6c384');
    expect(chatChannelColor('tell', kanso)).toBe('#8a9a7b');
    expect(chatChannelColor('gtell', kanso)).toBe('#938aa9');
    expect(chatChannelColor('yell', kanso)).toBe('#8ea4a2');
    expect(chatChannelColor('pray', kanso)).toBe('#c5c9c7');
    expect(chatChannelColor('cabal', kanso)).toBe('#7fb4ca');
    expect(chatChannelColor('clan', kanso)).toBe('#7aa89f');
    expect(chatChannelColor('faction', kanso)).toBe('#c4b28a');
    expect(chatChannelColor('newbie', kanso)).toBe('#87a987');
    expect(chatChannelColor('immortal', kanso)).toBe('#e46876');
    expect(chatChannelColor('imp', kanso)).toBe('#7aa89f');
  });

  it('follows the theme', () => {
    expect(chatChannelColor('say', vellum)).toBe('#b88226');
    expect(chatChannelColor('tell', vellum)).toBe('#4f7a3a');
    expect(chatChannelColor('immortal', vellum)).toBe('#c2574a');
  });
});

describe('chatInks', () => {
  it('reads at 3:1 on the panel, tag included, for every color a channel can take on every built in theme', () => {
    for (const theme of BUILTIN_THEMES) {
      const tokens = themeTokens(theme);
      const panel = hex(tokens.panel);
      const inks = chatInks(theme.xterm, tokens);
      for (const slot of ANSI_SLOTS) {
        const { color, fadeTag } = inks[slot];
        const label = `${theme.id} ${slot}`;
        expect(contrast(hex(color), panel), label).toBeGreaterThanOrEqual(CHAT_CONTRAST);
        // The tag draws at CHAT_TAG_OPACITY over the panel, or solid.
        const tag = fadeTag ? composite(hex(color), panel, CHAT_TAG_OPACITY) : hex(color);
        expect(contrast(tag, panel), `${label} tag`).toBeGreaterThanOrEqual(CHAT_CONTRAST);
      }
    }
  });

  it('moves only lightness, and only for a color that falls short', () => {
    for (const theme of BUILTIN_THEMES) {
      const tokens = themeTokens(theme);
      const panel = hex(tokens.panel);
      const inks = chatInks(theme.xterm, tokens);
      for (const slot of ANSI_SLOTS) {
        const label = `${theme.id} ${slot}`;
        const was = hex(theme.xterm[slot]);
        const now = hex(inks[slot].color);
        if (contrast(was, panel) >= CHAT_CONTRAST) {
          expect(now, label).toEqual(was);
          continue;
        }
        // The hue holds. Chroma holds too, or gives way only where the
        // new lightness leaves sRGB. A near gray's hue can swing a degree
        // or two on the rounding to whole sRGB values, so the hue check
        // starts at chroma 0.05.
        const a = rgbToOklch(was);
        const b = rgbToOklch(now);
        expect(b.C, label).toBeLessThan(a.C + 0.003);
        if (a.C >= 0.05) {
          expect(Math.abs(((b.h - a.h + 540) % 360) - 180), label).toBeLessThan(1);
        }
        // Lighter on a dark panel, darker on a light one.
        expect(Math.sign(b.L - a.L), label).toBe(tokens.appearance === 'dark' ? 1 : -1);
      }
    }
  });

  it('gives up chroma, not hue, where the lift leaves sRGB', () => {
    // Everforest yellow has to darken past the edge of sRGB to read at
    // 3:1 on a cream panel. A clamp per channel would turn it orange.
    const yellow = '#dfa000';
    const panel = '#f6efdc';
    const ink = chatInks({ ...vellum, yellow }, { panel, appearance: 'light' }).yellow;
    const was = rgbToOklch(hex(yellow));
    const now = rgbToOklch(hex(ink.color));
    expect(contrast(hex(ink.color), hex(panel))).toBeGreaterThanOrEqual(CHAT_CONTRAST);
    expect(Math.abs(now.h - was.h)).toBeLessThan(1);
    expect(now.C).toBeLessThan(was.C - 0.01);
  });

  it('lifts the published colors that fade on a panel, and leaves the theme alone', () => {
    const cases = [
      ['vellum', 'brightYellow'],
      ['solarized-light', 'cyan'],
      ['solarized-dark', 'red'],
      ['tango-dark', 'blue'],
      ['classic-vivid', 'blue'],
      ['everforest-light', 'yellow'],
      ['everforest-light', 'red'],
    ] as const;
    for (const [id, slot] of cases) {
      const theme = findTheme(id);
      const before = { ...theme.xterm };
      const ink = chatInks(theme.xterm, themeTokens(theme))[slot];
      expect(ink.color, `${id} ${slot}`).not.toBe(theme.xterm[slot]);
      expect(theme.xterm, id).toEqual(before);
    }
  });

  it('keeps the tag a step back only while the step back reads at 3:1', () => {
    expect(chatInks(kanso, themeTokens(findTheme('kanso-zen'))).brightYellow).toEqual({
      color: kanso.brightYellow,
      fadeTag: true,
    });
    // Vellum's green tell reads at 4.3:1 on its panel, but near 2.6:1 a
    // step back, so its tag draws solid.
    expect(chatInks(vellum, themeTokens(findTheme('vellum'))).green).toEqual({
      color: vellum.green,
      fadeTag: false,
    });
  });

  it('draws a color or panel that does not parse as given', () => {
    const nord = findTheme('nord');
    const tokens = themeTokens(nord);
    const broken = { ...nord.xterm, red: 'var(--red)' };
    expect(chatInks(broken, tokens).red).toEqual({ color: 'var(--red)', fadeTag: true });
    expect(chatInks(nord.xterm, { ...tokens, panel: 'transparent' }).black).toEqual({
      color: nord.xterm.black,
      fadeTag: true,
    });
  });
});

describe('the colors you pick for a channel', () => {
  const picked = normalizeChatColors({ say: 'brightBlue', Tell: 'red' });

  it('lists the eleven channels the game sends, in the order the menu shows them', () => {
    expect(CHAT_CHANNELS).toEqual([
      'say',
      'tell',
      'gtell',
      'yell',
      'pray',
      'cabal',
      'clan',
      'faction',
      'newbie',
      'immortal',
      'imp',
    ]);
  });

  it('takes the slot you picked over the one the game prints it in', () => {
    expect(chatChannelSlot('say', picked)).toBe('brightBlue');
    expect(chatChannelSlot(' TELL ', picked)).toBe('red');
    expect(chatChannelSlot('yell', picked)).toBe('cyan');
    expect(chatChannelColor('say', kanso, picked)).toBe(kanso.brightBlue);
    expect(chatChannelColor('say', vellum, picked)).toBe(vellum.brightBlue);
  });

  it('reads only the sixteen slots, and never a key off the prototype', () => {
    const colors = normalizeChatColors({
      say: 'sparkle',
      yell: 7,
      '  ': 'red',
      constructor: 'green',
      Clan: 'brightCyan',
    });
    expect([...colors.entries()]).toEqual([
      ['constructor', 'green'],
      ['clan', 'brightCyan'],
    ]);
    expect(normalizeChatColors(null).size).toBe(0);
    expect(normalizeChatColors(['red']).size).toBe(0);
    expect(chatChannelSlot('toString', normalizeChatColors({}))).toBe(chatChannelSlot('tostring'));
  });

  it('offers Default and the theme colors, checking the current pick', () => {
    const choices = chatColorChoices('say', picked, kanso);
    expect(choices).toHaveLength(17);
    expect(choices[0]).toEqual({
      value: null,
      label: 'Default',
      swatch: kanso.brightYellow,
      checked: false,
    });
    expect(choices.slice(1).map((c) => c.label)).toEqual([
      'Black',
      'Red',
      'Green',
      'Yellow',
      'Blue',
      'Magenta',
      'Cyan',
      'White',
      'Bright black',
      'Bright red',
      'Bright green',
      'Bright yellow',
      'Bright blue',
      'Bright magenta',
      'Bright cyan',
      'Bright white',
    ]);
    expect(choices.filter((c) => c.checked).map((c) => c.value)).toEqual(['brightBlue']);
    expect(choices.find((c) => c.value === 'red')?.swatch).toBe(kanso.red);
    // A channel you never recolored checks Default.
    const plain = chatColorChoices('yell', picked, kanso);
    expect(plain.filter((c) => c.checked).map((c) => c.value)).toEqual([null]);
  });

  it('tells two tables apart only by what they hold', () => {
    expect(sameChatColors(picked, normalizeChatColors({ tell: 'red', say: 'brightBlue' }))).toBe(
      true,
    );
    expect(sameChatColors(picked, normalizeChatColors({ say: 'brightBlue' }))).toBe(false);
    expect(sameChatColors(picked, normalizeChatColors({ say: 'blue', tell: 'red' }))).toBe(false);
  });
});
