import { useEffect, useMemo, useRef } from 'react';
import { countPhrase, draftValues } from '../../../../lib/automationDraft';
import { searchText } from '../../../../lib/automationList';
import {
  presetSavePlan,
  presetToggles,
  storedPresetIds,
  type PresetToggle,
} from '../../../../lib/automationRecords';
import { PRESET_CATEGORIES, presetById, PRESETS, presetTriggers } from '../../../../lib/presets';
import {
  getUiConfig,
  presetsInstall,
  presetsRemove,
  setUiConfig,
  type UiConfig,
} from '../../../../lib/session';
import type { SetUiConfig } from '../../pageTypes';
import { Card, Row, Toggle } from '../../ui';
import { DraftEditor } from './DraftEditor';
import type { DetailProps, DirtyReport, KindSpec } from './types';

const TRIGGER_NOUN = { one: 'trigger', many: 'triggers' };

interface PresetsEditorProps {
  config: UiConfig;
  setConfig: SetUiConfig;
  onDirty: (report: DirtyReport | null) => void;
  onError: (message: string | null) => void;
}

/** The trigger presets, one toggle each under its category. Save
 *  installs the presets you turned on, removes the ones you turned off,
 *  and stores the list in enabled_presets, which launch reads to put
 *  them back. */
export function PresetsEditor({ config, setConfig, onDirty, onError }: PresetsEditorProps) {
  const configRef = useRef(config);
  useEffect(() => {
    configRef.current = config;
  }, [config]);

  const spec = useMemo<KindSpec<PresetToggle>>(
    () => ({
      noun: { one: 'preset', many: 'presets' },
      filterLabel: 'Filter presets',
      emptyDetail: 'Choose a preset to see what it adds.',
      emptyList: 'Vosh has no presets.',
      // Read the stored list fresh, so a profile switch loads the new
      // profile's presets.
      load: async () => presetToggles((await getUiConfig()).enabled_presets),
      save: async (draft) => {
        const plan = presetSavePlan(draft);
        for (const id of plan.remove) await presetsRemove(id);
        const install = PRESETS.filter((p) => plan.install.includes(p.id)).flatMap(presetTriggers);
        if (install.length > 0) await presetsInstall(install);
        const enabled_presets = storedPresetIds(draftValues(draft));
        const next = { ...configRef.current, enabled_presets };
        await setUiConfig(next);
        configRef.current = next;
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
      profileScoped
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
