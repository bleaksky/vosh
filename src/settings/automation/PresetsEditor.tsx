import { useMemo } from 'react';
import { countPhrase, draftValues } from '../../automation/automationDraft';
import { searchText } from '../../automation/automationList';
import {
  presetSavePlan,
  presetToggles,
  storedPresetIds,
  type PresetToggle,
} from '../../automation/automationRecords';
import {
  PRESET_CATEGORIES,
  presetById,
  presetMacros,
  PRESETS,
  presetTriggers,
} from '../../automation/presets';
import { presetsInstall, presetsRemove } from '../../ipc/automation';
import { getUiConfig, setUiFields } from '../../ipc/uiConfig';
import type { SetUiConfig } from '../pageTypes';
import { Card, Row, Toggle } from '../../ui';
import { DraftEditor } from './DraftEditor';
import type { DetailProps, DirtyReport, KindSpec } from './types';

const TRIGGER_NOUN = { one: 'trigger', many: 'triggers' };

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

function PresetDetail({ value: t, update }: DetailProps<PresetToggle>) {
  const preset = presetById(t.id);
  if (!preset) return null;
  return (
    <Card className="st-auto-card">
      <Row label={preset.name} description={preset.description}>
        <Toggle checked={t.enabled} onChange={(enabled) => update((v) => ({ ...v, enabled }))} />
      </Row>
      <Row label="Adds">
        <span className="st-auto-value">{countPhrase(preset.triggers.length, TRIGGER_NOUN)}</span>
      </Row>
    </Card>
  );
}
