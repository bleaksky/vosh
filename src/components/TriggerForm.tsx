import { useEffect, useMemo, useState } from 'react';
import type { HighlightStyle, NamedColor, TriggerRecord } from '../lib/session';

interface Props {
  load: () => Promise<string>;
  save: (json: string) => Promise<number>;
  onError: (e: string | null) => void;
}

type ActionKind = 'highlight' | 'gag' | 'replace' | 'send' | 'route';

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
    pattern: '',
    priority: 5,
    enabled: true,
    action: { kind: 'highlight', style: { fg: 'yellow' } },
  };
}

// Structured editor for triggers. Cards stacked vertically; each card
// is one trigger with field controls. Preset-installed triggers
// (those carrying a non-empty `preset` tag) are still listed but
// marked and edit-disabled, since they get re-applied from
// src/lib/presets.ts on every startup.
export function TriggerForm({ load, save, onError }: Props) {
  const [list, setList] = useState<TriggerRecord[] | null>(null);
  const [savedAt, setSavedAt] = useState<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    load()
      .then((json) => {
        if (cancelled) return;
        try {
          const parsed = JSON.parse(json);
          setList(Array.isArray(parsed) ? parsed : []);
        } catch {
          setList([]);
        }
      })
      .catch((e) => onError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [load, onError]);

  const userTriggers = useMemo(
    () => (list ?? []).filter((t) => !t.preset),
    [list],
  );
  const presetTriggers = useMemo(
    () => (list ?? []).filter((t) => !!t.preset),
    [list],
  );

  const update = (idx: number, patch: Partial<TriggerRecord>) => {
    if (!list) return;
    const real = idxInList(list, idx, false);
    if (real < 0) return;
    const next = list.slice();
    next[real] = { ...next[real], ...patch };
    setList(next);
  };

  const updateAction = (idx: number, action: TriggerRecord['action']) => {
    update(idx, { action });
  };

  const remove = (idx: number) => {
    if (!list) return;
    const real = idxInList(list, idx, false);
    if (real < 0) return;
    const next = list.slice();
    next.splice(real, 1);
    setList(next);
  };

  const addTrigger = () => {
    if (!list) return;
    setList([...list, blankTrigger()]);
  };

  const doSave = async () => {
    if (!list) return;
    try {
      await save(JSON.stringify(list, null, 2));
      setSavedAt(Date.now());
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
          form edits the live trigger store; save replaces it atomically
        </span>
      </div>

      <div className="trigger-form-list">
        {userTriggers.map((t, i) => (
          <TriggerCard
            key={`u-${i}`}
            trigger={t}
            onChange={(patch) => update(i, patch)}
            onActionChange={(action) => updateAction(i, action)}
            onRemove={() => remove(i)}
          />
        ))}
        {presetTriggers.length > 0 && (
          <div className="trigger-form-preset-group">
            <div className="trigger-form-preset-heading">presets (auto-installed, edit through code)</div>
            {presetTriggers.map((t, i) => (
              <TriggerCard
                key={`p-${i}`}
                trigger={t}
                onChange={() => undefined}
                onActionChange={() => undefined}
                onRemove={() => undefined}
                readOnly
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
        {savedAt !== null && <span className="settings-saved">saved.</span>}
      </div>
    </div>
  );
}

// Translate "the i-th user trigger" into the real index in the full
// list (which interleaves user + preset). preset == true is the
// preset slice; preset == false is the user slice.
function idxInList(list: TriggerRecord[], slot: number, preset: boolean): number {
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
  onActionChange: (action: TriggerRecord['action']) => void;
  onRemove: () => void;
  readOnly?: boolean;
}

function TriggerCard({ trigger, onChange, onActionChange, onRemove, readOnly }: CardProps) {
  const kind = trigger.action.kind;
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
        <span className="trigger-card-label">pattern</span>
        <input
          type="text"
          value={trigger.pattern}
          disabled={readOnly}
          spellCheck={false}
          placeholder="^You feel a lot better!$"
          onChange={(e) => onChange({ pattern: e.target.value })}
        />
      </div>
      <div className="trigger-card-row">
        <span className="trigger-card-label">action</span>
        <div className="trigger-card-action">
          <select
            value={kind}
            disabled={readOnly}
            onChange={(e) => onActionChange(blankAction(e.target.value as ActionKind))}
          >
            <option value="highlight">highlight</option>
            <option value="gag">gag</option>
            <option value="replace">replace</option>
            <option value="send">send</option>
            <option value="route">route</option>
          </select>
          <ActionFields
            action={trigger.action}
            onChange={onActionChange}
            readOnly={readOnly ?? false}
          />
        </div>
      </div>
    </div>
  );
}

function blankAction(kind: ActionKind): TriggerRecord['action'] {
  switch (kind) {
    case 'highlight':
      return { kind: 'highlight', style: { fg: 'yellow' } };
    case 'gag':
      return { kind: 'gag' };
    case 'replace':
      return { kind: 'replace', template: '' };
    case 'send':
      return { kind: 'send', template: '' };
    case 'route':
      return { kind: 'route', pane: 'chat' };
  }
}

interface ActionFieldsProps {
  action: TriggerRecord['action'];
  onChange: (action: TriggerRecord['action']) => void;
  readOnly: boolean;
}

function ActionFields({ action, onChange, readOnly }: ActionFieldsProps) {
  if (action.kind === 'highlight') {
    return (
      <HighlightStyleEditor
        style={action.style}
        onChange={(style) => onChange({ kind: 'highlight', style })}
        readOnly={readOnly ?? false}
      />
    );
  }
  if (action.kind === 'gag') {
    return <span className="trigger-card-help">drop the matched line entirely</span>;
  }
  if (action.kind === 'replace' || action.kind === 'send') {
    return (
      <input
        type="text"
        spellCheck={false}
        placeholder={
          action.kind === 'replace'
            ? '{fg:244}$1{reset}{fg:210}$2{reset}{fg:244}$3{reset}'
            : 'flee'
        }
        value={action.template}
        disabled={readOnly}
        onChange={(e) =>
          onChange({ ...action, template: e.target.value } as TriggerRecord['action'])
        }
      />
    );
  }
  // route
  return (
    <input
      type="text"
      spellCheck={false}
      placeholder="chat"
      value={action.pane}
      disabled={readOnly}
      onChange={(e) => onChange({ kind: 'route', pane: e.target.value })}
    />
  );
}

interface HSProps {
  style: HighlightStyle;
  onChange: (style: HighlightStyle) => void;
  readOnly: boolean;
}

function HighlightStyleEditor({ style, onChange, readOnly }: HSProps) {
  // exactOptionalPropertyTypes refuses `{ key: undefined }`; build the
  // next style by omitting falsy keys instead.
  const setKey = <K extends keyof HighlightStyle>(
    key: K,
    value: HighlightStyle[K] | undefined,
  ) => {
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
