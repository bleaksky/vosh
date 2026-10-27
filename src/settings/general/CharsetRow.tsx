import { useEffect, useState } from 'react';
import { charsetGet, charsetSet, type Charset } from '../../ipc/session';
import { Row, Select } from '../../ui';
import { useShown } from '../shownProfile';

const CHOICES = [
  { value: 'utf8', label: 'UTF-8' },
  { value: 'latin1', label: 'Latin 1' },
];

/** How the profile Settings shows reads the game's text and writes
 *  yours. It reads again when Settings moves to another profile, and a
 *  pick saves at once, back where it was when the save fails. */
export function CharsetRow({ onError }: { onError: (message: string | null) => void }) {
  const profile = useShown().profile ?? undefined;
  const [charset, setCharset] = useState<Charset | null>(null);

  useEffect(() => {
    let cancelled = false;
    setCharset(null);
    charsetGet(profile)
      .then((next) => {
        if (!cancelled) setCharset(next);
      })
      .catch((e) => {
        if (!cancelled) onError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [profile, onError]);

  const pick = (value: string) => {
    const next: Charset = value === 'latin1' ? 'latin1' : 'utf8';
    const before = charset;
    setCharset(next);
    charsetSet(next, profile).then(
      () => onError(null),
      (e) => {
        setCharset((now) => (now === next ? before : now));
        onError(String(e));
      },
    );
  };

  return (
    <Row
      label="Text encoding"
      description="Most worlds use UTF-8, and Vosh still shows an older world's accented letters. Pick Latin 1 when the game garbles the accented letters you send."
      anchor="charset"
    >
      <Select
        value={charset ?? 'utf8'}
        onChange={pick}
        options={CHOICES}
        disabled={charset === null}
      />
    </Row>
  );
}
