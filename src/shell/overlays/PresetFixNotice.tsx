import { useSyncExternalStore, type MouseEvent } from 'react';
import { openSettingsTab } from '../../lib/settingsLink';
import { closePresetFix, presetFixStore } from '../../stores/presetFixStore';

// A press on the notice's buttons leaves the caret on the command line.
const keepCaret = (event: MouseEvent) => event.preventDefault();

// The notice a preset fix leaves when it changed a row you edited, or
// took away a trigger you edited (Presets board 4). It sits on the update
// notice recipe in the warn tone and stays until you close it. Close
// leaves the marks in Settings. Show opens Settings on the trigger, or on
// the preset's card for a swatch or a trigger the preset no longer
// builds.
export function PresetFixNotice() {
  const notice = useSyncExternalStore(
    presetFixStore.subscribe,
    presetFixStore.get,
    presetFixStore.get,
  );
  if (!notice) return null;
  return (
    <div className="ov-update is-warn" role="status" aria-live="polite">
      <span className="ov-update-dot" aria-hidden="true" />
      <span className="ov-update-msg">{notice.message}</span>
      <span className={`ov-update-meta${notice.mono ? ' is-mono' : ''}`}>{notice.meta}</span>
      <span className="ov-update-actions">
        <button
          type="button"
          className="ov-button"
          onMouseDown={keepCaret}
          onClick={closePresetFix}
        >
          Close
        </button>
        <button
          type="button"
          className="ov-button is-primary"
          onMouseDown={keepCaret}
          onClick={() => openSettingsTab(notice.link)}
        >
          Show
        </button>
      </span>
    </div>
  );
}
