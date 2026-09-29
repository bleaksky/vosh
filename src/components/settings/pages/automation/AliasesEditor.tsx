import { useEffect, useId, useRef, useState } from 'react';
import { groupKeyOf, searchText } from '../../../../lib/automationList';
import {
  aliasKey,
  blankAlias,
  jsonListText,
  loadAliases,
  normalizeAlias,
  parseJsonList,
  saveAliasDraft,
  validateAliases,
  type AliasRecord,
} from '../../../../lib/automationRecords';
import { withGroup } from '../../../../lib/automationTriggers';
import { subscribeAliasGroupsChanged } from '../../../../lib/session';
import { Card, Disclosure, Field, FieldArea, Row, Toggle } from '../../ui';
import { CodeRow, GroupField } from './fields';
import { DraftEditor } from './DraftEditor';
import type { DetailProps, EditorProps, KindSpec } from './types';

const ALIASES_SPEC: KindSpec<AliasRecord> = {
  noun: { one: 'alias', many: 'aliases' },
  filterLabel: 'Filter aliases',
  newLabel: 'New alias',
  deleteLabel: 'Delete alias',
  emptyDetail: 'Choose an alias to edit it.',
  emptyList: 'You have no aliases yet.',
  load: () => loadAliases(),
  save: (draft) => saveAliasDraft(draft),
  validate: validateAliases,
  entry: (a) => ({
    name: a.name,
    meta: (a.script ?? a.expansion).split('\n')[0],
    group: groupKeyOf(a.group),
    enabled: a.enabled,
    text: searchText(a.name, a.group, a.expansion, a.script),
  }),
  keyOf: aliasKey,
  blank: blankAlias,
  json: {
    toText: jsonListText,
    fromText: (text) => parseJsonList(text, normalizeAlias),
  },
  subscribe: (onChange) => subscribeAliasGroupsChanged(() => onChange()),
  renderDetail: (props) => <AliasDetail {...props} />,
  monoName: true,
  monoMeta: true,
};

export function AliasesEditor(props: EditorProps) {
  return <DraftEditor spec={ALIASES_SPEC} {...props} />;
}

function AliasDetail({ value: a, update, fresh, revealInList }: DetailProps<AliasRecord>) {
  const [advanced, setAdvanced] = useState(a.script !== undefined);
  const advancedId = useId();
  const nameRef = useRef<HTMLInputElement | null>(null);
  const lua = a.script !== undefined;

  useEffect(() => {
    if (fresh) nameRef.current?.focus();
  }, [fresh]);

  const set = (patch: Partial<AliasRecord>) => update((v) => ({ ...v, ...patch }));

  return (
    <Card className="st-auto-card">
      <Row label="Name">
        <Field ref={nameRef} mono width="100%" value={a.name} onChange={(name) => set({ name })} />
      </Row>
      <Row label="Group">
        <GroupField
          width="100%"
          value={a.group ?? ''}
          onCommit={(group) => {
            update((v) => withGroup(v, group));
            revealInList();
          }}
        />
      </Row>
      <Row
        label="Expansion"
        description={lua ? 'Vosh runs the Lua script under Advanced instead.' : undefined}
      >
        <FieldArea
          mono
          width="100%"
          value={a.expansion}
          disabled={lua}
          onChange={(expansion) => set({ expansion })}
        />
      </Row>
      <Row label="Enabled">
        <Toggle checked={a.enabled} onChange={(enabled) => set({ enabled })} />
      </Row>
      <Disclosure
        label="Advanced"
        description="Run a Lua script instead of the expansion."
        expanded={advanced}
        aria-controls={advancedId}
        onClick={() => setAdvanced((open) => !open)}
      />
      {advanced && (
        <div id={advancedId} className="st-auto-advanced">
          <Row
            label="Run Lua instead"
            description="Vosh runs the script and ignores the expansion."
          >
            <Toggle
              checked={lua}
              onChange={(on) =>
                update((v) => {
                  const next = { ...v };
                  if (on) next.script = v.script ?? '';
                  else delete next.script;
                  return next;
                })
              }
            />
          </Row>
          {lua && (
            <CodeRow
              label="Lua script"
              description="The captures table holds the words you type after the alias."
              value={a.script ?? ''}
              onChange={(script) => set({ script })}
            />
          )}
        </div>
      )}
    </Card>
  );
}
