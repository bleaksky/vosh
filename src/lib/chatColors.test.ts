import { describe, expect, it } from 'vitest';
import {
  CHAT_CHANNELS,
  chatChannelColor,
  chatChannelSlot,
  chatColorChoices,
  normalizeChatColors,
  sameChatColors,
} from './chatColors';
import { findTheme } from './themes';

const kanso = findTheme('kanso-zen').xterm;
const vellum = findTheme('vellum').xterm;

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
