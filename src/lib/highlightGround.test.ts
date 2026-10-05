import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { setFitGameColors } from './fitGameColors';
import { resetHighlightGround, setHighlightGround, setReadableHighlights } from './highlightGround';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));

const sent = () => vi.mocked(invoke).mock.calls.map(([, args]) => args);

describe('the highlight ground report', () => {
  beforeEach(() => {
    setFitGameColors(false);
    resetHighlightGround();
    vi.mocked(invoke).mockClear();
  });

  it('sends the theme background while the setting is on', () => {
    setHighlightGround('#f7f4ee');
    expect(invoke).toHaveBeenCalledWith('highlight_ground_set', {
      background: '#f7f4ee',
      game: null,
    });
  });

  it('sends null while the setting is off, and the background again once it is on', () => {
    setHighlightGround('#f7f4ee');
    setReadableHighlights(false);
    setHighlightGround('#2e3440');
    setReadableHighlights(true);
    expect(sent()).toEqual([
      { background: '#f7f4ee', game: null },
      { background: null, game: null },
      { background: '#2e3440', game: null },
    ]);
  });

  it('sends only a change', () => {
    setHighlightGround('#f7f4ee');
    setHighlightGround('#f7f4ee');
    setReadableHighlights(true);
    setFitGameColors(false);
    expect(sent()).toEqual([{ background: '#f7f4ee', game: null }]);
  });

  it('holds no ground until a theme reports', () => {
    setReadableHighlights(false);
    setReadableHighlights(true);
    setFitGameColors(true);
    expect(sent()).toEqual([{ background: null, game: null }]);
  });

  it('sends the theme background as the game ground while Fit game colors is on', () => {
    setHighlightGround('#f0e5cf');
    setFitGameColors(true);
    setReadableHighlights(false);
    setHighlightGround('#f1f1f1');
    setFitGameColors(false);
    expect(sent()).toEqual([
      { background: '#f0e5cf', game: null },
      { background: '#f0e5cf', game: '#f0e5cf' },
      { background: null, game: '#f0e5cf' },
      { background: null, game: '#f1f1f1' },
      { background: null, game: null },
    ]);
  });
});
