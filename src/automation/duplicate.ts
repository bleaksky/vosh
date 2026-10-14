import type { TriggerRecord } from '../ipc/automation';
import type { AliasRecord, MacroRecord } from './automationRecords';

// The copies Duplicate makes in Settings. Each one saves beside its
// original untouched: a trigger or alias takes "copy" on the end of its
// name, and a number after that when the name is taken. A copy is
// always yours, even of a preset's item.

/** The first of `base`, then `base` with 2, 3 and so on after `gap`,
 *  that no name in `taken` holds. */
function freeName(base: string, gap: string, taken: ReadonlySet<string>): string {
  if (!taken.has(base)) return base;
  for (let n = 2; ; n += 1) {
    const name = `${base}${gap}${n}`;
    if (!taken.has(name)) return name;
  }
}

/** A copy of trigger `t`, named `<name> copy`, with its patterns and
 *  actions its own to change. */
export function duplicateTrigger(t: TriggerRecord, all: readonly TriggerRecord[]): TriggerRecord {
  const { preset: _preset, kept: _kept, ...rest } = t as TriggerRecord & { kept?: string[] };
  const copy = structuredClone(rest) as TriggerRecord;
  const taken = new Set(all.map((other) => other.name.trim()));
  copy.name = freeName(`${t.name.trim()} copy`, ' ', taken);
  return copy;
}

/** A copy of alias `a`, named `<name>-copy`, since an alias name is
 *  one word. */
export function duplicateAlias(a: AliasRecord, all: readonly AliasRecord[]): AliasRecord {
  const taken = new Set(all.map((other) => other.name.trim()));
  return { ...a, name: freeName(`${a.name.trim()}-copy`, '', taken) };
}

/** A copy of macro `m` with no key yet, since one key runs one macro.
 *  You press a key for it before it saves. */
export function duplicateMacro(m: MacroRecord): MacroRecord {
  const { preset: _preset, ...rest } = m;
  return { ...rest, key: '' };
}
