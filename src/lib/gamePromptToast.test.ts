import { describe, expect, it } from 'vitest';
import { gamePromptToast, lostPartsToast } from './gamePromptToast';

describe('gamePromptToast', () => {
  it('names the codes your capture took', () => {
    expect(gamePromptToast({ kind: 'gmcp', text: '%h %m ', applied: true, lost: [] })).toEqual({
      kind: 'info',
      message: 'Vosh reads your new prompt.',
      meta: '%h %m ',
    });
    expect(
      gamePromptToast({ kind: 'prompt', text: '<%hhp> ', applied: true, lost: [] })?.meta,
    ).toBe('<%hhp> ');
  });

  it('stays quiet when the capture took nothing', () => {
    expect(gamePromptToast({ kind: 'gmcp', text: '%h ', applied: false, lost: [] })).toBeNull();
    expect(gamePromptToast({ kind: 'off', text: '', applied: true, lost: [] })).toBeNull();
  });
});

describe('lostPartsToast', () => {
  it('says once which part of your design nothing feeds now (P14)', () => {
    expect(
      lostPartsToast({ kind: 'prompt', text: '%n%C[%h/%Hhp]%c', applied: true, lost: ['tank_hp'] }),
    ).toEqual({
      kind: 'info',
      message:
        "Your prompt no longer shows your tank's health, so that part of your design stays blank.",
    });
  });

  it('names every part, and says those parts for more than one', () => {
    expect(
      lostPartsToast({
        kind: 'gmcp',
        text: '[%h]',
        applied: true,
        lost: ['pos', 'slot1', 'stallion'],
      })?.message,
    ).toBe(
      'Your prompt no longer shows your position, affect slot 1, and your stallion, so those parts of your design stay blank.',
    );
  });

  it('stays quiet when every part is still fed or the capture took nothing', () => {
    expect(lostPartsToast({ kind: 'gmcp', text: '[%h]', applied: true, lost: [] })).toBeNull();
    expect(
      lostPartsToast({ kind: 'gmcp', text: '[%h]', applied: false, lost: ['tank_hp'] }),
    ).toBeNull();
  });
});
