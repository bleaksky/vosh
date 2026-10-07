import { useMemo } from 'react';
import { normalizeAlert, withAlertPart, withAlertParts } from '../../automation/alertParts';
import {
  alertPresetById,
  isAlertPresetId,
  PRESET_ALERT_DEFAULT,
} from '../../automation/alertPresets';
import {
  countPhrase,
  draftChanges,
  draftValues,
  serializeValue,
} from '../../automation/automationDraft';
import { searchText } from '../../automation/automationList';
import {
  keptKeyNote,
  keysYourMacrosKeep,
  presetSavePlan,
  presetToggles,
  storedPresetIds,
  type PresetToggle,
} from '../../automation/automationRecords';
import {
  type Preset,
  PRESET_CATEGORIES,
  presetById,
  presetMacros,
  PRESETS,
  presetTriggers,
} from '../../automation/presets';
import { alertPresetsGet, alertPresetsSet } from '../../ipc/alerts';
import { type AlertParts, presetsInstall, presetsRemove } from '../../ipc/automation';
import { getUiConfig, setUiFields } from '../../ipc/uiConfig';
import { listJoin } from '../../lib/text';
import { useMacroList } from '../../stores/config/macroListStore';
import type { SetUiConfig } from '../pageTypes';
import { Card, CardNote, cx, Keycap, Row, Toggle } from '../../ui';
import { AlertDetailRows, AlertRow, BannerOffNote, type AlertDetail } from './AlertRows';
import { DraftEditor } from './DraftEditor';
import type { DetailProps, DirtyReport, KindSpec } from './types';
import { useBannerPermission } from './useBannerPermission';

const TRIGGER_NOUN = { one: 'trigger', many: 'triggers' };
const MACRO_NOUN = { one: 'macro', many: 'macros' };
const ALERT_NOUN = { one: 'alert', many: 'alerts' };
const ALERTS_CATEGORY = 'Alerts';

interface PresetsEditorProps {
  setConfig: SetUiConfig;
  onDirty: (report: DirtyReport | null) => void;
  onError: (message: string | null) => void;
}

/** The presets, one toggle each under its category, the five alert
 *  presets under Alerts. Save first writes the parts of each alert
 *  preset you changed, then installs the triggers and macros of the
 *  presets you turned on, removes the ones you turned off, and stores
 *  the list in enabled_presets, which launch reads to put the ones that
 *  are on back and take the rest out. */
export function PresetsEditor({ setConfig, onDirty, onError }: PresetsEditorProps) {
  const spec = useMemo<KindSpec<PresetToggle>>(
    () => ({
      id: 'presets',
      noun: { one: 'preset', many: 'presets' },
      filterLabel: 'Filter presets',
      emptyDetail: 'Choose a preset to see what it adds.',
      emptyList: 'Vosh has no presets.',
      // Read the stored list fresh, so Settings moving to another
      // profile loads its presets. Each profile keeps its own list, and
      // in loadout mode every profile shares one, next to the preset
      // triggers in the shared catalog.
      load: async (profile) => {
        const [config, alerts] = await Promise.all([
          getUiConfig(profile),
          alertPresetsGet(profile),
        ]);
        return [
          ...presetToggles(config.enabled_presets),
          ...alerts.ids.map((id) => ({
            id,
            enabled: alerts.on.includes(id),
            alert: normalizeAlert(alerts.alerts[id]) ?? PRESET_ALERT_DEFAULT,
          })),
        ];
      },
      save: async (draft, _written, profile) => {
        // Rust saves each one at once. A Save that fails after them keeps
        // the draft unsaved, and the next Save sends the same parts again.
        for (const { before, after } of draftChanges(draft).changed) {
          if (!after.alert || serializeValue(before.alert) === serializeValue(after.alert)) {
            continue;
          }
          await alertPresetsSet(
            after.id,
            isPresetDefault(after.alert) ? null : after.alert,
            profile,
          );
        }
        const plan = presetSavePlan(draft);
        for (const id of plan.remove) await presetsRemove(id, profile);
        const on = PRESETS.filter((p) => plan.install.includes(p.id));
        const triggers = on.flatMap((p) => presetTriggers(p));
        const macros = on.flatMap(presetMacros);
        if (triggers.length > 0 || macros.length > 0) {
          await presetsInstall(triggers, macros, profile);
        }
        // Keep the ids no preset of this build knows, as the profile
        // holds them now.
        const stored = (await getUiConfig(profile)).enabled_presets;
        const enabled_presets = storedPresetIds(draftValues(draft), stored);
        await setUiFields({ enabled_presets }, profile);
        setConfig((prev) => (prev ? { ...prev, enabled_presets } : prev));
      },
      entry: (t) => {
        const alert = alertPresetById(t.id);
        if (alert) {
          return {
            name: alert.name,
            group: ALERTS_CATEGORY,
            enabled: t.enabled,
            text: searchText(alert.name, alert.description, ALERTS_CATEGORY),
          };
        }
        const preset = presetById(t.id);
        const category = preset ? PRESET_CATEGORIES[preset.category] : '';
        return {
          name: preset?.name ?? t.id,
          group: category,
          enabled: t.enabled,
          text: searchText(preset?.name, preset?.description, category),
        };
      },
      keyOf: (t) => t.id,
      renderDetail: (props) =>
        isAlertPresetId(props.value.id) ? (
          <AlertPresetDetail {...props} onError={onError} />
        ) : (
          <PresetDetail {...props} />
        ),
    }),
    [setConfig, onError],
  );

  return (
    <DraftEditor spec={spec} json={false} onJson={() => {}} onDirty={onDirty} onError={onError} />
  );
}

export function PresetDetail({ value: t, update }: DetailProps<PresetToggle>) {
  const preset = presetById(t.id);
  if (!preset) return null;
  const binds = preset.macros ?? [];
  const adds = listJoin([
    ...(preset.triggers.length > 0 ? [countPhrase(preset.triggers.length, TRIGGER_NOUN)] : []),
    ...(binds.length > 0 ? [countPhrase(binds.length, MACRO_NOUN)] : []),
  ]);
  return (
    <Card className="st-auto-card">
      <Row label={preset.name} description={preset.description}>
        <Toggle checked={t.enabled} onChange={(enabled) => update((v) => ({ ...v, enabled }))} />
      </Row>
      <Row label="Adds">
        <span className="st-auto-value">{adds}</span>
      </Row>
      {binds.length > 0 && <PresetKeys preset={preset} />}
    </Card>
  );
}

function isPresetDefault(alert: AlertParts): boolean {
  return serializeValue(alert) === serializeValue(PRESET_ALERT_DEFAULT);
}

/** The card of an alert preset, as board 2 draws it: its toggle, what
 *  it listens to, the Alert row, the rows of the parts that are pressed
 *  and the switch. Banner shows waits for a preset whose banner can
 *  carry words. A preset always holds its parts, so releasing every one
 *  keeps the table and the preset rings nothing. Turning on a preset
 *  whose Banner is on asks first, as pressing Banner does (board 3), and
 *  while the system turns banners off the card opens with a note. */
export function AlertPresetDetail({
  value: t,
  update,
  onError,
}: DetailProps<PresetToggle> & { onError: (message: string | null) => void }) {
  const banner = useBannerPermission();
  const preset = alertPresetById(t.id);
  if (!preset) return null;
  const alert = t.alert ?? PRESET_ALERT_DEFAULT;
  const turn = (enabled: boolean) => {
    const press = () => update((v) => ({ ...v, enabled }));
    if (enabled && alert.banner) banner.askFirst(press);
    else press();
  };
  const rows: AlertDetail[] = [
    ...(alert.sound !== undefined ? (['sound'] as const) : []),
    ...(alert.attention !== undefined ? (['attention'] as const) : []),
    ...(alert.banner && preset.words ? (['words'] as const) : []),
    'background',
  ];
  return (
    <Card className="st-auto-card">
      {banner.permission === 'denied' && <BannerOffNote onError={onError} />}
      <Row label={preset.name} description={preset.description}>
        <Toggle checked={t.enabled} onChange={turn} />
      </Row>
      <Row label="Listens to">
        <span className="st-auto-value">{preset.listensTo}</span>
      </Row>
      <AlertRow
        alert={alert}
        disabled={false}
        banner={banner}
        onPress={(part, on) =>
          update((v) => ({ ...v, alert: withAlertPart(v.alert ?? alert, part, on) }))
        }
      />
      <AlertDetailRows
        alert={alert}
        disabled={false}
        only={rows}
        onChange={(patch) =>
          update((v) => ({ ...v, alert: withAlertParts(v.alert ?? alert, patch) }))
        }
      />
      <Row label="Adds">
        <span className="st-auto-value">{countPhrase(1, ALERT_NOUN)}</span>
      </Row>
    </Card>
  );
}

/** The keys a macro preset binds, as Scripts board 7 draws them, each
 *  on a keycap before the command it sends, in the preset's order. The
 *  card names the numpad, so a numpad key's cap holds its digit alone. A
 *  key one of your macros uses stays yours, so its pair wears the warn
 *  ring and a note closes the card. The note shows with the preset on or
 *  off, since it is true either way. */
function PresetKeys({ preset }: { preset: Preset }) {
  const kept = new Set(keysYourMacrosKeep(preset, useMacroList()));
  const binds = preset.macros ?? [];
  const held = binds.filter((m) => kept.has(m.key));
  return (
    <>
      <Row label="Keys">
        <div className="st-auto-keys" role="group" aria-label="Keys this preset binds">
          {binds.map((m) => (
            <span key={m.key} className={cx('st-auto-keypair', kept.has(m.key) && 'is-warn')}>
              <Keycap>{m.key.replace(/^Numpad/, '')}</Keycap>
              <span className="st-auto-keysend">{m.command}</span>
            </span>
          ))}
        </div>
      </Row>
      {held.length > 0 && <CardNote tone="warn">{keptKeyNote(held)}</CardNote>}
    </>
  );
}
