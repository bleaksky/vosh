import { describe, expect, it } from 'vitest';
import { OutputShaper } from './outputShaper';
import { decodeOutputPayload } from './session';

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
    const bytes = new TextEncoder().encode('Ælfric says hello to Tarvik.\r\n');
    // Æ takes two bytes, and the first read ends after one.
    const first = decodeOutputPayload({ b64: btoa(String.fromCharCode(...bytes.slice(0, 1))) });
    const rest = decodeOutputPayload({ b64: btoa(String.fromCharCode(...bytes.slice(1))) });
    const shaper = new OutputShaper(10);
    const a = shaper.text(first);
    const b = shaper.text(rest);
    expect(a.text + b.text).toBe('Ælfric says hello to Tarvik.\r\n');
    expect(b.replace).toBeNull();
    const shaped = new OutputShaper(10);
    expect(shaped.shape(first).text + shaped.shape(rest).text).toBe(a.text + b.text);
  });

  it('decodes a replace for the names cache too', () => {
    const out = decodeOutputPayload({
      b64: '',
      replace: { gen: 3, b64: btoa('Selune waves.'), fresh: false },
    });
    expect(new OutputShaper(80).text(out)).toEqual({ text: '', replace: 'Selune waves.' });
  });
});
