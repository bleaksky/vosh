import { Button } from '../ui';

// The writing card with no character to write for. It asks you to log
// in first.

export function WritingNoCharacter({ onClose }: { onClose: () => void }) {
  return (
    <div
      className="pc-card st-controls wr-card"
      role="dialog"
      aria-label="Write"
      style={{ left: 12, bottom: 120 }}
    >
      <div className="pc-head">
        <h2 className="pc-title">Write</h2>
        <span className="pc-spacer" />
        <Button onClick={onClose}>Close</Button>
      </div>
      <div className="pc-rule" />
      <div className="pc-body">
        <p className="pc-copy">
          Log in with a character first. Vosh keeps your writing separate for each one.
        </p>
      </div>
    </div>
  );
}
