import { useEffect, useState } from 'react';
import { AliasForm } from '../../AliasForm';
import { ImportTab } from '../../ImportTab';
import { LoadoutsTab } from '../../LoadoutsTab';
import { MacrosTab } from '../../MacrosTab';
import { TimersTab } from '../../TimersTab';
import { TriggerForm } from '../../TriggerForm';
import { exportAliases, exportTriggers, importAliases, importTriggers } from '../../../lib/session';
import type { SettingsTarget } from '../../../lib/settingsNav';
import {
  EditorModeSwitcher,
  JsonTab,
  LegacyIsland,
  TickConfigEditor,
} from '../legacy/LegacyEditors';
import type { SettingsPageProps } from '../pageTypes';
import { Button, Card, Section, Segmented, type SegmentedOption } from '../ui';

// Placeholder for the Automation board (SettingsAutomation.dc.html).
// The board's kind switcher picks which old editor shows, so every
// trigger, alias, macro, timer, and tick setting stays in reach until
// the list and detail editor lands. Import… on the switcher row opens
// the old import tab. The tick and import targets pick a view here and
// need no scrolling, since each view starts under the switcher.
// Replace this whole component with the board.

type Kind = 'triggers' | 'aliases' | 'macros' | 'timers' | 'presets' | 'loadouts';
type View = Kind | 'import';

const KINDS: readonly SegmentedOption<Kind>[] = [
  { value: 'triggers', label: 'Triggers' },
  { value: 'aliases', label: 'Aliases' },
  { value: 'macros', label: 'Macros' },
  { value: 'timers', label: 'Timers' },
  { value: 'presets', label: 'Presets' },
];

const LOADOUTS: SegmentedOption<Kind> = { value: 'loadouts', label: 'Loadouts' };

const isKind = (value: string | undefined): value is Kind =>
  value === 'loadouts' || KINDS.some((k) => k.value === value);

/** The view a target opens, or null to keep the current one. */
function viewFor(target: SettingsTarget): View | null {
  if (target.anchor === 'import') return 'import';
  if (isKind(target.section)) return target.section;
  return null;
}

export function AutomationGroup({ target, navSeq, onError, pathB }: SettingsPageProps) {
  const [view, setView] = useState<View>(() => viewFor(target) ?? 'triggers');

  useEffect(() => {
    const next = viewFor(target);
    if (next) setView(next);
    // navSeq marks each navigation, even to the same target.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [navSeq]);

  const kinds = pathB ? [...KINDS, LOADOUTS] : KINDS;

  return (
    <>
      <div className="st-toolbar">
        <Segmented
          label="Kind"
          options={kinds}
          value={view === 'import' ? null : view}
          onChange={setView}
        />
        <Button onClick={() => setView('import')}>Import…</Button>
      </div>

      {view === 'triggers' && (
        <Card>
          <LegacyIsland>
            <EditorModeSwitcher
              modeKey="triggers"
              formRender={() => (
                <TriggerForm load={exportTriggers} save={importTriggers} onError={onError} />
              )}
              jsonRender={() => (
                <JsonTab
                  kind="triggers"
                  singular="trigger"
                  load={exportTriggers}
                  save={importTriggers}
                  onError={onError}
                />
              )}
            />
          </LegacyIsland>
        </Card>
      )}

      {view === 'aliases' && (
        <Card>
          <LegacyIsland>
            <EditorModeSwitcher
              modeKey="aliases"
              formRender={() => (
                <AliasForm load={exportAliases} save={importAliases} onError={onError} />
              )}
              jsonRender={() => (
                <JsonTab
                  kind="aliases"
                  singular="alias"
                  plural="aliases"
                  load={exportAliases}
                  save={importAliases}
                  onError={onError}
                />
              )}
            />
          </LegacyIsland>
        </Card>
      )}

      {view === 'macros' && (
        <Card>
          <LegacyIsland>
            <MacrosTab onError={onError} />
          </LegacyIsland>
        </Card>
      )}

      {view === 'timers' && (
        <>
          <Section title="Tick">
            <LegacyIsland>
              <TickConfigEditor onError={onError} />
            </LegacyIsland>
          </Section>
          <Section title="Timers">
            <LegacyIsland>
              <TimersTab onError={onError} />
            </LegacyIsland>
          </Section>
        </>
      )}

      {view === 'presets' && (
        <Card>
          <p className="st-note" data-interim="">
            The preset list is not ready yet. Vosh keeps installing the presets your profile turns
            on.
          </p>
        </Card>
      )}

      {view === 'loadouts' && (
        <Card>
          <LegacyIsland>
            <LoadoutsTab onError={onError} />
          </LegacyIsland>
        </Card>
      )}

      {view === 'import' && (
        <Section title="Import from another client">
          <LegacyIsland>
            <ImportTab onError={onError} />
          </LegacyIsland>
        </Section>
      )}
    </>
  );
}
