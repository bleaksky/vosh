import type { ReactNode } from 'react';
import { FIRST_ATTENTION, FIRST_TONE, type AlertPart } from '../../automation/alertParts';
import { alertsOpenSettings } from '../../ipc/alerts';
import type { AlertParts } from '../../ipc/automation';
import { errorText, listJoin } from '../../lib/text';
import { ALERT_TONES, playAlertTone } from '../../stores/session/alertTones';
import {
  Button,
  CardNote,
  CheckIcon,
  cx,
  IconButton,
  PlayIcon,
  Row,
  Segmented,
  Select,
  Toggle,
  type SelectOption,
} from '../../ui';
import { ConfirmDialog } from '../../ui/ConfirmDialog';
import type { BannerPermission } from './useBannerPermission';

// The rows that edit an alert, as board 1 of the Alerts and Scenes review
// draws them on the trigger card and under its Advanced.

function platform(): string | undefined {
  return typeof document === 'undefined' ? undefined : document.documentElement.dataset.platform;
}

/** The attention part by what it does on this platform, from the tag
 *  main.tsx sets on the root. request_user_attention bounces the Dock
 *  on macOS, flashes the taskbar on Windows and marks the window as
 *  wanting you on Linux. */
function attentionLabel(): string {
  const p = platform();
  if (p === 'windows') return 'Flash';
  if (p === 'linux') return 'Mark';
  return 'Bounce';
}

/** The parts `alert` has on, as the Alert row names them, as `Banner
 *  and Sound`, or `none` while it has none. */
export function AlertPartsOn({ alert, none }: { alert: AlertParts | undefined; none: string }) {
  const on = [
    alert?.banner ? 'Banner' : '',
    alert?.sound !== undefined ? 'Sound' : '',
    alert?.attention !== undefined ? attentionLabel() : '',
  ].filter(Boolean);
  return on.length > 0 ? listJoin(on) : none;
}

/** Why Banner shows nothing while the system turns Vosh's banners off.
 *  Only macOS and Windows turn them off. */
function bannerOffNote(): [why: string, still: string] {
  const settings = platform() === 'windows' ? 'Windows Settings' : 'System Settings';
  return [
    `Banners from Vosh are off in ${settings}, so Banner shows nothing.`,
    `Sound and ${attentionLabel()} still work.`,
  ];
}

/** The Alert row. Banner, Sound and Bounce each press on and off on
 *  their own, and a pressed one leads with the pane menu's check, so the
 *  row never reads as a pick of one. `onPress` gets the part and whether
 *  it is now on. Banner on goes through `banner.askFirst`, so the first
 *  one opens Vosh's ask (board 3), which this row draws. While the
 *  system turns banners off, Banner wears the warn ring. */
export function AlertRow({
  alert,
  disabled,
  banner,
  description,
  onPress,
}: {
  alert: AlertParts | undefined;
  disabled: boolean;
  banner: BannerPermission;
  /** The line under the label, as a preset trigger's says what its
   *  preset has once you changed the row. */
  description?: ReactNode;
  onPress: (part: AlertPart, on: boolean) => void;
}) {
  const parts: { part: AlertPart; label: string; on: boolean }[] = [
    { part: 'banner', label: 'Banner', on: Boolean(alert?.banner) },
    { part: 'sound', label: 'Sound', on: alert?.sound !== undefined },
    { part: 'attention', label: attentionLabel(), on: alert?.attention !== undefined },
  ];
  const off = banner.permission === 'denied';
  return (
    <>
      <Row label="Alert" description={description}>
        <div role="group" aria-label="Alert with" className="st-seg is-multi">
          {parts.map(({ part, label, on }) => {
            const warn = off && part === 'banner';
            return (
              <button
                key={part}
                type="button"
                className={cx('st-seg-item', warn && 'is-warn')}
                title={warn ? bannerOffNote()[0] : undefined}
                aria-pressed={on}
                disabled={disabled}
                onClick={() => {
                  const press = () => onPress(part, !on);
                  if (part === 'banner' && !on) banner.askFirst(press);
                  else press();
                }}
              >
                {on && <CheckIcon size={12} />}
                {label}
              </button>
            );
          })}
        </div>
      </Row>
      {banner.asking && (
        <ConfirmDialog
          title="Let Vosh post banners?"
          body="Vosh posts banners only for the alerts you turn on. macOS asks you next."
          confirmLabel="Continue"
          cancelLabel="Not now"
          tone="primary"
          onConfirm={() => banner.answer(true)}
          onCancel={() => banner.answer(false)}
        />
      )}
    </>
  );
}

/** The note an alert preset's card opens with while the system turns
 *  Vosh's banners off, with the button to its notification settings. */
export function BannerOffNote({ onError }: { onError: (message: string | null) => void }) {
  const open = () => {
    alertsOpenSettings()
      .then(() => onError(null))
      .catch((e: unknown) => onError(errorText(e)));
  };
  return (
    <CardNote tone="warn" action={<Button onClick={open}>Open notification settings</Button>}>
      {bannerOffNote().join(' ')}
    </CardNote>
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

/** One of the four rows under the Alert row, by the key it sets. */
export type AlertDetail = 'sound' | 'attention' | 'words' | 'background';

/** The four rows that tune an alert, which close the card under Advanced
 *  (board 1). They show whatever parts are pressed. A part that is off
 *  shows what pressing it would use, Chime and Once, and a pick there
 *  turns it on. Play sounds the tone shown, on or off. `only` keeps the
 *  rows it names, in this order, as the card of an alert preset does
 *  (board 2). `onChange` gets the keys a row sets. */
export function AlertDetailRows({
  alert,
  disabled,
  only,
  onChange,
}: {
  alert: AlertParts | undefined;
  disabled: boolean;
  only?: readonly AlertDetail[];
  onChange: (patch: Partial<AlertParts>) => void;
}) {
  const tone = alert?.sound ?? FIRST_TONE;
  const options = toneOptions(tone);
  const toneLabel = options.find((o) => o.value === tone)?.label ?? tone;
  const has = (row: AlertDetail) => !only || only.includes(row);
  return (
    <>
      {has('sound') && (
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
      )}
      {has('attention') && (
        <Row label={attentionLabel()}>
          <Segmented
            options={ATTENTION_OPTIONS.map((o) => ({ ...o, disabled }))}
            value={alert?.attention ?? FIRST_ATTENTION}
            onChange={(attention) => onChange({ attention })}
          />
        </Row>
      )}
      {has('words') && (
        <Row label="Banner shows">
          <Segmented
            options={BANNER_OPTIONS.map((o) => ({ ...o, disabled }))}
            value={alert?.words ? 'words' : 'title'}
            onChange={(shows) => onChange({ words: shows === 'words' })}
          />
        </Row>
      )}
      {has('background') && (
        <Row label="Only while you are not looking at its session">
          <Toggle
            checked={alert?.background ?? true}
            disabled={disabled}
            onChange={(background) => onChange({ background })}
          />
        </Row>
      )}
    </>
  );
}
