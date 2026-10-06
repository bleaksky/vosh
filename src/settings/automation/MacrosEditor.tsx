import { useEffect, useRef } from 'react';
import { groupKeyOf, searchText } from '../../automation/automationList';
import {
  blankMacro,
  jsonListText,
  normalizeMacro,
  parseJsonList,
  saveMacroDraft,
  validateMacros,
  type MacroRecord,
} from '../../automation/automationRecords';
import { withGroup } from '../../automation/automationTriggers';
import { labelForKey } from '../../automation/macroKeys';
import { listMacros, subscribeMacrosChanged } from '../../ipc/automation';
import { isMacPlatform } from '../../lib/shortcuts';
import { Card, Field, Row, Toggle } from '../../ui';
import { CardNote, GroupField, KeyCaptureField } from './fields';
import { DraftEditor } from './DraftEditor';
import { macroClashNote } from './macroClash';
import type { DetailProps, EditorProps, KindSpec } from './types';

const MACROS_SPEC: KindSpec<MacroRecord> = {
  id: 'macros',
  groups: 'macros',
  noun: { one: 'macro', many: 'macros' },
  filterLabel: 'Filter macros',
  newLabel: 'New macro',
  deleteLabel: 'Delete macro',
  emptyDetail: 'Choose a macro to edit it.',
  emptyList: 'You have no macros yet.',
  load: async () => (await listMacros()).map(normalizeMacro),
  // One call per binding, the way the old Macros tab saved rows.
  // Unbinding goes first, so a key another macro takes over stays bound.
  save: (draft, written) => saveMacroDraft(draft, written),
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
  // A key the session keys share stays with this macro in the sessions
  // on its profile (Sessions Q11), and the card says what it does
  // elsewhere.
  const clash = macroClashNote(m.key, isMacPlatform());

  return (
    <Card className="st-auto-card">
      {clash && <CardNote>{clash}</CardNote>}
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
