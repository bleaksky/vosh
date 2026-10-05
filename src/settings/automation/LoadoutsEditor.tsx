import { useMemo, useRef } from 'react';
import { draftValues, updateDraftItem } from '../../automation/automationDraft';
import { searchText } from '../../automation/automationList';
import {
  activeLoadouts,
  loadoutToggles,
  type LoadoutToggle,
} from '../../automation/automationRecords';
import {
  loadoutsGetState,
  loadoutsSetActive,
  subscribeLoadoutsChanged,
  type LoadoutSummary,
} from '../../ipc/loadouts';
import { Button, Card, Chip, Row, Toggle } from '../../ui';
import { DraftEditor } from './DraftEditor';
import type { DetailProps, DirtyReport, KindSpec } from './types';

interface LoadoutsEditorProps {
  onDirty: (report: DirtyReport | null) => void;
  onError: (message: string | null) => void;
}

/** Loadout mode only. Each loadout turns on its groups of aliases,
 *  triggers, and macros in the shared catalog, and Vosh runs the groups
 *  of every active loadout. Save sets the active list. */
export function LoadoutsEditor({ onDirty, onError }: LoadoutsEditorProps) {
  const summaries = useRef(new Map<string, LoadoutSummary>());

  const spec = useMemo<KindSpec<LoadoutToggle>>(
    () => ({
      id: 'loadouts',
      noun: { one: 'loadout', many: 'loadouts' },
      filterLabel: 'Filter loadouts',
      emptyDetail: 'Choose a loadout to see its groups.',
      emptyList: 'You have no loadouts yet.',
      load: async () => {
        const state = await loadoutsGetState();
        summaries.current = new Map(state.loadouts.map((l) => [l.name, l]));
        return loadoutToggles(state.loadouts, state.active);
      },
      save: async (draft) => {
        await loadoutsSetActive(activeLoadouts(draftValues(draft)));
      },
      entry: (t) => {
        const summary = summaries.current.get(t.name);
        return {
          name: t.name,
          group: '',
          enabled: t.active,
          text: searchText(t.name, summary?.description, summary?.enabled_groups.join(' ')),
        };
      },
      keyOf: (t) => t.name,
      subscribe: (onChange) => subscribeLoadoutsChanged(onChange),
      renderDetail: (props) => (
        <LoadoutDetail {...props} summary={summaries.current.get(props.value.name)} />
      ),
    }),
    [],
  );

  return (
    <DraftEditor
      spec={spec}
      json={false}
      onJson={() => {}}
      onDirty={onDirty}
      onError={onError}
      barExtra={(draft, setDraft) => (
        <Button
          disabled={!draft.items.some((item) => item.value.active)}
          onClick={() => {
            let next = draft;
            for (const item of draft.items) {
              next = updateDraftItem(next, item.uid, (v) => ({ ...v, active: false }));
            }
            setDraft(next);
          }}
        >
          Turn all off
        </Button>
      )}
    />
  );
}

function LoadoutDetail({
  value: t,
  update,
  summary,
}: DetailProps<LoadoutToggle> & { summary: LoadoutSummary | undefined }) {
  const groups = summary?.enabled_groups ?? [];
  return (
    <Card className="st-auto-card">
      <Row label={t.name} description={summary?.description ?? undefined}>
        <Toggle checked={t.active} onChange={(active) => update((v) => ({ ...v, active }))} />
      </Row>
      <Row label="Groups">
        {groups.length > 0 ? (
          <ul className="st-auto-chips" aria-label="Groups">
            {groups.map((g) => (
              <Chip key={g} as="li">
                {g}
              </Chip>
            ))}
          </ul>
        ) : (
          <span className="st-auto-value">None</span>
        )}
      </Row>
    </Card>
  );
}
