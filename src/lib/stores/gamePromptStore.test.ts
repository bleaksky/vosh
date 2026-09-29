import { describe, expect, it } from 'vitest';
import { aabahranPacket } from '../../test/aabahranGmcp';
import { parseGamePrompt } from './gamePromptStore';

const fixture = (name: string) => parseGamePrompt(aabahranPacket(name).data);

describe('parseGamePrompt', () => {
  it('reads the prompt settings the game sends at login', () => {
    expect(fixture('char-prompt.gmcp')).toEqual({
      enabled: true,
      prompt: '%n%P%C<%hhp %mm %vmv> ',
      fprompt: '',
    });
  });

  it('reads prompt off', () => {
    expect(fixture('char-prompt-off.gmcp')?.enabled).toBe(false);
  });

  it('keeps the text raw, colour codes and trailing spaces included', () => {
    expect(fixture('char-prompt-fight.gmcp')).toEqual({
      enabled: true,
      prompt: '%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c',
      fprompt: '`1%h``hp [%p] > ',
    });
  });

  it('tolerates missing fields and reads anything else as nothing', () => {
    expect(parseGamePrompt({})).toEqual({ enabled: true, prompt: '', fprompt: '' });
    expect(parseGamePrompt({ enabled: 'no', prompt: 7 })).toEqual({
      enabled: true,
      prompt: '',
      fprompt: '',
    });
    expect(parseGamePrompt(null)).toBeNull();
    expect(parseGamePrompt([])).toBeNull();
  });
});
