import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import {
  HELP_GOTO_EVENT,
  HELP_PENDING_KEY,
  helpNoMatchNotice,
  helpOpensOn,
  openHelpTopic,
} from './helpLink';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({ emit: vi.fn(() => Promise.resolve()) }));

describe('#help with words', () => {
  it('opens Help on a topic, a number, or words a topic holds', () => {
    expect(helpOpensOn('shape.prompt-show')).toBe(true);
    expect(helpOpensOn('9.3')).toBe(true);
    expect(helpOpensOn('prompt')).toBe(true);
  });

  it('says in the terminal when no topic holds the words', () => {
    expect(helpOpensOn('zzyzx')).toBe(false);
    expect(helpOpensOn('  ')).toBe(false);
    const notice = helpNoMatchNotice('zzyzx');
    expect(notice).toBe('No help topic mentions zzyzx. Type #help for the slash commands.');
    // The writing style keeps colons and semicolons out of the prose.
    expect(notice).not.toMatch(/[;:]/);
  });
});

describe('opening Help on a target', () => {
  const stored = new Map<string, string>();

  beforeEach(() => {
    stored.clear();
    vi.stubGlobal('localStorage', {
      setItem: (k: string, v: string) => stored.set(k, v),
      getItem: (k: string) => stored.get(k) ?? null,
      removeItem: (k: string) => stored.delete(k),
    });
    vi.mocked(invoke).mockClear();
    vi.mocked(emit).mockClear();
  });

  it('leaves it for a cold open, tells an open window, and opens the window', () => {
    openHelpTopic('prompt');
    expect(stored.get(HELP_PENDING_KEY)).toBe('prompt');
    expect(emit).toHaveBeenCalledWith(HELP_GOTO_EVENT, 'prompt');
    expect(invoke).toHaveBeenCalledWith('open_help_window');
  });
});
