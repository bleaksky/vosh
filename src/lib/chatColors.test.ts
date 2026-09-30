import { describe, expect, it } from 'vitest';
import { chatChannelColor, chatChannelSlot } from './chatColors';
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
