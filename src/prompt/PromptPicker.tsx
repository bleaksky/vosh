import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent } from 'react';
import type { BandEnv } from '../terminal/bandCells';
import {
  fieldName,
  flatRows,
  needsCode,
  paramPrompt,
  pickerGroups,
  rowKey,
  sourceLine,
  type LayoutId,
  type PickerRow,
} from './pickerRows';
import { type PromptState } from '../ipc/prompt';
import {
  promptForms,
  type PromptForm,
  type PromptFormatChoice,
  type PromptPreviewName,
} from '../ipc/promptDesign';
import { scrollWithin } from '../lib/scrollWithin';
import { parseSgrCells } from '../terminal/sgrCells';
import { cx, Field, SearchIcon } from '../ui';
import { CellLine } from './PromptCells';

// Insert value…: every field Vosh can draw, grouped by topic, inside
// the card so the panel and your prompt stay in view. The left list
// shows what each field reads now, or why it reads nothing. The right
// pane says where the highlighted field comes from and draws each of
// its forms with the values the card shows. A click on a form, or
// Enter, adds it at the caret. Right moves into the forms and Left
// back.

const LAYOUT_HELP: Record<LayoutId, { help: string; sample: string }> = {
  nl: { help: 'Starts a new line.', sample: '↵' },
  nl_fight: { help: 'Starts a new line only in a fight.', sample: '↵' },
  space: { help: 'A space between two parts.', sample: '·' },
  right: { help: 'Pushes what follows on its line to the right edge.', sample: '→' },
};

interface PromptPickerProps {
  /** The session whose prompt the card works on. */
  session: number;
  state: PromptState;
  preview: PromptPreviewName;
  env: BandEnv;
  cellW: number;
  refresh: number;
  onInsert: (field: string, format: PromptFormatChoice) => void;
  onInsertLayout: (id: LayoutId) => void;
  /** You opened it from the keyboard, so the search takes focus and you
   *  can type at once. Opened with a click it rests. */
  focusSearch?: boolean;
}

export function PromptPicker({
  session,
  state,
  preview,
  env,
  cellW,
  refresh,
  onInsert,
  onInsertLayout,
  focusSearch = false,
}: PromptPickerProps) {
  const [query, setQuery] = useState('');
  const groups = useMemo(
    () => pickerGroups(state.catalog, state.packages, query),
    [state.catalog, state.packages, query],
  );
  const rows = useMemo(() => flatRows(groups), [groups]);
  const [current, setCurrent] = useState<string | null>(null);
  const row = rows.find((r) => rowKey(r) === current) ?? rows[0] ?? null;
  const [pane, setPane] = useState<'list' | 'forms'>('list');
  const [formAt, setFormAt] = useState(0);
  const [param, setParam] = useState('');
  const [forms, setForms] = useState<PromptForm[]>([]);
  const listRef = useRef<HTMLDivElement | null>(null);
  const searchRef = useRef<HTMLInputElement | null>(null);
  // Focus stays in the search while the arrows move the highlight, so
  // the search names the highlighted value or form as the one a reader
  // is on (the combobox pattern).
  const ids = useId();
  const valuesId = `${ids}-values`;
  const formsId = `${ids}-forms`;
  const optionId = (key: string) => `${ids}-v-${key.replace(/[^A-Za-z0-9_-]/g, '_')}`;
  const formId = (index: number) => `${ids}-f-${index}`;

  const focusFirst = useRef(focusSearch);
  useEffect(() => {
    if (focusFirst.current) searchRef.current?.focus({ preventScroll: true });
  }, []);

  // The highlighted field's forms, drawn with the values the card shows.
  const field = row?.kind === 'field' ? row.field : null;
  const name = field ? fieldName(field, param) : null;
  useEffect(() => {
    setFormAt(0);
    if (!name) {
      setForms([]);
      return;
    }
    let alive = true;
    void promptForms(name, preview === 'now' ? null : preview, session)
      .then((next) => {
        if (alive) setForms(next);
      })
      .catch(() => {
        if (alive) setForms([]);
      });
    return () => {
      alive = false;
    };
  }, [name, preview, refresh, session]);

  // A new field asks for its own name.
  useEffect(() => setParam(''), [field?.name]);

  // The highlighted row stays in view.
  useEffect(() => {
    const key = row ? rowKey(row) : null;
    if (!key) return;
    scrollWithin(listRef.current?.querySelector<HTMLElement>(`[data-key="${CSS.escape(key)}"]`), {
      block: 'nearest',
    });
  }, [row]);

  const usable = field ? !needsCode(field) : true;
  const insertForm = (index: number) => {
    const form = forms[index];
    if (!form || !name || !usable) return;
    onInsert(name, { format: form.format });
  };
  const choose = (target: PickerRow | null) => {
    if (!target) return;
    if (target.kind === 'layout') onInsertLayout(target.id);
    else insertForm(formAt);
  };

  const onKeyDown = (e: KeyboardEvent) => {
    const at = row ? rows.indexOf(row) : -1;
    const count = pane === 'list' ? rows.length : row?.kind === 'layout' ? 1 : forms.length;
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const dir = e.key === 'ArrowDown' ? 1 : -1;
      if (pane === 'list') {
        const next = rows[Math.max(0, Math.min(rows.length - 1, at + dir))];
        if (next) setCurrent(rowKey(next));
      } else {
        setFormAt((i) => Math.max(0, Math.min(count - 1, i + dir)));
      }
    } else if (e.key === 'ArrowRight' && pane === 'list' && row) {
      if (e.target instanceof HTMLInputElement && e.target.selectionStart !== e.target.value.length)
        return;
      e.preventDefault();
      setPane('forms');
    } else if (e.key === 'ArrowLeft' && pane === 'forms') {
      e.preventDefault();
      setPane('list');
    } else if (e.key === 'Enter') {
      e.preventDefault();
      choose(row);
    }
  };

  const ask = field?.param ? paramPrompt(field) : null;
  const activeId =
    pane === 'forms'
      ? row?.kind === 'layout'
        ? formId(0)
        : forms.length > 0
          ? formId(formAt)
          : undefined
      : row
        ? optionId(rowKey(row))
        : undefined;
  return (
    <div className="pc-body pc-picker" onKeyDown={onKeyDown}>
      <div className="pc-picker-list">
        <Field
          ref={searchRef}
          type="search"
          width={248}
          icon={<SearchIcon />}
          placeholder="Search values"
          aria-label="Search values"
          role="combobox"
          aria-expanded="true"
          aria-autocomplete="list"
          aria-controls={pane === 'forms' ? formsId : valuesId}
          {...(activeId ? { 'aria-activedescendant': activeId } : {})}
          value={query}
          onChange={(text) => {
            setQuery(text);
            setCurrent(null);
            setPane('list');
          }}
        />
        <div
          ref={listRef}
          id={valuesId}
          className="pc-picker-rows"
          role="listbox"
          aria-label="Values"
        >
          <ul>
            {groups.map((group, g) => (
              <li key={group.id} role="presentation">
                {g > 0 && <div role="separator" className="pc-picker-sep" />}
                <div role="presentation" className="pc-picker-group">
                  {group.label}
                </div>
                <ul role="presentation">
                  {group.rows.map((r) => {
                    const key = rowKey(r);
                    const on = row !== null && rowKey(row) === key;
                    return (
                      <li key={key} role="none">
                        <button
                          type="button"
                          role="option"
                          id={optionId(key)}
                          aria-selected={on}
                          data-key={key}
                          tabIndex={-1}
                          className={cx(
                            'pc-picker-row',
                            on && pane === 'list' && 'is-on',
                            on && 'is-current',
                            r.dim && 'is-dim',
                          )}
                          onClick={() => {
                            setCurrent(key);
                            setPane('list');
                          }}
                          onDoubleClick={() => choose(r)}
                        >
                          <span className="pc-picker-name">
                            {r.kind === 'field' ? r.field.label : r.label}
                          </span>
                          <span className="pc-picker-value">{r.status}</span>
                        </button>
                      </li>
                    );
                  })}
                </ul>
              </li>
            ))}
          </ul>
          {rows.length === 0 && <p className="pc-picker-none">No value matches.</p>}
        </div>
      </div>
      <div className="pc-picker-forms">
        {row?.kind === 'layout' && (
          <>
            <h3 className="pc-picker-title">{row.label}</h3>
            <p className="pc-picker-source">{LAYOUT_HELP[row.id].help}</p>
            <ul id={formsId} role="listbox" aria-label="Formats" className="pc-forms">
              <li role="none">
                <button
                  type="button"
                  role="option"
                  id={formId(0)}
                  aria-selected
                  className={cx('pc-form', 'is-on')}
                  onClick={() => onInsertLayout(row.id)}
                >
                  <span>{row.label}</span>
                  <span
                    className="pc-form-sample is-glyph"
                    aria-hidden="true"
                    style={{ background: env.bg }}
                  >
                    {LAYOUT_HELP[row.id].sample}
                  </span>
                </button>
              </li>
            </ul>
          </>
        )}
        {field && (
          <>
            <h3 className="pc-picker-title">{field.label}</h3>
            <p className="pc-picker-source">{sourceLine(field)}</p>
            {ask && (
              <div className="pc-picker-param">
                <Field
                  width={264}
                  mono={field.name === 'gmcp'}
                  placeholder={ask.placeholder}
                  aria-label={ask.label}
                  value={param}
                  onChange={setParam}
                />
                {field.name === 'gmcp' && state.packages.length > 0 && (
                  <span className="pc-picker-packages">
                    {state.packages.map((p) => (
                      <button
                        key={p}
                        type="button"
                        className="pc-chip"
                        onClick={() => setParam(`${p}.`)}
                      >
                        {p}
                      </button>
                    ))}
                  </span>
                )}
              </div>
            )}
            <ul
              id={formsId}
              role="listbox"
              aria-label="Formats"
              className={cx('pc-forms', !usable && 'is-dim')}
            >
              {forms.map((form, i) => {
                const on = i === formAt;
                const cells = parseSgrCells(form.sample.ansi)[0] ?? [];
                return (
                  <li key={form.format} role="none">
                    <button
                      type="button"
                      role="option"
                      id={formId(i)}
                      aria-selected={on}
                      disabled={!usable}
                      className={cx('pc-form', on && 'is-on')}
                      onMouseEnter={() => setFormAt(i)}
                      onClick={() => insertForm(i)}
                    >
                      <span>{form.label}</span>
                      <span
                        className="pc-form-sample"
                        aria-hidden="true"
                        style={{ background: env.bg }}
                      >
                        {cells.length > 0 ? (
                          <CellLine cells={cells} env={env} cellW={cellW} limit={22} />
                        ) : (
                          <span className="pc-form-empty">{form.segment}</span>
                        )}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          </>
        )}
      </div>
    </div>
  );
}
