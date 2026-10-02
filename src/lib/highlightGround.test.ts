import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { resetHighlightGround, setHighlightGround, setReadableHighlights } from './highlightGround';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));

const sent = () => vi.mocked(invoke).mock.calls.map(([, args]) => args);

describe('the highlight ground report', () => {
  beforeEach(() => {
    resetHighlightGround();
    vi.mocked(invoke).mockClear();
  });

  it('sends the theme background while the setting is on', () => {
    setHighlightGround('#f7f4ee');
    expect(invoke).toHaveBeenCalledWith('highlight_ground_set', { background: '#f7f4ee' });
  });

  it('sends null while the setting is off, and the background again once it is on', () => {
    setHighlightGround('#f7f4ee');
    setReadableHighlights(false);
    setHighlightGround('#2e3440');
    setReadableHighlights(true);
    expect(sent()).toEqual([
      { background: '#f7f4ee' },
      { background: null },
      { background: '#2e3440' },
    ]);
  });

  it('sends only a change', () => {
    setHighlightGround('#f7f4ee');
    setHighlightGround('#f7f4ee');
    setReadableHighlights(true);
    expect(sent()).toEqual([{ background: '#f7f4ee' }]);
  });

  it('holds no ground until a theme reports', () => {
    setReadableHighlights(false);
    setReadableHighlights(true);
    expect(sent()).toEqual([{ background: null }]);
  });
});
