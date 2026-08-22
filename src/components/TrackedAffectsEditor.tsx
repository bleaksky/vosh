import { useState } from 'react';
import { type TrackedAffect, type UiConfig } from '../lib/session';
import { Chevron, XIcon } from './Icons';

interface Props {
  config: UiConfig;
  update: (patch: Partial<UiConfig>) => void;
}

// Tracked-affect list editor. Each row is name + optional display
// label + up/down/remove. "Add" row at the bottom commits a new
// affect when the user types and presses Enter (or clicks add).
// Lives behind Settings · Panels · Panes · Affects in the Option B
// structure; previously sat under Settings · General.
export function TrackedAffectsEditor({ config, update }: Props) {
  const [draft, setDraft] = useState('');

  const norm = (s: string) => s.toLowerCase().replace(/\s+/g, ' ').trim();
  const addAffect = (raw: string) => {
    const name = raw.trim();
    if (name.length === 0) return;
    if (config.tracked_affects.some((t) => norm(t.name) === norm(name))) {
      setDraft('');
      return;
    }
    update({
      tracked_affects: [...config.tracked_affects, { name, label: null }],
    });
    setDraft('');
  };

  return (
    <div className="settings-tracked">
      <div className="settings-tracked-chips">
        {config.tracked_affects.length === 0 && (
          <span className="settings-font-empty">no affects tracked yet</span>
        )}
        {config.tracked_affects.map((entry, i) => {
          const moveTo = (target: number) => {
            if (target < 0 || target >= config.tracked_affects.length || target === i) return;
            const next = config.tracked_affects.slice();
            const [moved] = next.splice(i, 1);
            next.splice(target, 0, moved);
            update({ tracked_affects: next });
          };
          const updateEntry = (patch: Partial<TrackedAffect>) => {
            const next = config.tracked_affects.slice();
            next[i] = { ...next[i], ...patch };
            update({ tracked_affects: next });
          };
          const removeEntry = () => {
            update({
              tracked_affects: config.tracked_affects.filter((_, j) => j !== i),
            });
          };
          return (
            <span
              key={`${entry.name}-${i}`}
              className="settings-tracked-chip settings-tracked-chip-pair"
            >
              <button
                type="button"
                className="settings-tracked-move"
                aria-label={`move ${entry.name} up`}
                disabled={i === 0}
                onClick={() => moveTo(i - 1)}
                title="move up"
              >
                <Chevron open={false} up />
              </button>
              <button
                type="button"
                className="settings-tracked-move"
                aria-label={`move ${entry.name} down`}
                disabled={i === config.tracked_affects.length - 1}
                onClick={() => moveTo(i + 1)}
                title="move down"
              >
                <Chevron open />
              </button>
              <input
                type="text"
                className="settings-tracked-name"
                spellCheck={false}
                value={entry.name}
                placeholder="server name"
                title="affect name the server pushes (matched case-insensitively)"
                onChange={(e) => updateEntry({ name: e.target.value })}
              />
              <span className="settings-tracked-sep" aria-hidden="true">
                →
              </span>
              <input
                type="text"
                className="settings-tracked-label"
                spellCheck={false}
                value={entry.label ?? ''}
                placeholder="label (optional)"
                title="display label; leave empty to show the server name"
                onChange={(e) =>
                  updateEntry({
                    label: e.target.value.length > 0 ? e.target.value : null,
                  })
                }
              />
              <button
                type="button"
                className="settings-tracked-remove"
                aria-label={`remove ${entry.name}`}
                onClick={removeEntry}
              >
                <XIcon />
              </button>
            </span>
          );
        })}
      </div>
      <div className="settings-tracked-add">
        <input
          type="text"
          spellCheck={false}
          placeholder="affect name"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault();
              addAffect(draft);
            }
          }}
        />
        <button
          type="button"
          className="settings-btn"
          disabled={draft.trim().length === 0}
          onClick={() => addAffect(draft)}
        >
          add
        </button>
      </div>
    </div>
  );
}
