import { Toggle } from '../ui';

// Don't ask again under Post…'s confirm. On as you post, it turns
// Settings › Input › Ask before you post off, so Post posts at once.

export function DontAskAgain({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: (on: boolean) => void;
}) {
  return (
    <label className="ov-switch-row">
      Don’t ask again
      <Toggle checked={checked} onChange={onChange} />
    </label>
  );
}
