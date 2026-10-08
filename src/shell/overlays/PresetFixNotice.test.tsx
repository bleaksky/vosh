import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { closePresetFix, presetFixStore, showPresetFix } from '../../stores/presetFixStore';
import { pushToast, dismissToast, getToasts } from '../../stores/toasts';
import { CornerNotices } from './CornerNotices';

// The corner notice a preset fix leaves at launch, on
// the update notice recipe in the warn tone, in the corner's slot list.

// A toast keeps its dismiss timer on window.
vi.stubGlobal('window', globalThis);

afterEach(() => {
  closePresetFix();
  for (const t of getToasts()) dismissToast(t.id);
});

const SECONDARY = { preset: 'disarm_buff_fade', trigger: 'disarm.secondary', row: 'send' };

describe('the preset fix notice', () => {
  it('names the trigger in the warn tone with Close and Show', () => {
    showPresetFix({ told: [SECONDARY], removed: [] });
    const html = renderToStaticMarkup(<CornerNotices />);
    expect(html).toContain('<div class="ov-update is-warn" role="status" aria-live="polite">');
    expect(html).toContain(
      '<span class="ov-update-msg">A preset fix changed a row you edited</span>',
    );
    expect(html).toContain('<span class="ov-update-meta is-mono">disarm.secondary</span>');
    expect(html).toMatch(/>Close<\/button><button[^>]*class="btn is-primary"[^>]*>Show</);
  });

  it('keeps the notice until you close it, past a run that tells nothing', () => {
    showPresetFix({ told: [SECONDARY], removed: [] });
    showPresetFix({ told: [], removed: [] });
    expect(presetFixStore.get()?.link).toBe('automation:triggers#triggers:disarm.secondary');
    closePresetFix();
    expect(renderToStaticMarkup(<CornerNotices />)).toBe('<div class="ov-corner"></div>');
  });

  it('sits under the toasts in the corner', () => {
    showPresetFix({ told: [SECONDARY], removed: [] });
    pushToast({ kind: 'info', message: 'Copied 4 characters' });
    const html = renderToStaticMarkup(<CornerNotices />);
    expect(html.indexOf('ov-toasts')).toBeGreaterThan(html.indexOf('ov-corner'));
    expect(html.indexOf('ov-update is-warn')).toBeGreaterThan(html.indexOf('ov-toasts'));
  });
});
