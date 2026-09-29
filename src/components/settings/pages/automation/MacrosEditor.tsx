import { useEffect, useRef } from 'react';
import { groupKeyOf, searchText } from '../../../../lib/automationList';
import {
  blankMacro,
  jsonListText,
  macroSavePlan,
  normalizeMacro,
  parseJsonList,
  validateMacros,
  type MacroRecord,
} from '../../../../lib/automationRecords';
import { withGroup } from '../../../../lib/automationTriggers';
import { labelForKey } from '../../../../lib/macroKeys';
import { deleteMacro, listMacros, setMacro, subscribeMacrosChanged } from '../../../../lib/session';
import { Card, Field, Row, Toggle } from '../../ui';
import { GroupField, KeyCaptureField } from './fields';
import { DraftEditor } from './DraftEditor';
import type { DetailProps, EditorProps, KindSpec } from './types';

const MACROS_SPEC: KindSpec<MacroRecord> = {
  noun: { one: 'macro', many: 'macros' },
  filterLabel: 'Filter macros',
  newLabel: 'New macro',
  deleteLabel: 'Delete macro',
  emptyDetail: 'Choose a macro to edit it.',
  emptyList: 'You have no macros yet.',
  load: async () => (await listMacros()).map(normalizeMacro),
  // One call per binding, the way the old Macros tab saved rows.
  // Unbinding goes first, so a key another macro takes over stays bound.
  save: async (draft) => {
    const plan = macroSavePlan(draft);
    for (const key of plan.remove) await deleteMacro(key);
    for (const m of plan.set) await setMacro(m.key, m.command, m.group ?? null, m.enabled);
  },
  validate: validateMacros,
  entry: (m) => ({
    name: m.key ? labelForKey(m.key) : '',
    meta: m.command,
    group: groupKeyOf(m.group),
    enabled: m.enabled,
    text: searchText(m.key, m.command, m.group),
  }),
  keyOf: (m) => m.key,
  blank: blankMacro,
  json: {
    toText: jsonListText,
    fromText: (text) => parseJsonList(text, normalizeMacro),
  },
  subscribe: (onChange) => subscribeMacrosChanged(() => onChange()),
  renderDetail: (props) => <MacroDetail {...props} />,
  monoName: true,
  monoMeta: true,
};

export function MacrosEditor(props: EditorProps) {
  return <DraftEditor spec={MACROS_SPEC} {...props} />;
}

function MacroDetail({ value: m, update, fresh, revealInList }: DetailProps<MacroRecord>) {
  const keyRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (fresh) keyRef.current?.focus();
  }, [fresh]);

  const set = (patch: Partial<MacroRecord>) => update((v) => ({ ...v, ...patch }));

  return (
    <Card className="st-auto-card">
      <Row label="Key">
        <KeyCaptureField ref={keyRef} width="100%" value={m.key} onChange={(key) => set({ key })} />
      </Row>
      <Row label="Command">
        <Field mono width="100%" value={m.command} onChange={(command) => set({ command })} />
      </Row>
      <Row label="Group">
        <GroupField
          width="100%"
          value={m.group ?? ''}
          onCommit={(group) => {
            update((v) => withGroup(v, group));
            revealInList();
          }}
        />
      </Row>
      <Row label="Enabled">
        <Toggle checked={m.enabled} onChange={(enabled) => set({ enabled })} />
      </Row>
    </Card>
  );
}
