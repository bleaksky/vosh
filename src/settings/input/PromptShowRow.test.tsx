import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { PROMPT_SHOW_LABELS, promptShowLock } from '../../prompt/showState';
import { normalizePromptShowState, type PromptShow, type PromptShowState } from '../../ipc/prompt';
import { SETTINGS_ROWS } from '../settingsSearch';
import { PromptShowField } from './PromptShowRow';

// The Input page's stores reach the Tauri bridge. The rows under test
// draw from the values they are handed.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

const reads: PromptShowState = {
  show: 'text',
  capture: true,
  draw: true,
  gameSent: true,
  zone: 1,
  promptsOff: false,
};

function draw(value: PromptShow, state: PromptShowState | null = reads): string {
  return renderToStaticMarkup(
    <PromptShowField value={value} state={state} onChange={() => undefined} />,
  );
}

const segments = (html: string) =>
  [...html.matchAll(/class="st-seg-item"[^>]*>([^<]*)</g)].map((m) => m[1]);
const pressed = (html: string) =>
  [...html.matchAll(/aria-pressed="true"[^>]*>([^<]*)</g)].map((m) => m[1]);
const labels = (html: string) =>
  [...html.matchAll(/class="st-row-label"[^>]*>([^<]*)</g)].map((m) => m[1]);
const disabled = (html: string) => [...html.matchAll(/<button[^>]*disabled=""/g)].length;

describe('PromptShowField', () => {
  it('offers In the text, Lifted, and Pinned under Where your prompt shows', () => {
    const html = draw('text');
    expect(labels(html)).toEqual(['Where your prompt shows']);
    expect(segments(html)).toEqual(['In the text', 'Lifted', 'Pinned']);
    expect(pressed(html)).toEqual(['In the text']);
    expect(html).toContain('data-st-anchor="prompt-show"');
    expect(disabled(html)).toBe(0);
  });

  it('says what each place does under the row', () => {
    expect(draw('text')).toContain('Your prompt shows in the text, where the game sends it.');
    const lifted = draw('lifted');
    expect(pressed(lifted)).toEqual(['Lifted']);
    expect(lifted).toContain(
      'Each prompt stays in the text on a raised band, so your prompts stand apart from the game.',
    );
    const pinned = draw('pinned');
    expect(pressed(pinned)).toEqual(['Pinned']);
    expect(pinned).toContain(
      'Only your latest prompt shows, on a band above the command line. Earlier prompts leave the text, and Prompts triggers still see every one.',
    );
  });

  it('turns off while the profile reads no prompt, with the Draw row sentence', () => {
    const sent = draw('text', { ...reads, capture: false, gameSent: true });
    expect(sent).toContain('class="st-row is-disabled"');
    expect(sent).toContain('Customize your prompt first.');
    expect(disabled(sent)).toBe(3);
    const older = draw('text', { ...reads, capture: false, gameSent: false });
    expect(older).toContain('Tell Vosh your game&#x27;s prompt first.');
  });

  it('waits with every segment off until the state loads', () => {
    const html = draw('pinned', null);
    expect(disabled(html)).toBe(3);
    expect(html).not.toContain('is-disabled');
  });
});

describe('when you can pick a place', () => {
  it('waits for the state, then locks with a reason while the profile reads no prompt', () => {
    expect(promptShowLock(null)).toEqual({ locked: true, why: null });
    expect(promptShowLock(reads)).toEqual({ locked: false, why: null });
    expect(promptShowLock({ ...reads, capture: false })).toEqual({
      locked: true,
      why: 'Customize your prompt first.',
    });
    expect(promptShowLock({ ...reads, capture: false, gameSent: false })).toEqual({
      locked: true,
      why: "Tell Vosh your game's prompt first.",
    });
  });

  it('names each place the same in the row and the card', () => {
    expect(PROMPT_SHOW_LABELS).toEqual({ text: 'In the text', lifted: 'Lifted', pinned: 'Pinned' });
  });
});

describe('where your prompt shows, found and read', () => {
  it('has a search entry that opens the row on the Prompt tab', () => {
    const entry = SETTINGS_ROWS.find((r) => r.label === 'Where your prompt shows');
    expect(entry?.target).toEqual({ group: 'prompt', anchor: 'prompt-show' });
    expect(entry?.keywords).toContain('pinned');
    expect(entry?.keywords).toContain('lifted');
  });

  it('reads the state prompt_show_get returns', () => {
    expect(
      normalizePromptShowState({
        show: 'pinned',
        capture: true,
        draw: true,
        game_sent: false,
        zone: 3,
        prompts_off: true,
      }),
    ).toEqual({
      show: 'pinned',
      capture: true,
      draw: true,
      gameSent: false,
      zone: 3,
      promptsOff: true,
    });
    expect(normalizePromptShowState(null)).toEqual({
      show: 'text',
      capture: false,
      draw: false,
      gameSent: false,
      zone: 1,
      promptsOff: false,
    });
    expect(normalizePromptShowState({ zone: 40 }).zone).toBe(6);
    expect(normalizePromptShowState({ show: 'floating' }).show).toBe('text');
  });
});
