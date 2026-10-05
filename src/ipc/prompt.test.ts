import { describe, expect, it } from 'vitest';
import { normalizePromptShow } from './prompt';
import { normalizeUiConfig, type RawUiConfig } from './uiConfig';

const raw = (patch: Partial<RawUiConfig> = {}): RawUiConfig => ({
  theme: 'nord',
  auto_update: false,
  font_family: 'Menlo',
  font_size: 14,
  tracked_affects: [],
  enabled_presets: [],
  ...patch,
});

describe('where your prompt shows', () => {
  it('reads the three places and the text for anything else', () => {
    expect(normalizePromptShow('text')).toBe('text');
    expect(normalizePromptShow('lifted')).toBe('lifted');
    expect(normalizePromptShow('pinned')).toBe('pinned');
    expect(normalizePromptShow('floating')).toBe('text');
    expect(normalizePromptShow(undefined)).toBe('text');
    expect(normalizePromptShow(3)).toBe('text');
  });

  it('leaves your prompt out of the Settings config', () => {
    // An older backend still sends the three fields. The config drops
    // them, so no save sends them back.
    const config = normalizeUiConfig({
      ...raw({}),
      prompt_template_enabled: true,
      prompt_template: '%hp',
      prompt_show: 'pinned',
    } as RawUiConfig);
    for (const key of ['prompt_template_enabled', 'prompt_template', 'prompt_show']) {
      expect(key in config).toBe(false);
    }
  });
});
