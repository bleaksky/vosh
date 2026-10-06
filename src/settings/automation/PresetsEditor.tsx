import { useMemo } from 'react';
import { countPhrase, draftValues } from '../../automation/automationDraft';
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
import { presetsInstall, presetsRemove } from '../../ipc/automation';
import { getUiConfig, setUiFields } from '../../ipc/uiConfig';
import { listJoin } from '../../lib/text';
import { useMacroList } from '../../stores/config/macroListStore';
import type { SetUiConfig } from '../pageTypes';
import { Card, CardNote, cx, Keycap, Row, Toggle } from '../../ui';
import { DraftEditor } from './DraftEditor';
import type { DetailProps, DirtyReport, KindSpec } from './types';

const TRIGGER_NOUN = { one: 'trigger', many: 'triggers' };
const MACRO_NOUN = { one: 'macro', many: 'macros' };

interface PresetsEditorProps {
  setConfig: SetUiConfig;
  onDirty: (report: DirtyReport | null) => void;
  onError: (message: string | null) => void;
  /** Each profile keeps its own list. In loadout mode every profile
   *  shares one, next to the preset triggers in the shared catalog. */
  profileScoped: boolean;
}

/** The presets, one toggle each under its category. Save installs the
 *  triggers and macros of the presets you turned on, removes the ones you
 *  turned off, and stores the list in enabled_presets, which launch reads
 *  to put the ones that are on back and take the rest out. */
export function PresetsEditor({ setConfig, onDirty, onError, profileScoped }: PresetsEditorProps) {
  const spec = useMemo<KindSpec<PresetToggle>>(
    () => ({
      id: 'presets',
      noun: { one: 'preset', many: 'presets' },
      filterLabel: 'Filter presets',
      emptyDetail: 'Choose a preset to see what it adds.',
      emptyList: 'Vosh has no presets.',
      // Read the stored list fresh, so a profile switch loads the new
      // profile's presets. In loadout mode the list is shared, and a
      // switch keeps it.
      load: async () => presetToggles((await getUiConfig()).enabled_presets),
      save: async (draft) => {
        const plan = presetSavePlan(draft);
        for (const id of plan.remove) await presetsRemove(id);
        const on = PRESETS.filter((p) => plan.install.includes(p.id));
        const triggers = on.flatMap(presetTriggers);
        const macros = on.flatMap(presetMacros);
        if (triggers.length > 0 || macros.length > 0) await presetsInstall(triggers, macros);
        // Keep the ids this page has no preset for, such as the alert
        // presets, as the profile holds them now.
        const stored = (await getUiConfig()).enabled_presets;
        const enabled_presets = storedPresetIds(draftValues(draft), stored);
        await setUiFields({ enabled_presets });
        setConfig((prev) => (prev ? { ...prev, enabled_presets } : prev));
      },
      entry: (t) => {
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
      renderDetail: (props) => <PresetDetail {...props} />,
    }),
    [setConfig],
  );

  return (
    <DraftEditor
      spec={spec}
      json={false}
      onJson={() => {}}
      onDirty={onDirty}
      onError={onError}
      profileScoped={profileScoped}
    />
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
