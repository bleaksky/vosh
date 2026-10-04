import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { DEFAULT_VITALS_OPTIONS, type VitalsOptions } from '../../lib/session';
import type { PromptShowState } from '../../lib/promptShow';
import { PanelHost } from './PanelHost';

// The stores behind the panel reach the Tauri bridge when they start.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

let options: VitalsOptions = DEFAULT_VITALS_OPTIONS;
vi.mock('../../lib/stores/vitalsOptionsStore', () => ({
  useVitalsOptions: () => options,
}));

// No panes, so only the footer can draw. The footer stands in as its
// own section, since its stores need the running app.
vi.mock('./panelLayoutStore', () => ({
  usePanelLayout: () => null,
  getPanelLayout: () => null,
  setPaneTree: vi.fn(),
}));
vi.mock('./usePaneMins', () => ({ usePaneMins: () => ({}) }));
vi.mock('./VitalsFooter', () => ({
  VitalsFooter: ({ opponentOnly }: { opponentOnly?: boolean }) => (
    <section className={opponentOnly ? 'panel-opponent' : 'panel-vitals'} />
  ),
}));

const PINNED: PromptShowState = {
  show: 'pinned',
  capture: true,
  draw: true,
  gameSent: true,
  zone: 1,
  promptsOff: false,
};

function drawsVitals(promptShow: PromptShowState | null, hide: boolean): boolean {
  options = { ...DEFAULT_VITALS_OPTIONS, hide_when_pinned: hide };
  return renderToStaticMarkup(<PanelHost promptShow={promptShow} />).includes('panel-vitals');
}

describe('PanelHost', () => {
  it('drops the vitals while your prompt is pinned and the switch is on', () => {
    expect(drawsVitals(PINNED, true)).toBe(false);
  });

  it('keeps the opponent row while your pinned prompt hides the vitals', () => {
    options = { ...DEFAULT_VITALS_OPTIONS, hide_when_pinned: true };
    expect(renderToStaticMarkup(<PanelHost promptShow={PINNED} />)).toContain('panel-opponent');
    options = { ...DEFAULT_VITALS_OPTIONS, hide_when_pinned: false };
    expect(renderToStaticMarkup(<PanelHost promptShow={PINNED} />)).not.toContain('panel-opponent');
  });

  it('brings the vitals back when either one is off', () => {
    expect(drawsVitals(PINNED, false)).toBe(true);
    expect(drawsVitals({ ...PINNED, show: 'text' }, true)).toBe(true);
    expect(drawsVitals(null, true)).toBe(true);
  });

  it('keeps the vitals while prompts are off in the game', () => {
    expect(drawsVitals({ ...PINNED, promptsOff: true }, true)).toBe(true);
  });

  it('writes your terminal size on the panel for the game text in the panes', () => {
    options = DEFAULT_VITALS_OPTIONS;
    expect(renderToStaticMarkup(<PanelHost promptShow={null} fontSize={16} />)).toContain(
      '<div class="panel-host" style="--font-mud-px:16">',
    );
    // 12 px, the panes as they were drawn, when no size or no real one
    // comes in.
    for (const size of [undefined, 0, Number.NaN]) {
      expect(renderToStaticMarkup(<PanelHost promptShow={null} fontSize={size} />)).toContain(
        '<div class="panel-host" style="--font-mud-px:12">',
      );
    }
  });
});
