import { useRef, useState } from 'react';
import type { UiConfig } from '../../../../lib/session';
import type { UpdateConfig } from '../../legacy/useSettingsAutoSave';
import { Field, Row } from '../../ui';

interface FontStackRowProps {
  config: UiConfig;
  update: UpdateConfig;
}

/** The terminal font as a CSS font list you type, for a font the Font
 *  menu does not list or a fallback after it. It saves when you leave
 *  the field or press Enter, so the terminal does not jump between
 *  fonts while you type. Esc puts the saved list back. */
export function FontStackRow({ config, update }: FontStackRowProps) {
  // The text while you edit, or null when it shows the saved list.
  const [draft, setDraft] = useState<string | null>(null);
  // Esc leaves the field without saving what you typed.
  const cancelRef = useRef(false);
  const commit = () => {
    const cancelled = cancelRef.current;
    cancelRef.current = false;
    if (draft === null || cancelled) {
      setDraft(null);
      return;
    }
    const next = draft.trim();
    setDraft(null);
    if (next !== '' && next !== config.font_family) update({ font_family: next }, { now: true });
  };
  return (
    <Row
      anchor="font-stack"
      label="Font stack"
      description="Vosh uses the first font in this list that you have."
    >
      <Field
        value={draft ?? config.font_family}
        placeholder='"BerkeleyMono Bundled", Menlo, monospace'
        onChange={setDraft}
        onFocus={() => setDraft(config.font_family)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === 'Enter') e.currentTarget.blur();
          if (e.key === 'Escape') {
            cancelRef.current = true;
            e.currentTarget.blur();
          }
        }}
      />
    </Row>
  );
}
