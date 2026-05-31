import { useEffect, useMemo, useState } from 'react';
import type {
  HighlightStyle,
  NamedColor,
  TriggerAction,
  TriggerPattern,
  TriggerRecord,
} from '../lib/session';
import { normalizeActions, normalizePatterns } from '../lib/session';
import { colorize, decolorize } from '../lib/colorTokens';
import { UnsavedDot } from './UnsavedDot';
import { useUnsavedWarning } from '../lib/unsaved';

interface Props {
  load: () => Promise<string>;
  save: (json: string) => Promise<number>;
  onError: (e: string | null) => void;
}

type VisualKind = 'none' | 'highlight' | 'replace' | 'gag';
type EffectKind = 'send' | 'route';

const COLORS: NamedColor[] = [
  'black',
  'red',
  'green',
  'yellow',
  'blue',
  'magenta',
  'cyan',
  'white',
  'bright_black',
  'bright_red',
  'bright_green',
  'bright_yellow',
  'bright_blue',
  'bright_magenta',
  'bright_cyan',
  'bright_white',
];

function blankTrigger(): TriggerRecord {
  return {
    name: '',
    patterns: [{ pattern: '', enabled: true }],
    priority: 5,
    enabled: true,
    actions: [{ kind: 'highlight', style: { fg: 'yellow' } }],
  };
}

// Split a Vec<TriggerAction> into the (at most) one Visual entry
// and zero-or-more Effects entries. Mirrors the form's two
// sections; reverse on save reconstitutes the vec.
function splitActions(actions: TriggerAction[]): {
  visual: Exclude<TriggerAction, { kind: 'send' } | { kind: 'route' }> | null;
  effects: Extract<TriggerAction, { kind: 'send' } | { kind: 'route' }>[];
} {
  let visual: ReturnType<typeof splitActions>['visual'] = null;
  const effects: ReturnType<typeof splitActions>['effects'] = [];
  for (const a of actions) {
    if (a.kind === 'send' || a.kind === 'route') {
      effects.push(a);
    } else if (visual === null) {
      visual = a;
    }
  }
  return { visual, effects };
}

function joinActions(
  visual: ReturnType<typeof splitActions>['visual'],
  effects: ReturnType<typeof splitActions>['effects'],
): TriggerAction[] {
  const out: TriggerAction[] = [];
  if (visual) out.push(visual);
  out.push(...effects);
  return out;
}

function blankVisual(kind: VisualKind): ReturnType<typeof splitActions>['visual'] {
  switch (kind) {
    case 'none':
      return null;
    case 'highlight':
      return { kind: 'highlight', style: { fg: 'yellow' } };
    case 'replace':
      return { kind: 'replace', template: '' };
    case 'gag':
      return { kind: 'gag' };
  }
}

function blankEffect(kind: EffectKind): TriggerAction {
  return kind === 'send' ? { kind: 'send', template: '' } : { kind: 'route', pane: 'chat' };
}

function decolorizeTemplates(t: TriggerRecord): TriggerRecord {
  return {
    ...t,
    actions: t.actions.map((a) =>
      a.kind === 'replace' || a.kind === 'send' ? { ...a, template: decolorize(a.template) } : a,
    ),
  };
}

function colorizeTemplates(t: TriggerRecord): TriggerRecord {
  return {
    ...t,
    actions: t.actions.map((a) =>
      a.kind === 'replace' || a.kind === 'send' ? { ...a, template: colorize(a.template) } : a,
    ),
  };
}

export function TriggerForm({ load, save, onError }: Props) {
  const [list, setList] = useState<TriggerRecord[] | null>(null);
  // Baseline = the last value the backend confirmed. Drives the
  // "● unsaved" indicator so a stray edit the user undid doesn't
  // leave the form looking dirty.
  const [baseline, setBaseline] = useState<string>('[]');
  const [savedAt, setSavedAt] = useState<number | null>(null);
  const dirty = useMemo(() => list !== null && JSON.stringify(list) !== baseline, [list, baseline]);
  useUnsavedWarning(dirty);

  useEffect(() => {
    if (savedAt === null) return;
    const id = window.setTimeout(() => setSavedAt(null), 1500);
    return () => window.clearTimeout(id);
  }, [savedAt]);

  useEffect(() => {
    let cancelled = false;
    load()
      .then((json) => {
        if (cancelled) return;
        let arr: TriggerRecord[] = [];
        try {
          const parsed = JSON.parse(json);
          arr = Array.isArray(parsed)
            ? (parsed as unknown[]).map((row) => {
                const r = row as Record<string, unknown>;
                const out: TriggerRecord = {
                  name: String(r.name ?? ''),
                  patterns: normalizePatterns(row),
                  priority: typeof r.priority === 'number' ? r.priority : 0,
                  enabled: r.enabled !== false,
                  actions: normalizeActions(row),
                };
                if (typeof r.preset === 'string') out.preset = r.preset;
                return out;
              })
            : [];
        } catch {
          arr = [];
        }
        const initial = arr.map(decolorizeTemplates);
        setList(initial);
        setBaseline(JSON.stringify(initial));
      })
      .catch((e) => onError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [load, onError]);

  const userTriggers = useMemo(() => (list ?? []).filter((t) => !t.preset), [list]);
  const presetTriggers = useMemo(() => (list ?? []).filter((t) => !!t.preset), [list]);

  const updateAt = (realIdx: number, patch: Partial<TriggerRecord>) => {
    if (!list || realIdx < 0) return;
    const next = list.slice();
    next[realIdx] = { ...next[realIdx], ...patch };
    setList(next);
  };

  const updateUser = (slot: number, patch: Partial<TriggerRecord>) => {
    if (!list) return;
    updateAt(indexInList(list, slot, false), patch);
  };

  const removeUser = (slot: number) => {
    if (!list) return;
    const idx = indexInList(list, slot, false);
    if (idx < 0) return;
    const next = list.slice();
    next.splice(idx, 1);
    setList(next);
  };

  const addTrigger = () => {
    if (!list) return;
    setList([...list, blankTrigger()]);
  };

  const doSave = async () => {
    if (!list) return;
    try {
      const out = list.map(colorizeTemplates);
      await save(JSON.stringify(out, null, 2));
      setSavedAt(Date.now());
      // Baseline tracks the editor-shape list (decolorized) since
      // that's what the user is comparing against in the UI; the
      // saved wire form is the colorized version.
      setBaseline(JSON.stringify(list));
    } catch (e) {
      onError(String(e));
    }
  };

  if (!list) return <div className="settings-loading">loading triggers…</div>;

  return (
    <div className="trigger-form">
      <div className="settings-triggers-meta">
        <span className="settings-triggers-count">
          {userTriggers.length} user · {presetTriggers.length} preset
        </span>
        <span className="settings-triggers-hint">
          a trigger can hold many patterns (each toggleable) and pair one visual with any number of
          effects
        </span>
      </div>

      <div className="trigger-form-list">
        {userTriggers.map((t, i) => (
          <TriggerCard
            key={`u-${i}`}
            trigger={t}
            onChange={(patch) => updateUser(i, patch)}
            onRemove={() => removeUser(i)}
            readOnly={false}
          />
        ))}
        {presetTriggers.length > 0 && (
          <div className="trigger-form-preset-group">
            <div className="trigger-form-preset-heading">
              presets (auto-installed, edit through code)
            </div>
            {presetTriggers.map((t, i) => (
              <TriggerCard
                key={`p-${i}`}
                trigger={t}
                onChange={() => undefined}
                onRemove={() => undefined}
                readOnly={true}
              />
            ))}
          </div>
        )}
      </div>

      <div className="settings-actions">
        <button type="button" className="settings-btn" onClick={() => void doSave()}>
          [save]
        </button>
        <button type="button" className="settings-btn settings-btn-mute" onClick={addTrigger}>
          [+ trigger]
        </button>
        {dirty && <UnsavedDot />}
        {savedAt !== null && <span className="settings-saved">saved.</span>}
      </div>
    </div>
  );
}

function indexInList(list: TriggerRecord[], slot: number, preset: boolean): number {
  let seen = -1;
  for (let i = 0; i < list.length; i++) {
    const isPreset = !!list[i].preset;
    if (isPreset === preset) {
      seen += 1;
      if (seen === slot) return i;
    }
  }
  return -1;
}

interface CardProps {
  trigger: TriggerRecord;
  onChange: (patch: Partial<TriggerRecord>) => void;
  onRemove: () => void;
  readOnly: boolean;
}

function TriggerCard({ trigger, onChange, onRemove, readOnly }: CardProps) {
  const { visual, effects } = splitActions(trigger.actions);
  const visualKind: VisualKind = visual?.kind ?? 'none';
  const patterns =
    trigger.patterns.length > 0 ? trigger.patterns : [{ pattern: '', enabled: true }];

  const setPatterns = (next: TriggerPattern[]) => {
    onChange({ patterns: next.length > 0 ? next : [{ pattern: '', enabled: true }] });
  };

  const updatePatternAt = (idx: number, patch: Partial<TriggerPattern>) => {
    const next = patterns.slice();
    next[idx] = { ...next[idx], ...patch };
    setPatterns(next);
  };

  const removePatternAt = (idx: number) => {
    if (patterns.length <= 1) return;
    const next = patterns.slice();
    next.splice(idx, 1);
    setPatterns(next);
  };

  const addPatternRow = () => {
    setPatterns([...patterns, { pattern: '', enabled: true }]);
  };

  const setVisual = (next: ReturnType<typeof splitActions>['visual']) => {
    onChange({ actions: joinActions(next, effects) });
  };

  const setEffectAt = (idx: number, next: TriggerAction) => {
    const nextEffects = effects.slice();
    nextEffects[idx] = next as (typeof nextEffects)[number];
    onChange({ actions: joinActions(visual, nextEffects) });
  };

  const removeEffectAt = (idx: number) => {
    const nextEffects = effects.slice();
    nextEffects.splice(idx, 1);
    onChange({ actions: joinActions(visual, nextEffects) });
  };

  const addEffect = (kind: EffectKind) => {
    const nextEffects = [...effects, blankEffect(kind) as (typeof effects)[number]];
    onChange({ actions: joinActions(visual, nextEffects) });
  };

  const radioGroup = `visual-${trigger.name}-${patterns[0]?.pattern ?? ''}`;

  return (
    <div className={`trigger-card${readOnly ? ' is-readonly' : ''}`}>
      <div className="trigger-card-head">
        <label className="trigger-card-enabled">
          <input
            type="checkbox"
            checked={trigger.enabled}
            disabled={readOnly}
            onChange={(e) => onChange({ enabled: e.target.checked })}
          />
          enabled
        </label>
        <input
          className="trigger-card-name"
          type="text"
          placeholder="name"
          value={trigger.name}
          disabled={readOnly}
          onChange={(e) => onChange({ name: e.target.value })}
        />
        <input
          className="trigger-card-priority"
          type="number"
          min={0}
          max={99}
          value={trigger.priority}
          disabled={readOnly}
          onChange={(e) => onChange({ priority: Number(e.target.value) || 0 })}
          title="priority"
        />
        {trigger.preset && (
          <span className="trigger-card-preset" title={`from preset: ${trigger.preset}`}>
            {trigger.preset}
          </span>
        )}
        {!readOnly && (
          <button type="button" className="trigger-card-remove" onClick={onRemove}>
            ×
          </button>
        )}
      </div>

      <div className="trigger-card-row">
        <span className="trigger-card-label">
          {patterns.length > 1 ? `patterns (${patterns.length})` : 'pattern'}
        </span>
        <div className="trigger-card-patterns">
          {patterns.map((p, i) => (
            <div key={i} className="trigger-card-pattern-row">
              <label
                className="trigger-card-pattern-toggle"
                title={p.enabled ? 'pattern row enabled' : 'pattern row disabled'}
              >
                <input
                  type="checkbox"
                  checked={p.enabled}
                  disabled={readOnly}
                  onChange={(e) => updatePatternAt(i, { enabled: e.target.checked })}
                />
              </label>
              <input
                type="text"
                className={`trigger-card-pattern-input${p.enabled ? '' : ' is-off'}`}
                value={p.pattern}
                disabled={readOnly}
                spellCheck={false}
                placeholder={i === 0 ? '^You feel a lot better!$' : 'another pattern…'}
                onChange={(e) => updatePatternAt(i, { pattern: e.target.value })}
              />
              {!readOnly && patterns.length > 1 && (
                <button
                  type="button"
                  className="trigger-card-pattern-remove"
                  onClick={() => removePatternAt(i)}
                  title="remove this pattern row"
                >
                  ×
                </button>
              )}
            </div>
          ))}
          {!readOnly && (
            <button
              type="button"
              className="trigger-card-pattern-add"
              onClick={addPatternRow}
              title="add another pattern (matches if any enabled row matches)"
            >
              [+ pattern]
            </button>
          )}
        </div>
      </div>

      <div className="trigger-card-row">
        <span className="trigger-card-label">visual</span>
        <div className="trigger-card-visual">
          <div className="trigger-card-visual-radios">
            {(['none', 'highlight', 'replace', 'gag'] as VisualKind[]).map((k) => (
              <label key={k} className="trigger-card-radio">
                <input
                  type="radio"
                  name={radioGroup}
                  checked={visualKind === k}
                  disabled={readOnly}
                  onChange={() => setVisual(blankVisual(k))}
                />
                {k}
              </label>
            ))}
          </div>
          {visual && (
            <VisualFields
              visual={visual}
              onChange={(next) => setVisual(next)}
              readOnly={readOnly}
            />
          )}
        </div>
      </div>

      <div className="trigger-card-row">
        <span className="trigger-card-label">effects</span>
        <div className="trigger-card-effects">
          {effects.length === 0 && <span className="trigger-card-help">no side effects</span>}
          {effects.map((eff, i) => (
            <div key={i} className="trigger-card-effect-row">
              <span className="trigger-card-effect-label">{eff.kind}</span>
              <input
                type="text"
                spellCheck={false}
                placeholder={eff.kind === 'send' ? 'get 1.;wield 1.' : 'chat'}
                value={eff.kind === 'send' ? eff.template : eff.pane}
                disabled={readOnly}
                onChange={(e) =>
                  setEffectAt(
                    i,
                    eff.kind === 'send'
                      ? { kind: 'send', template: e.target.value }
                      : { kind: 'route', pane: e.target.value },
                  )
                }
              />
              {!readOnly && (
                <button
                  type="button"
                  className="trigger-card-effect-remove"
                  onClick={() => removeEffectAt(i)}
                >
                  ×
                </button>
              )}
            </div>
          ))}
          {!readOnly && (
            <div className="trigger-card-effect-actions">
              <button
                type="button"
                className="trigger-card-effect-add"
                onClick={() => addEffect('send')}
              >
                [+ send]
              </button>
              <button
                type="button"
                className="trigger-card-effect-add"
                onClick={() => addEffect('route')}
              >
                [+ route]
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

interface VisualFieldsProps {
  visual: NonNullable<ReturnType<typeof splitActions>['visual']>;
  onChange: (next: ReturnType<typeof splitActions>['visual']) => void;
  readOnly: boolean;
}

function VisualFields({ visual, onChange, readOnly }: VisualFieldsProps) {
  if (visual.kind === 'gag') {
    return <span className="trigger-card-help">drop the matched line entirely</span>;
  }
  if (visual.kind === 'replace') {
    return (
      <div className="trigger-card-template">
        <input
          type="text"
          spellCheck={false}
          placeholder="{fg:244}$1{reset}{fg:210}$2{reset}"
          value={visual.template}
          disabled={readOnly}
          onChange={(e) => onChange({ kind: 'replace', template: e.target.value })}
        />
        <span className="trigger-card-hint">
          tokens: {'{red}'} {'{bold_red}'} {'{fg:244}'} {'{#ff3399}'} {'{reset}'}; $1 $2 … reference
          capture groups
        </span>
      </div>
    );
  }
  // highlight
  return (
    <HighlightStyleEditor
      style={visual.style}
      onChange={(style) => onChange({ kind: 'highlight', style })}
      readOnly={readOnly}
    />
  );
}

interface HSProps {
  style: HighlightStyle;
  onChange: (style: HighlightStyle) => void;
  readOnly: boolean;
}

function HighlightStyleEditor({ style, onChange, readOnly }: HSProps) {
  const setKey = <K extends keyof HighlightStyle>(key: K, value: HighlightStyle[K] | undefined) => {
    const next: HighlightStyle = { ...style };
    if (value === undefined || value === false) {
      delete next[key];
    } else {
      next[key] = value;
    }
    onChange(next);
  };
  return (
    <div className="trigger-card-style">
      <label>
        fg
        <select
          value={style.fg ?? ''}
          disabled={readOnly}
          onChange={(e) =>
            setKey('fg', e.target.value ? (e.target.value as NamedColor) : undefined)
          }
        >
          <option value="">—</option>
          {COLORS.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>
      </label>
      <label>
        bg
        <select
          value={style.bg ?? ''}
          disabled={readOnly}
          onChange={(e) =>
            setKey('bg', e.target.value ? (e.target.value as NamedColor) : undefined)
          }
        >
          <option value="">—</option>
          {COLORS.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>
      </label>
      <label>
        <input
          type="checkbox"
          checked={!!style.bold}
          disabled={readOnly}
          onChange={(e) => setKey('bold', e.target.checked ? true : undefined)}
        />
        bold
      </label>
      <label>
        <input
          type="checkbox"
          checked={!!style.underline}
          disabled={readOnly}
          onChange={(e) => setKey('underline', e.target.checked ? true : undefined)}
        />
        underline
      </label>
      <label>
        <input
          type="checkbox"
          checked={!!style.inverse}
          disabled={readOnly}
          onChange={(e) => setKey('inverse', e.target.checked ? true : undefined)}
        />
        inverse
      </label>
    </div>
  );
}
