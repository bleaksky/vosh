import { FIRST_ATTENTION, FIRST_TONE, type AlertPart } from '../../automation/alertParts';
import type { AlertParts } from '../../ipc/automation';
import { ALERT_TONES, playAlertTone } from '../../stores/session/alertTones';
import {
  CheckIcon,
  IconButton,
  PlayIcon,
  Row,
  Segmented,
  Select,
  Toggle,
  type SelectOption,
} from '../../ui';

// The rows that edit an alert, as board 1 of the Alerts and Scenes review
// draws them on the trigger card and under its Advanced.

/** The attention part by what it does on this platform, from the tag
 *  main.tsx sets on the root. request_user_attention bounces the Dock
 *  on macOS, flashes the taskbar on Windows and marks the window as
 *  wanting you on Linux. */
function attentionLabel(): string {
  const platform =
    typeof document === 'undefined' ? undefined : document.documentElement.dataset.platform;
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
  const parts: { part: AlertPart; label: string; on: boolean }[] = [
    { part: 'banner', label: 'Banner', on: Boolean(alert?.banner) },
    { part: 'sound', label: 'Sound', on: alert?.sound !== undefined },
    { part: 'attention', label: attentionLabel(), on: alert?.attention !== undefined },
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

const ATTENTION_OPTIONS = [
  { value: 'once', label: 'Once' },
  { value: 'until', label: 'Until you return' },
] as const;

const BANNER_OPTIONS = [
  { value: 'title', label: 'Title only' },
  { value: 'words', label: 'Title and words' },
] as const;

/** The tones, plus the stored one when Vosh does not know it, so the
 *  select never shows a tone it does not hold. mud.alert can save any
 *  name. */
function toneOptions(current: string): readonly SelectOption[] {
  if (ALERT_TONES.some((t) => t.value === current)) return ALERT_TONES;
  return [...ALERT_TONES, { value: current, label: current }];
}

/** The four rows that tune an alert, which close the card under Advanced
 *  (board 1). They show whatever parts are pressed. A part that is off
 *  shows what pressing it would use, Chime and Once, and a pick there
 *  turns it on. Play sounds the tone shown, on or off. `onChange` gets
 *  the keys a row sets. */
export function AlertDetailRows({
  alert,
  disabled,
  onChange,
}: {
  alert: AlertParts | undefined;
  disabled: boolean;
  onChange: (patch: Partial<AlertParts>) => void;
}) {
  const tone = alert?.sound ?? FIRST_TONE;
  const options = toneOptions(tone);
  const toneLabel = options.find((o) => o.value === tone)?.label ?? tone;
  return (
    <>
      <Row label="Sound">
        <IconButton
          className="st-auto-play"
          label={`Play ${toneLabel}`}
          icon={<PlayIcon />}
          disabled={disabled}
          onClick={() => playAlertTone(tone)}
        />
        <Select
          width={128}
          value={tone}
          options={options}
          disabled={disabled}
          onChange={(sound) => onChange({ sound })}
        />
      </Row>
      <Row label={attentionLabel()}>
        <Segmented
          options={ATTENTION_OPTIONS.map((o) => ({ ...o, disabled }))}
          value={alert?.attention ?? FIRST_ATTENTION}
          onChange={(attention) => onChange({ attention })}
        />
      </Row>
      <Row label="Banner shows">
        <Segmented
          options={BANNER_OPTIONS.map((o) => ({ ...o, disabled }))}
          value={alert?.words ? 'words' : 'title'}
          onChange={(shows) => onChange({ words: shows === 'words' })}
        />
      </Row>
      <Row label="Only while you are not looking at its session">
        <Toggle
          checked={alert?.background ?? true}
          disabled={disabled}
          onChange={(background) => onChange({ background })}
        />
      </Row>
    </>
  );
}
