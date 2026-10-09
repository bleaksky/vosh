import type { RefObject } from 'react';
import { stopAskingToPost } from '../ipc/uiConfig';
import { ConfirmDialog } from '../ui/ConfirmDialog';
import { stopsAsking } from './askPost';
import type { Ask } from './cardDialogs';
import { DontAskAgain } from './DontAskAgain';

// The writing card's confirm. It sits over the card's foot, and under
// Post…'s confirm it offers Don't ask again.

export interface Confirm extends Ask {
  run: () => void;
  /** The confirm offers Don't ask again, which turns Ask before you
   *  post off. */
  skip?: boolean;
}

export function WritingConfirm({
  confirm,
  skipAsk,
  setSkipAsk,
  setConfirm,
  cardRef,
}: {
  confirm: Confirm;
  skipAsk: boolean;
  setSkipAsk: (on: boolean) => void;
  setConfirm: (confirm: Confirm | null) => void;
  cardRef: RefObject<HTMLDivElement | null>;
}) {
  // A confirm sits over the card's foot, 12 in from its right.
  const cardBox = cardRef.current?.getBoundingClientRect();
  const confirmAt = cardBox
    ? {
        right: window.innerWidth - cardBox.right + 12,
        bottom: window.innerHeight - cardBox.bottom + 60,
      }
    : null;
  return (
    <ConfirmDialog
      title={confirm.title}
      body={confirm.body}
      confirmLabel={confirm.label}
      {...(confirm.cancel ? { cancelLabel: confirm.cancel } : {})}
      tone={confirm.tone ?? 'danger'}
      onConfirm={() => {
        const go = confirm.run;
        if (stopsAsking(confirm.skip, skipAsk)) void stopAskingToPost().catch(() => {});
        setConfirm(null);
        go();
      }}
      onCancel={() => setConfirm(null)}
      {...(confirmAt ? { at: confirmAt } : {})}
    >
      {confirm.skip && <DontAskAgain checked={skipAsk} onChange={setSkipAsk} />}
    </ConfirmDialog>
  );
}
