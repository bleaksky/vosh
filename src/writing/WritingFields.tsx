import type { KeyboardEvent } from 'react';

// To and Subject over a note's text, one block on the terminal ground.
// The label reads in the UI face and the value in the terminal face,
// since the value is what the game gets. A board that takes only
// immortal in To shows Immortal fixed, a note written in a tongue gains
// Language, and a bug or typo report shows the room you stand in, which
// the game records as you post. Tab and Return move from To to Subject
// to the text.

export type FieldName = 'to' | 'subject' | 'language';

interface Props {
  to: string;
  toFixed: boolean;
  subject: string;
  language: string | null;
  room: string | null;
  /** The field the game turned down. */
  bad: FieldName | null;
  readOnly: boolean;
  onTo: (to: string) => void;
  onSubject: (subject: string) => void;
  onLanguage: (language: string) => void;
  /** Return in the last field moves to the text. */
  onText: () => void;
}

export function WritingFields({
  to,
  toFixed,
  subject,
  language,
  room,
  bad,
  readOnly,
  onTo,
  onSubject,
  onLanguage,
  onText,
}: Props) {
  const next = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key !== 'Enter') return;
    e.preventDefault();
    const fields = Array.from(
      e.currentTarget.closest('.wr-fields')?.querySelectorAll<HTMLInputElement>('input') ?? [],
    );
    const at = fields.indexOf(e.currentTarget);
    if (at >= 0 && at < fields.length - 1) fields[at + 1].focus();
    else onText();
  };
  const field = (
    name: FieldName,
    label: string,
    value: string,
    set: (v: string) => void,
    placeholder?: string,
  ) => (
    <div className={`wr-f${bad === name ? ' is-bad' : ''}`}>
      <label className="wr-f-l" htmlFor={`wr-f-${name}`}>
        {label}
      </label>
      <input
        id={`wr-f-${name}`}
        className="wr-f-v"
        value={value}
        readOnly={readOnly}
        placeholder={placeholder}
        spellCheck={false}
        autoCapitalize="off"
        autoCorrect="off"
        autoComplete="off"
        onChange={(e) => set(e.target.value)}
        onKeyDown={next}
      />
    </div>
  );
  return (
    <div className={`wr-fields${language !== null ? ' has-three' : ''}`}>
      {toFixed ? (
        <div className="wr-f">
          <span className="wr-f-l">To</span>
          <span className="wr-f-v is-fixed">Immortal</span>
        </div>
      ) : (
        field('to', 'To', to, onTo, 'Add who it’s to')
      )}
      {field('subject', 'Subject', subject, onSubject, 'Add a subject')}
      {language !== null && field('language', 'Language', language, onLanguage, 'Add a tongue')}
      {room !== null && (
        <div className="wr-f">
          <span className="wr-f-l">Room</span>
          <span className="wr-f-v is-fixed">{room}</span>
          <span className="wr-f-end">Where you are</span>
        </div>
      )}
    </div>
  );
}
