import { describe, expect, it } from 'vitest';
import { OutputShaper } from './outputShaper';
import { decodeOutputPayload } from '../ipc/terminal';
import { washFields } from './xterm/xtermWash';
import washFixture from '../../fixtures/wash/fields.json';

describe('OutputShaper', () => {
  it('writes nothing for a repaint of the pinned band alone', () => {
    // The clock repaints the band once a second while you sit idle. With
    // nothing for the text, the live pane writes nothing and stays where
    // you scrolled it.
    const band = decodeOutputPayload({ b64: '', pin: btoa('<1020> 29') });
    expect(new OutputShaper(80).shape(band).output).toBeNull();
  });

  it('passes on whether the pinned row is open with nothing else to write', () => {
    const out = decodeOutputPayload({ b64: '', pin: btoa('<1020> 29'), pin_row: true });
    expect(new OutputShaper(80).shape(out).output).toEqual({ text: '', pinRow: true });
  });

  it('hands the recent names cache the same text with no wrapping when it writes nothing', () => {
    // A copy the native underlay hides writes nothing, but Tab still
    // completes the names it saw, a character split across reads too.
    const bytes = new TextEncoder().encode('Ælfric says hello to Orla.\r\n');
    // Æ takes two bytes, and the first read ends after one.
    const first = decodeOutputPayload({ b64: btoa(String.fromCharCode(...bytes.slice(0, 1))) });
    const rest = decodeOutputPayload({ b64: btoa(String.fromCharCode(...bytes.slice(1))) });
    const shaper = new OutputShaper(10);
    const a = shaper.text(first);
    const b = shaper.text(rest);
    expect(a.text + b.text).toBe('Ælfric says hello to Orla.\r\n');
    expect(b.replace).toBeNull();
    const shaped = new OutputShaper(10);
    expect(shaped.shape(first).text + shaped.shape(rest).text).toBe(a.text + b.text);
  });

  it('decodes a replace for the names cache too', () => {
    const out = decodeOutputPayload({
      b64: '',
      replace: { gen: 3, b64: btoa('Tolliver waves.'), fresh: false },
    });
    expect(new OutputShaper(80).text(out)).toEqual({ text: '', replace: 'Tolliver waves.' });
  });

  it('paints washes in the replace, its above and tail, the restore and the hold', () => {
    // A trigger washed each piece yellow. The field of Obsidian Ember
    // takes the tint's place everywhere the writer writes.
    const ember = washFixture.cases[0];
    const fields = washFields(ember.palette, ember.ground);
    const wash = (s: string) => btoa(`\x1b[33;48;2;51;51;0m${s}\x1b[0m`);
    const out = decodeOutputPayload({
      b64: wash('Your sanctuary flickers and fades.'),
      replace: {
        gen: 2,
        b64: wash('Maren waves.'),
        fresh: false,
        above: { plain: 'Orla nods.', b64: wash('Orla nods.') },
        tail: wash('Tolliver bows.'),
      },
      restore: wash('Orla smiles.'),
      hold: btoa('\r\n'),
    });
    const shaped = new OutputShaper(80, () => fields).shape(out).output!;
    const field = '\x1b[33;48;2;42;35;20m';
    const pieces = [
      shaped.replace!.text,
      shaped.replace!.above!.text,
      shaped.replace!.tail!,
      shaped.restore!,
      shaped.text,
    ];
    for (const piece of pieces) {
      expect(piece).toContain(field);
      expect(piece).not.toContain('51;51;0');
    }
    // The text's row ends in the hold, which fills it to the edge.
    expect(shaped.hold).toBe('\x1b[48;2;42;35;20m\x1b[K\x1b[49m\r\n');
  });
});
