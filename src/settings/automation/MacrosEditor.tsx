import { useEffect, useMemo, useRef } from 'react';
import { groupKeyOf, searchText } from '../../automation/automationList';
import {
  blankMacro,
  jsonListText,
  keptKeyNote,
  normalizeMacro,
  parseJsonList,
  saveMacroDraft,
  validateMacros,
  type MacroRecord,
} from '../../automation/automationRecords';
import { withGroup } from '../../automation/automationTriggers';
import { labelForKey } from '../../automation/macroKeys';
import { presetById } from '../../automation/presets';
import { listMacros, subscribeMacrosChanged, type Macro } from '../../ipc/automation';
import { useMacroList } from '../../stores/config/macroListStore';
import { Card, CardNote, Field, Row, Toggle } from '../../ui';
import { GroupField, KeyCaptureField } from './fields';
import { DraftEditor } from './DraftEditor';
import type { DetailProps, EditorProps, KindSpec } from './types';

const MACROS_SPEC: KindSpec<MacroRecord> = {
  id: 'macros',
  groups: 'macros',
  noun: { one: 'macro', many: 'macros' },
  filterLabel: 'Filter macros',
  newLabel: 'New macro',
  deleteLabel: 'Delete macro',
  // Presets put their macros back at launch. Turn the preset off.
  canDelete: (m) => !m.preset,
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
    preset: Boolean(m.preset),
    text: searchText(m.key, m.command, m.group),
  }),
  // A preset macro can share its key with yours.
  keyOf: (m) => `${m.preset ?? ''}\n${m.key}`,
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
  // Your macro on a key a preset macro wants carries the warn ring.
  const warnNotes = useKeptKeyNotes();
  return <DraftEditor spec={MACROS_SPEC} {...props} warnNotes={warnNotes} />;
}

/** Each of your macros whose key a preset macro wants, by its row name,
 *  with what its ring and its card say. Read from the store, so a key
 *  you move shows once you save. Rust holds the preset macro on that key
 *  off (hold_taken_keys in src-tauri/src/loadouts/presets.rs). */
function useKeptKeyNotes(): ReadonlyMap<string, string> {
  const macros = useMacroList();
  return useMemo(() => keptKeyNotes(macros), [macros]);
}

function keptKeyNotes(macros: readonly Macro[]): ReadonlyMap<string, string> {
  const yours = new Set(macros.filter((m) => !m.preset).map((m) => m.key));
  const notes = new Map<string, string>();
  for (const m of macros) {
    const preset = m.preset ? presetById(m.preset) : undefined;
    if (!preset || !yours.has(m.key)) continue;
    const key = labelForKey(m.key);
    notes.set(
      key,
      `${preset.name} also wants ${key}, for ${m.command}. Your macro keeps the key, so ${m.command} has no key until you move this one.`,
    );
  }
  return notes;
}

function MacroDetail({ value: m, update, fresh, revealInList }: DetailProps<MacroRecord>) {
  const keyRef = useRef<HTMLInputElement | null>(null);
  // A preset macro changes only its group here, as a preset trigger does.
  const locked = Boolean(m.preset);
  // Your macro keeps a key a preset macro wants, and says so. The preset
  // macro held off on that key says the same as the preset's card.
  const kept = useKeptKeyNotes().get(labelForKey(m.key));
  const warn = kept !== undefined && locked ? keptKeyNote([m]) : kept;

  useEffect(() => {
    if (fresh) keyRef.current?.focus();
  }, [fresh]);

  const set = (patch: Partial<MacroRecord>) => update((v) => ({ ...v, ...patch }));

  return (
    <Card className="st-auto-card">
      {warn && <CardNote tone="warn">{warn}</CardNote>}
      {locked && (
        <CardNote>
          This macro comes from a preset, so only its group changes here. Turn the preset off under
          Presets to remove it.
        </CardNote>
      )}
      <Row label="Key">
        <KeyCaptureField
          ref={keyRef}
          width="100%"
          value={m.key}
          disabled={locked}
          onChange={(key) => set({ key })}
        />
      </Row>
      <Row label="Command">
        <Field
          mono
          width="100%"
          value={m.command}
          disabled={locked}
          onChange={(command) => set({ command })}
        />
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
        <Toggle checked={m.enabled} disabled={locked} onChange={(enabled) => set({ enabled })} />
      </Row>
    </Card>
  );
}
