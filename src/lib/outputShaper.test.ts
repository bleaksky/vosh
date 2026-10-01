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
});
