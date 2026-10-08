import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { postStillAsks } from './cardDialogs';
import { stopsAsking } from './askPost';
import { DontAskAgain } from './DontAskAgain';

const none = () => undefined;

describe('Don’t ask again under Post…', () => {
  it('draws a switch named Don’t ask again, off until you turn it on', () => {
    const off = renderToStaticMarkup(<DontAskAgain checked={false} onChange={none} />);
    expect(off).toContain('class="ov-switch-row"');
    expect(off).toContain('Don’t ask again');
    expect(off).toContain('role="switch"');
    expect(off).not.toContain('checked=""');
    const on = renderToStaticMarkup(<DontAskAgain checked onChange={none} />);
    expect(on).toContain('checked=""');
  });

  it('turns Ask before you post off only when the confirm offered it and you turned it on', () => {
    expect(stopsAsking(true, true)).toBe(true);
    expect(stopsAsking(true, false)).toBe(false);
    expect(stopsAsking(undefined, true)).toBe(false);
    expect(stopsAsking(false, true)).toBe(false);
  });

  it('still asks for a report that would record a room other than where you began it', () => {
    expect(postStillAsks('bug', 'Room one', 'Room two')).toBe(true);
    expect(postStillAsks('bug', 'Room two', 'Room two')).toBe(false);
    expect(postStillAsks('bug', null, 'Room two')).toBe(false);
    expect(postStillAsks('note', 'Room one', 'Room two')).toBe(false);
  });
});
