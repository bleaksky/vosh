import { Fragment, useMemo, type ReactNode } from 'react';
import { normalizeAlert, withAlertPart, withAlertParts } from '../../automation/alertParts';
import {
  alertPresetById,
  isAlertPresetId,
  PRESET_ALERT_DEFAULT,
} from '../../automation/alertPresets';
import { countPhrase, draftChanges, serializeValue } from '../../automation/automationDraft';
import { searchText } from '../../automation/automationList';
import {
  keptKeyNote,
  keysYourMacrosKeep,
  presetToggles,
  type PresetToggle,
} from '../../automation/automationRecords';
import { setTriggerGroups, triggerStore } from '../../automation/automationTriggers';
import {
  changesLine,
  editColors,
  editsToSave,
  flagCount,
  groupsReset,
  hasEdits,
  withColorEdit,
  withColorKept,
} from '../../automation/presetEdits';
import { runPresetPlan } from '../../automation/presetPlan';
import { type Preset, PRESET_CATEGORIES, presetById } from '../../automation/presets';
import { alertPresetsGet, alertPresetsSet } from '../../ipc/alerts';
import { type AlertParts, onPresetsChanged, type PresetSwitch } from '../../ipc/automation';
import {
  onPresetEditsChanged,
  presetEditsGet,
  presetEditsSet,
  type PresetEdit,
} from '../../ipc/presetEdits';
import { getUiConfig, type UiConfig } from '../../ipc/uiConfig';
import { knownWorld } from '../../lib/knownWorlds';
import { listJoin } from '../../lib/text';
import { useMacroList } from '../../stores/config/macroListStore';
import { loadTarget } from '../../stores/session/useConnection';
import type { SetUiConfig } from '../pageTypes';
import { getShownProfile } from '../shownProfile';
import { Button, Card, CardNote, cx, Keycap, Row, Toggle } from '../../ui';
import { AlertDetailRows, AlertRow, BannerOffNote, type AlertDetail } from './AlertRows';
import { DraftEditor } from './DraftEditor';
import { PresetColors } from './PresetColors';
import { PresetSample } from '../../automation/PresetSampleView';
import { SamplePaintContext, useSamplePaint } from '../../automation/samplePaint';
import type { DetailProps, DirtyReport, KindSpec, TriggersLink } from './types';
import { useBannerPermission } from './useBannerPermission';

const TRIGGER_NOUN = { one: 'trigger', many: 'triggers' };
const MACRO_NOUN = { one: 'macro', many: 'macros' };
const ALERT_NOUN = { one: 'alert', many: 'alerts' };
const ALERTS_CATEGORY = 'Alerts';

interface PresetsEditorProps {
  config: UiConfig;
  setConfig: SetUiConfig;
  /** Loadout mode, where every profile shares one list of presets. */
  pathB: boolean;
  onDirty: (report: DirtyReport | null) => void;
  onError: (message: string | null) => void;
  /** Select this preset, by id, each time `seq` goes up. */
  selectPreset: { key: string; seq: number } | null;
  /** Open Triggers on a trigger, or filtered, as a link on a card asks. */
  onOpenTriggers: OpenTriggers;
}

/** Open Triggers where a link on a preset's card points. */
type OpenTriggers = (to: Omit<TriggersLink, 'seq'>) => void;

/** The deep link anchor of a preset's row, `presets:<id>`. */
function presetAnchor(id: string): string {
  return `presets:${id}`;
}

/** The world a preset's Suggested row names, the known world of the
 *  host you connect to, else undefined. */
function suggestedWorld(): string | undefined {
  return knownWorld(loadTarget().host)?.name;
}

/** The presets, one toggle each under its category, the five alert
 *  presets under Alerts. Save first writes your edits to each preset
 *  through preset_edits_set, with the group of each trigger whose group
 *  edit Reset to preset took back to the preset's in the trigger store,
 *  and the parts of each alert preset you changed, then runs the preset plan for the profile, which turns on and
 *  off only the presets you flipped here, over the list as the profile
 *  holds it then, through presets_enabled_set (First Run Q17), and
 *  builds the presets in your colors. The page follows that command and
 *  your preset edits as the trigger list follows its store. */
export function PresetsEditor({
  config,
  setConfig,
  pathB,
  onDirty,
  onError,
  selectPreset,
  onOpenTriggers,
}: PresetsEditorProps) {
  const paint = useSamplePaint(config);
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
        const [config, alerts, edits] = await Promise.all([
          getUiConfig(profile),
          alertPresetsGet(profile),
          presetEditsGet(profile),
        ]);
        return [
          ...presetToggles(config.enabled_presets).map((t) =>
            edits[t.id] ? { ...t, edit: edits[t.id] } : t,
          ),
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
        const switches: PresetSwitch[] = [];
        let edited = false;
        for (const { before, after } of draftChanges(draft).changed) {
          if (before.enabled !== after.enabled) switches.push({ id: after.id, on: after.enabled });
          const preset = presetById(after.id);
          const rows = preset ? editsToSave(preset, before.edit, after.edit) : null;
          if (rows) {
            await presetEditsSet(after.id, rows, profile);
            edited = true;
          }
          const regroup = preset ? groupsReset(preset, before.edit, after.edit) : null;
          if (regroup && regroup.size > 0) {
            await setTriggerGroups(regroup, triggerStore(profile));
          }
          if (!after.alert || serializeValue(before.alert) === serializeValue(after.alert)) {
            continue;
          }
          await alertPresetsSet(
            after.id,
            isPresetDefault(after.alert) ? null : after.alert,
            profile,
          );
        }
        if (switches.length === 0 && !edited) return;
        // The preset plan builds the presets that are on with your edits
        // laid over them and lands the switches on the stored list, so a
        // preset another window turned on meanwhile stays on.
        await runPresetPlan(profile ?? null, switches);
        if (switches.length === 0) return;
        const { enabled_presets } = await getUiConfig(profile);
        setConfig((prev) => (prev ? { ...prev, enabled_presets } : prev));
      },
      // Follow a change another window made to the profile Settings
      // shows. In loadout mode every profile shares the list and the edits.
      subscribe: async (onChange) => {
        const follow = (profile: string | null) => {
          const shown = getShownProfile();
          if (pathB || profile === null || shown === undefined || profile === shown) onChange();
        };
        const stops = await Promise.all([onPresetsChanged(follow), onPresetEditsChanged(follow)]);
        return () => stops.forEach((stop) => stop());
      },
      entry: (t) => {
        const alert = alertPresetById(t.id);
        if (alert) {
          return {
            name: alert.name,
            group: ALERTS_CATEGORY,
            enabled: t.enabled,
            text: searchText(alert.name, alert.description, ALERTS_CATEGORY),
            anchor: presetAnchor(t.id),
            // An alert preset with parts of its own in [alerts] is edited.
            ...(t.alert && !isPresetDefault(t.alert) ? { edited: true } : {}),
          };
        }
        const preset = presetById(t.id);
        const category = preset ? PRESET_CATEGORIES[preset.category] : '';
        const world = suggestedWorld();
        return {
          name: preset?.name ?? t.id,
          group: category,
          enabled: t.enabled,
          text: searchText(preset?.name, preset?.description, category),
          anchor: presetAnchor(t.id),
          ...(world && preset?.suggest.includes(world) ? { dot: 'suggested' as const } : {}),
          ...(hasEdits(t.edit) ? { edited: true } : {}),
          ...fixWarn(preset, t.edit),
        };
      },
      keyOf: (t) => t.id,
      renderDetail: (props) =>
        isAlertPresetId(props.value.id) ? (
          <AlertPresetDetail {...props} onError={onError} />
        ) : (
          <PresetDetail {...props} onOpenTriggers={onOpenTriggers} />
        ),
    }),
    [setConfig, pathB, onError, onOpenTriggers],
  );

  return (
    <SamplePaintContext.Provider value={paint}>
      <DraftEditor
        spec={spec}
        json={false}
        onJson={() => {}}
        onDirty={onDirty}
        onError={onError}
        selectKey={selectPreset}
      />
    </SamplePaintContext.Provider>
  );
}

/** The card of a preset of the library, as Presets board 1 draws it:
 *  the description, Looks like, Colors, Suggested, Adds and Your changes,
 *  with Reset to preset under it while it holds your edits. Its links
 *  open Triggers while the preset is on, and read as plain text while it
 *  is off, since Triggers then holds none of its triggers. */
export function PresetDetail({
  value: t,
  update,
  onOpenTriggers,
}: DetailProps<PresetToggle> & { onOpenTriggers: OpenTriggers }) {
  const preset = presetById(t.id);
  if (!preset) return null;
  const binds = preset.macros ?? [];
  const link = (text: string, to: Omit<TriggersLink, 'seq'>): ReactNode =>
    t.enabled ? (
      <button type="button" className="st-auto-link" onClick={() => onOpenTriggers(to)}>
        {text}
      </button>
    ) : (
      text
    );
  const adds = [
    ...(preset.triggers.length > 0
      ? [link(countPhrase(preset.triggers.length, TRIGGER_NOUN), { filter: preset.name })]
      : []),
    ...(binds.length > 0 ? [countPhrase(binds.length, MACRO_NOUN)] : []),
  ];
  const changes = yourChanges(preset, t.edit, link);
  return (
    <>
      <Card className="st-auto-card">
        <Row label={preset.name} description={preset.description}>
          <Toggle checked={t.enabled} onChange={(enabled) => update((v) => ({ ...v, enabled }))} />
        </Row>
        {preset.sample.length > 0 && (
          <div className="st-row st-auto-block">
            <div className="st-row-text">
              <span className="st-row-label">Looks like</span>
            </div>
            <PresetSample preset={preset} colors={editColors(t.edit)} />
          </div>
        )}
        <PresetColors
          preset={preset}
          edit={t.edit}
          onColor={(key, value) =>
            update((v) => withEdit(v, withColorEdit(preset, v.edit, key, value)))
          }
          onKeep={(key) => update((v) => withEdit(v, v.edit && withColorKept(preset, v.edit, key)))}
        />
        {preset.suggest.length > 0 && (
          <Row label="Suggested">
            <span className="st-auto-value">For {listJoin(preset.suggest)}</span>
          </Row>
        )}
        <Row label="Adds">
          <span className="st-auto-value">{joined(adds, ' and ')}</span>
        </Row>
        {binds.length > 0 && <PresetKeys preset={preset} />}
        {changes && (
          <Row
            label="Your changes"
            description={t.enabled ? undefined : 'Kept while the preset is off.'}
          >
            <span className="st-auto-value">{changes}</span>
          </Row>
        )}
      </Card>
      {hasEdits(t.edit) && <ResetToPreset onReset={() => update((v) => withEdit(v, undefined))} />}
    </>
  );
}

/** The warn ring of a preset a fix changed under your edits, with the
 *  note a reader hears, on or off (board 4). */
function fixWarn(preset: Preset | undefined, edit: PresetEdit | undefined): { warn?: string } {
  const count = preset ? flagCount(preset, edit) : 0;
  if (count === 0) return {};
  return {
    warn: `A fix to this preset changed ${count === 1 ? 'a row' : `${count} rows`} you edited.`,
  };
}

/** `parts` with `between` between each two. */
function joined(parts: readonly ReactNode[], between: string): ReactNode {
  return parts.map((part, i) => (
    <Fragment key={i}>
      {i > 0 && between}
      {part}
    </Fragment>
  ));
}

/** What Your changes says, changesLine with each trigger a link. Null
 *  while you changed nothing. */
function yourChanges(
  preset: Preset,
  edit: PresetEdit | undefined,
  link: (text: string, to: Omit<TriggersLink, 'seq'>) => ReactNode,
): ReactNode {
  const line = changesLine(preset, edit);
  if (!line) return null;
  if ('count' in line) return line.count;
  return joined(
    [...line.colors, ...line.triggers.map((name) => link(name, { select: name }))],
    ', ',
  );
}

/** Reset to preset, under the card where Delete sits for your own items.
 *  It waits for Save like any other change, so Discard brings your edits
 *  back. */
function ResetToPreset({ onReset }: { onReset: () => void }) {
  return (
    <div className="st-auto-detail-actions">
      <Button onClick={onReset}>Reset to preset</Button>
    </div>
  );
}

/** `t` holding `edit`, or no edit at all once it is undefined. */
function withEdit(t: PresetToggle, edit: PresetEdit | undefined): PresetToggle {
  const { edit: _old, ...rest } = t;
  return edit ? { ...rest, edit } : rest;
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
    <>
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
      {!isPresetDefault(alert) && (
        <ResetToPreset onReset={() => update((v) => ({ ...v, alert: PRESET_ALERT_DEFAULT }))} />
      )}
    </>
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
