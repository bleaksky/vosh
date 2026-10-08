import { useState } from 'react';
import { profileDisplayName } from '../../lib/characterProfiles';
import { Toggle } from '../../ui';
import { ConfirmDialog } from '../../ui/ConfirmDialog';

// Export to Downloads for a profile that has characters: the confirm
// recipe in the primary tone with a switch for each character, all off,
// so a profile you share names your characters only when you choose.

interface Props {
  /** The profile, raw like `default`. */
  name: string;
  /** The characters it has on its world. */
  characters: readonly string[];
  /** Export, naming the characters you turned on. */
  onExport: (characters: string[]) => void;
  onCancel: () => void;
}

export function ExportDialog({ name, characters, onExport, onCancel }: Props) {
  const [named, setNamed] = useState<readonly string[]>([]);
  const display = profileDisplayName(name);
  return (
    <ConfirmDialog
      title={`Export ${display}?`}
      body={`Vosh saves ${display} in your Downloads folder. The file names only the characters you turn on here.`}
      confirmLabel="Export"
      tone="primary"
      onConfirm={() => onExport(characters.filter((c) => named.includes(c)))}
      onCancel={onCancel}
    >
      {characters.map((character) => (
        <label key={character} className="ov-switch-row">
          {character}
          <Toggle
            checked={named.includes(character)}
            onChange={(on) =>
              setNamed((prev) => (on ? [...prev, character] : prev.filter((c) => c !== character)))
            }
          />
        </label>
      ))}
    </ConfirmDialog>
  );
}
