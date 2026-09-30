import { describe, expect, it } from 'vitest';
import { gamePromptToast } from './gamePromptToast';

describe('gamePromptToast', () => {
  it('names the codes your capture took', () => {
    expect(gamePromptToast({ kind: 'gmcp', text: '%h %m ', applied: true })).toEqual({
      kind: 'info',
      message: 'Vosh reads your new prompt.',
      meta: '%h %m ',
    });
    expect(gamePromptToast({ kind: 'prompt', text: '<%hhp> ', applied: true })?.meta).toBe(
      '<%hhp> ',
    );
  });

  it('stays quiet when the capture took nothing', () => {
    expect(gamePromptToast({ kind: 'gmcp', text: '%h ', applied: false })).toBeNull();
    expect(gamePromptToast({ kind: 'off', text: '', applied: true })).toBeNull();
  });
});
