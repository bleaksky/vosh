// Visible dot + "unsaved" word reused across every manual-save
// form. Single source so the wording, color, and animation stay
// aligned across the trigger / alias / JSON tabs.
export function UnsavedDot() {
  return (
    <span className="settings-unsaved" title="you have unsaved changes — click [save]">
      <span className="settings-unsaved-dot" aria-hidden="true" />
      unsaved
    </span>
  );
}
