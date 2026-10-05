import { useId } from 'react';
import type { KindNoun } from '../../../../automation/automationDraft';
import { CodeEditor } from '../../../CodeEditor';
import { Button } from '../../ui';

interface JsonPanelProps {
  noun: KindNoun;
  text: string;
  /** The text does not read as a list yet. */
  bad: boolean;
  onChange: (text: string) => void;
  onDone: () => void;
}

/** Edit all as JSON: the kind's whole draft as one JSON list. Each
 *  edit that reads updates the draft, so the save bar below counts it,
 *  Save writes it, and Discard puts the last save back. */
export function JsonPanel({ noun, text, bad, onChange, onDone }: JsonPanelProps) {
  const headingId = useId();
  const noteId = useId();
  return (
    <section className="st-auto-json" data-st-anchor="json" aria-labelledby={headingId}>
      <div className="st-section-head">
        <h2 id={headingId} className="st-section-title">
          Edit all {noun.many} as JSON
        </h2>
        <div className="st-section-actions">
          <Button onClick={onDone}>Done</Button>
        </div>
      </div>
      <CodeEditor
        className="st-code st-auto-json-editor"
        ariaLabelledBy={headingId}
        ariaDescribedBy={noteId}
        fill
        value={text}
        onChange={onChange}
      />
      <p id={noteId} className={bad ? 'st-auto-json-note is-bad' : 'st-auto-json-note'}>
        {bad
          ? 'Vosh cannot read this JSON. Fix it to save.'
          : `Save applies what you change here to your ${noun.many}.`}
      </p>
    </section>
  );
}
