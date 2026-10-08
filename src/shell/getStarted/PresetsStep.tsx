import { useEffect, useState, type ReactNode } from 'react';
import { enabledPresetIds } from '../../automation/automationRecords';
import type { Preset } from '../../automation/presets';
import { PresetSample } from '../../automation/PresetSampleView';
import { SamplePaintContext, useSamplePaint } from '../../automation/samplePaint';
import { loadoutsGetState } from '../../ipc/loadouts';
import { getUiConfig, type UiConfig } from '../../ipc/uiConfig';
import { knownWorld } from '../../lib/knownWorlds';
import { openSettingsTab } from '../../lib/settingsLink';
import { countWord } from '../../lib/text';
import { Button, Toggle } from '../../ui';
import { suggestedPresets, type GetStartedFacts } from './steps';
import { switchPresets } from './switchPresets';

// The presets step of board 3: the presets suggested for the world you
// connect to, outside Chat, each with its description from presets.ts
// and its sample in the colors it paints on your theme. A switch turns
// one on and saves at once, and Turn on all turns on every one that is
// off in a single call (Q4, Q5, Q17).

/** The order board 3 lists the suggestions in, as Q5 names them: the
 *  room, a fight from both sides, a cure and what you gain. */
const ORDER = [
  'room_and_time',
  'combat_outgoing',
  'combat_incoming',
  'healing_basics',
  'loot_progression',
];

/** The suggestions in board 3's order, any the board does not name
 *  last. */
function listed(host: string): Preset[] {
  const at = (p: Preset) => (ORDER.includes(p.id) ? ORDER.indexOf(p.id) : ORDER.length);
  return [...suggestedPresets(host)].sort((a, b) => at(a) - at(b));
}

/** The presets on, as the step reads them, or null until Vosh reads the
 *  stored list. */
function presetsOn(facts: GetStartedFacts): Set<string> | null {
  return facts.enabledPresets ? new Set(enabledPresetIds(facts.enabledPresets)) : null;
}

/** Where Open Presets lands: the first suggestion that is off, the
 *  anchor B5 added, or the list. */
function presetsLink(host: string, facts: GetStartedFacts): string {
  const on = presetsOn(facts);
  const off = listed(host).find((p) => !on?.has(p.id));
  return off ? `automation:presets#presets:${off.id}` : 'automation:presets';
}

/** Open Settings on Presets. */
export function OpenPresets({ host, facts }: { host: string; facts: GetStartedFacts }) {
  return <Button onClick={() => openSettingsTab(presetsLink(host, facts))}>Open Presets</Button>;
}

export function PresetsStep({ host, facts }: { host: string; facts: GetStartedFacts }) {
  const [config, setConfig] = useState<UiConfig | null>(null);
  // In loadout mode every character shares one list (Q17).
  const [shared, setShared] = useState(false);
  useEffect(() => {
    let alive = true;
    getUiConfig()
      .then((cfg) => alive && setConfig(cfg))
      .catch((e: unknown) => console.error('[get started] reading the config failed', e));
    loadoutsGetState()
      .then((state) => alive && setShared(state.path_b_active))
      .catch((e: unknown) => console.error('[get started] reading loadout mode failed', e));
    return () => {
      alive = false;
    };
  }, []);

  const suggested = listed(host);
  const on = presetsOn(facts);
  const off = suggested.filter((p) => !on?.has(p.id));
  const list = (
    <ul className="gs-sugs">
      {suggested.map((preset) => (
        <li key={preset.id} className="gs-sug">
          <div className="gs-sug-top">
            <span className="gs-sug-name">{preset.name}</span>
            <Toggle
              checked={on?.has(preset.id) ?? false}
              aria-label={preset.name}
              disabled={on === null}
              onChange={(next) => void switchPresets([{ id: preset.id, on: next }])}
            />
          </div>
          <p className="gs-sug-line">{preset.description}</p>
          <PresetSample preset={preset} className="gs-sample" />
        </li>
      ))}
    </ul>
  );

  return (
    <>
      <p className="pc-copy">
        {shared
          ? 'Each switch saves at once, for every character.'
          : 'Each switch saves to this profile at once.'}
      </p>
      <div className="gs-sec">
        <span className="gs-sec-title">Suggested for {knownWorld(host)?.name}</span>
        <Button
          className="st-auto-quiet"
          disabled={on === null || off.length === 0}
          onClick={() => void switchPresets(off.map((p) => ({ id: p.id, on: true })))}
        >
          Turn on all {countWord(suggested.length).toLowerCase()}
        </Button>
      </div>
      {config ? <Painted config={config}>{list}</Painted> : list}
      <p className="gs-end">Settings lists every preset under Automation, Presets.</p>
    </>
  );
}

/** The samples in your terminal's colors, once Vosh reads them. */
function Painted({ config, children }: { config: UiConfig; children: ReactNode }) {
  const paint = useSamplePaint(config);
  return <SamplePaintContext.Provider value={paint}>{children}</SamplePaintContext.Provider>;
}
