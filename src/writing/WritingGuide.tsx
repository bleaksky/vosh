import { Button } from '../ui';
import type { Guide } from './kinds';

// The guide beside the text. The help's reminders in Vosh's words, as
// plain rows with no mark, since Vosh checks none of them. The width
// and the count live in the marks and the footer. Vosh's own words sit
// under them, and the button asks the game for the help itself.

export function WritingGuide({ guide, onHelp }: { guide: Guide; onHelp: () => void }) {
  return (
    <aside className="wr-guide" aria-label="Guide">
      <h3 className="wr-guide-h">{guide.head}</h3>
      <ul className="wr-guide-list">
        {guide.rows.map((row) =>
          typeof row === 'string' ? (
            <li key={row} className="wr-guide-row">
              {row}
            </li>
          ) : (
            <li key={row.head} className="wr-guide-row">
              <span className="wr-guide-head">{row.head}</span>
              <span className="wr-guide-sub">{row.sub}</span>
            </li>
          ),
        )}
      </ul>
      {guide.words.map((words) => (
        <p key={words} className="wr-guide-words">
          {words}
        </p>
      ))}
      <Button className="wr-small wr-guide-help" onClick={onHelp}>
        Read help {guide.help}
      </Button>
    </aside>
  );
}
