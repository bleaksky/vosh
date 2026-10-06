import type { AlertPart } from '../../automation/alertParts';
import type { AlertParts } from '../../ipc/automation';
import { CheckIcon, Row } from '../../ui';

// The rows that edit an alert, as board 1 of the Alerts and Scenes review
// draws them on the trigger card.

/** The attention part by what it does on this platform.
 *  request_user_attention bounces the Dock on macOS, flashes the taskbar
 *  on Windows and marks the window as wanting you on Linux. */
function attentionLabel(platform: string | undefined): string {
  if (platform === 'windows') return 'Flash';
  if (platform === 'linux') return 'Mark';
  return 'Bounce';
}

/** The Alert row. Banner, Sound and Bounce each press on and off on
 *  their own, and a pressed one leads with the pane menu's check, so the
 *  row never reads as a pick of one. `onPress` gets the part and whether
 *  it is now on. */
export function AlertRow({
  alert,
  disabled,
  onPress,
}: {
  alert: AlertParts | undefined;
  disabled: boolean;
  onPress: (part: AlertPart, on: boolean) => void;
}) {
  const platform =
    typeof document === 'undefined' ? undefined : document.documentElement.dataset.platform;
  const parts: { part: AlertPart; label: string; on: boolean }[] = [
    { part: 'banner', label: 'Banner', on: Boolean(alert?.banner) },
    { part: 'sound', label: 'Sound', on: alert?.sound !== undefined },
    { part: 'attention', label: attentionLabel(platform), on: alert?.attention !== undefined },
  ];
  return (
    <Row label="Alert">
      <div role="group" aria-label="Alert with" className="st-seg is-multi">
        {parts.map(({ part, label, on }) => (
          <button
            key={part}
            type="button"
            className="st-seg-item"
            aria-pressed={on}
            disabled={disabled}
            onClick={() => onPress(part, !on)}
          >
            {on && <CheckIcon size={12} />}
            {label}
          </button>
        ))}
      </div>
    </Row>
  );
}
