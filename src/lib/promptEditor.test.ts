import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { listen, type EventCallback } from '@tauri-apps/api/event';
import {
  normalizePromptConfig,
  onPromptGagWithoutReader,
  onPromptState,
  onPromptStatus,
  promptCandidates,
  promptCaptureCheck,
  promptCaptureFromLine,
  promptCompile,
  promptConfigGet,
  promptConfigSet,
  promptDesignsList,
  promptEdit,
  promptPreviewSet,
  promptRender,
  promptRenderMany,
  promptStateGet,
  promptWatch,
  subscribePromptConfigChanged,
  terminalCursor,
  type PromptConfig,
  type PromptConfigChangedPayload,
} from './session';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

afterEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(() => Promise.resolve());
  vi.mocked(listen).mockClear();
});

describe('the [prompt] table', () => {
  it('fills what the backend leaves out with the defaults', () => {
    expect(normalizePromptConfig({ draw: true, template: '%hp' })).toEqual({
      draw: true,
      template: '%hp',
      previous_templates: [],
      capture: { kind: 'none' },
      show: 'text',
    });
    expect(normalizePromptConfig(null)).toEqual({
      draw: false,
      template: '',
      previous_templates: [],
      capture: { kind: 'none' },
      show: 'text',
    });
  });

  it('keeps a capture, the earlier designs and where your prompt shows', () => {
    const capture = {
      kind: 'aabahran',
      prompt: '%n%P%C[%h/%Hhp]%c',
      fprompt: '',
      follow_game: true,
      source: 'gmcp',
      seen_at: '2026-09-29T12:58:02-05:00',
    };
    const config = normalizePromptConfig({
      draw: true,
      template: '%hp',
      previous_templates: ['[%hp]', 3],
      capture,
      show: 'pinned',
    });
    expect(config.capture).toEqual(capture);
    expect(config.previous_templates).toEqual(['[%hp]']);
    expect(config.show).toBe('pinned');
    // A capture kind this build does not know reads as none.
    expect(normalizePromptConfig({ capture: { kind: 'telepathy' } }).capture).toEqual({
      kind: 'none',
    });
  });
});

describe('the prompt editor commands', () => {
  it('reads the table and saves one under the name the backend takes', async () => {
    vi.mocked(invoke).mockImplementation(((command: string) =>
      Promise.resolve(
        command === 'prompt_config_get' ? { draw: true, template: '%hp' } : undefined,
      )) as typeof invoke);
    const config = await promptConfigGet();
    expect(invoke).toHaveBeenCalledWith('prompt_config_get');
    expect(config.capture).toEqual({ kind: 'none' });
    const next: PromptConfig = { ...config, template: '[%hp]' };
    await promptConfigSet(next);
    expect(invoke).toHaveBeenLastCalledWith('prompt_config_set', { config: next });
  });

  it('sends each command its arguments as the backend reads them', async () => {
    const sent = vi.mocked(invoke);
    await promptDesignsList();
    expect(sent).toHaveBeenLastCalledWith('prompt_designs_list');
    await promptCompile({ kind: 'aabahran', prompt: '[%h/%Hhp]', typed: true });
    expect(sent).toHaveBeenLastCalledWith('prompt_compile', {
      capture: { kind: 'aabahran', prompt: '[%h/%Hhp]', typed: true },
    });
    await promptCandidates();
    expect(sent).toHaveBeenLastCalledWith('prompt_candidates');
    await promptCaptureFromLine(7);
    expect(sent).toHaveBeenLastCalledWith('prompt_capture_from_line', { id: 7, names: null });
    await promptCaptureFromLine(7, ['health', '']);
    expect(sent).toHaveBeenLastCalledWith('prompt_capture_from_line', {
      id: 7,
      names: ['health', ''],
    });
    await promptCaptureCheck({ kind: 'regex', lines: ['^> $'], settle: true });
    expect(sent).toHaveBeenLastCalledWith('prompt_capture_check', {
      capture: { kind: 'regex', lines: ['^> $'], settle: true },
    });
    await promptRender({ template: '%hp' });
    expect(sent).toHaveBeenLastCalledWith('prompt_render', {
      template: '%hp',
      values: 'live',
      preview: null,
      overrides: null,
      placeholders: false,
    });
    await promptRender({
      template: '%hp',
      values: 'sample',
      preview: 'fight',
      overrides: { values: { hp: 180 }, lament: false },
      placeholders: true,
    });
    expect(sent).toHaveBeenLastCalledWith('prompt_render', {
      template: '%hp',
      values: 'sample',
      preview: 'fight',
      overrides: { values: { hp: 180 }, lament: false },
      placeholders: true,
    });
    await promptPreviewSet({ preview: 'low_health', placeholders: true });
    expect(sent).toHaveBeenLastCalledWith('prompt_preview_set', {
      preview: { preview: 'low_health', placeholders: true },
    });
    await promptPreviewSet(null);
    expect(sent).toHaveBeenLastCalledWith('prompt_preview_set', { preview: null });
    const requests = [{ template: '%hp' }, { template: '%mana', values: 'sample' as const }];
    await promptRenderMany(requests);
    expect(sent).toHaveBeenLastCalledWith('prompt_render_many', { requests });
    await promptEdit('%hp', { op: 'set_style', piece: 0, style: 'italic', on: false });
    expect(sent).toHaveBeenLastCalledWith('prompt_edit', {
      template: '%hp',
      op: { op: 'set_style', piece: 0, style: 'italic', on: false },
    });
    await promptStateGet();
    expect(sent).toHaveBeenLastCalledWith('prompt_state_get');
    await promptWatch(true);
    expect(sent).toHaveBeenLastCalledWith('prompt_watch', { on: true });
  });
});

describe('the prompt editor events', () => {
  it('names the profile whose table changed', async () => {
    let handler: EventCallback<unknown> | undefined;
    vi.mocked(listen).mockImplementationOnce((event, cb) => {
      expect(event).toBe('vosh://prompt-config-changed');
      handler = cb as EventCallback<unknown>;
      return Promise.resolve(() => {});
    });
    const heard: PromptConfigChangedPayload[] = [];
    await subscribePromptConfigChanged((payload) => heard.push(payload));
    const send = (payload: unknown) =>
      handler?.({ event: 'vosh://prompt-config-changed', id: 0, payload });
    send({ profile: 'default' });
    send({ profile: null });
    // An older backend sent an empty string.
    send('');
    expect(heard).toEqual([{ profile: 'default' }, { profile: null }, { profile: null }]);
  });

  it('listens for the prompt state, the status and a gag with no reader', async () => {
    const events: string[] = [];
    vi.mocked(listen).mockImplementation((event) => {
      events.push(event as string);
      return Promise.resolve(() => {});
    });
    await onPromptState(() => {});
    await onPromptStatus(() => {});
    await onPromptGagWithoutReader(() => {});
    expect(events).toEqual([
      'session://prompt-state',
      'session://prompt-status',
      'session://prompt-gag-without-reader',
    ]);
    vi.mocked(listen).mockImplementation(() => Promise.resolve(() => {}));
  });

  it('asks the native grid where its cursor and open region are', async () => {
    const report = {
      line: 1,
      col: 9,
      at_bottom: true,
      cols: 40,
      region: { gen: 3, line: 1, col: 0 },
    };
    vi.mocked(invoke).mockImplementation(() => Promise.resolve(report));
    await expect(terminalCursor()).resolves.toEqual(report);
    expect(vi.mocked(invoke).mock.calls).toEqual([['terminal_cursor']]);
  });
});
