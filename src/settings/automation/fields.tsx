import { forwardRef, useId, useState, type ReactNode } from 'react';
import { CodeEditor } from '../../ui/CodeEditor';
import { canonicalKeyFromEvent, labelForKey } from '../../automation/macroKeys';
import { CheckIcon, ChipButton, Field, PencilIcon, useRowIds, type FieldProps } from '../../ui';

// Controls the Automation detail cards share. Each one wraps a
// primitive from ui/ with the editing rule its field needs.

type PassThrough = Omit<FieldProps, 'value' | 'onChange' | 'onBlur' | 'onKeyDown'>;

/** The Group field. Typing leaves the item where it is. Enter or
 *  leaving the field files it under the group you typed, and Esc puts
 *  the old group back. Blank means no group. */
export function GroupField({
  value,
  onCommit,
  ...rest
}: PassThrough & { value: string; onCommit: (group: string) => void }) {
  const [draft, setDraft] = useState<string | null>(null);
  const commit = () => {
    if (draft === null) return;
    const next = draft.trim();
    setDraft(null);
    if (next !== value) onCommit(next);
  };
  return (
    <Field
      {...rest}
      value={draft ?? value}
      onChange={setDraft}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          commit();
        } else if (e.key === 'Escape' && draft !== null) {
          e.preventDefault();
          e.stopPropagation();
          setDraft(null);
        }
      }}
    />
  );
}

/** A whole number field. It takes digits only, clamps to `max` as you
 *  type, and to `min` when you leave it. A `unit` shows beside the field
 *  and describes it, so a screen reader says `30, seconds` and not a
 *  bare number. */
export function NumberField({
  value,
  onChange,
  min = 0,
  max = Number.MAX_SAFE_INTEGER,
  width = 64,
  unit,
  ...rest
}: PassThrough & {
  value: number;
  onChange: (value: number) => void;
  min?: number;
  max?: number;
  /** The unit after the field, like `seconds`. */
  unit?: string;
}) {
  const [text, setText] = useState<string | null>(null);
  const row = useRowIds();
  const unitId = useId();
  const describedBy =
    [rest['aria-describedby'] ?? row?.descriptionId, unit !== undefined ? unitId : undefined]
      .filter(Boolean)
      .join(' ') || undefined;
  return (
    <>
      <Field
        {...rest}
        aria-describedby={describedBy}
        inputMode="numeric"
        width={width}
        value={text ?? String(value)}
        onChange={(next) => {
          const digits = next.replace(/[^0-9]/g, '');
          setText(digits);
          if (!digits) return;
          const n = Math.min(max, Math.max(min, parseInt(digits, 10)));
          if (n !== value) onChange(n);
        }}
        onBlur={() => setText(null)}
      />
      {unit !== undefined && (
        <span id={unitId} className="st-auto-unit">
          {unit}
        </span>
      )}
    </>
  );
}

/** The macro Key field. Focus it and press a key or a combination to
 *  bind. Tab still moves on and Esc stops listening. */
export const KeyCaptureField = forwardRef<
  HTMLInputElement,
  PassThrough & { value: string; onChange: (key: string) => void }
>(function KeyCaptureField({ value, onChange, ...rest }, ref) {
  const [listening, setListening] = useState(false);
  return (
    <Field
      {...rest}
      ref={ref}
      mono
      readOnly
      value={listening ? '' : labelForKey(value)}
      onChange={() => {}}
      placeholder="Press a key"
      onFocus={() => setListening(true)}
      onBlur={() => setListening(false)}
      onKeyDown={(e) => {
        const plain = !e.ctrlKey && !e.altKey && !e.metaKey;
        if (e.key === 'Tab' && plain) return;
        if (e.key === 'Escape' && plain) {
          e.preventDefault();
          e.currentTarget.blur();
          return;
        }
        const key = canonicalKeyFromEvent(e, { allowPlainPrintable: true });
        if (!key) return;
        e.preventDefault();
        onChange(key);
        e.currentTarget.blur();
      }}
    />
  );
});

/** A card row that holds a Lua editor under its label. */
export function CodeRow({
  label,
  description,
  value,
  onChange,
  readOnly = false,
  placeholder,
}: {
  label: string;
  description?: ReactNode;
  value: string;
  onChange: (value: string) => void;
  readOnly?: boolean;
  placeholder?: string;
}) {
  const labelId = useId();
  const descId = useId();
  const described = description !== undefined;
  return (
    <div className="st-row st-auto-coderow">
      <div className="st-row-text">
        <span id={labelId} className="st-row-label">
          {label}
        </span>
        {described && (
          <span id={descId} className="st-row-desc">
            {description}
          </span>
        )}
      </div>
      <CodeEditor
        className="st-code"
        ariaLabelledBy={labelId}
        {...(described ? { ariaDescribedBy: descId } : {})}
        language="lua"
        inline
        minHeight="84px"
        maxHeight="240px"
        value={value}
        onChange={onChange}
        readOnly={readOnly}
        {...(placeholder !== undefined ? { placeholder } : {})}
      />
    </div>
  );
}

/** A flagged row's line, what the preset now has in the warn tone, and
 *  its two choices on the chip recipe (Presets board 4), for a trigger
 *  row or a swatch. Take the fix sets the row to the preset's value, and
 *  Keep mine keeps yours. Both wait for Save. */
export function FixChoice({
  verb,
  children,
  onTake,
  onKeep,
}: {
  verb: string;
  children: ReactNode;
  onTake: () => void;
  onKeep: () => void;
}) {
  return (
    <span className="st-auto-fix">
      <span className="st-auto-fix-line">
        The preset now {verb} {children}
      </span>
      <span className="st-auto-fix-choices">
        <ChipButton icon={<CheckIcon size={12} />} onClick={onTake}>
          Take the fix
        </ChipButton>
        <ChipButton icon={<PencilIcon size={12} />} onClick={onKeep}>
          Keep mine
        </ChipButton>
      </span>
    </span>
  );
}
