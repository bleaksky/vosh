import { useEffect, useState, type MouseEvent } from 'react';
import { getUiConfig, subscribeWritingOfferChanged } from '../ipc/uiConfig';
import { useTauriEvent } from '../ipc/useTauriEvent';
import type { WritingKind } from '../ipc/writing';
import { useWriting } from '../stores/session/writingStore';
import { Button } from '../ui';

// The card's offer when you open the game's editor yourself on a text
// Vosh can name (Description Editor Q3, Note Editor Q3, board 3). It is
// the update notice's recipe at the terminal's lower right: the accent
// dot, Write this in Vosh?, Keep typing and Open in Vosh. It goes when you
// choose, when anything else goes out after your line, and when the
// game's prompt returns, since the writer takes the offer back then.
// Settings › Input › Offer the card when the game's editor opens turns
// it off.

// A press on the notice's buttons leaves the caret on the command line.
const keepCaret = (event: MouseEvent) => event.preventDefault();

export function WritingOffer({ onOpen }: { onOpen: (kind: WritingKind, offer: number) => void }) {
  const offer = useWriting().offer;
  const [on, setOn] = useState(true);
  useEffect(() => {
    let live = true;
    getUiConfig()
      .then((config) => live && setOn(config.writing_offer))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, []);
  useTauriEvent(subscribeWritingOfferChanged, (next) => setOn(Boolean(next)));
  // The offer you said Keep typing to.
  const [kept, setKept] = useState<number | null>(null);
  if (!on || !offer || offer.id === kept) return null;
  return (
    <div className="ov-update" role="status" aria-live="polite">
      <span className="ov-update-dot dot is-accent" aria-hidden="true" />
      <span className="ov-update-msg">Write this in Vosh?</span>
      <span className="ov-update-actions">
        <Button onMouseDown={keepCaret} onClick={() => setKept(offer.id)}>
          Keep typing
        </Button>
        <Button
          variant="primary"
          onMouseDown={keepCaret}
          onClick={() => onOpen(offer.kind, offer.id)}
        >
          Open in Vosh
        </Button>
      </span>
    </div>
  );
}
