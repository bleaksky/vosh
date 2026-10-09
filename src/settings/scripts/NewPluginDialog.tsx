import { useId, useState } from 'react';
import { pluginCreate, type PluginRow } from '../../ipc/scripts';
import { errorText } from '../../lib/text';
import { Field } from '../../ui';
import { ConfirmDialog } from '../../ui/ConfirmDialog';
import { pluginNameOk, takenPluginName } from './pluginName';

// New plugin: the confirm recipe in the primary tone with one
// Name field. Create stays off until the name keeps the rule and is not
// one of your plugins, then makes the folder and hands the page the
// list with it, which opens the new plugin's page.

/** The hint under the field, which says the rule. */
const NAME_HINT = 'Letters, digits, and underscores.';

interface Props {
  /** Your plugins, whose names are taken. */
  plugins: readonly PluginRow[];
  /** The plugin `name` is made, and `list` is the list with it. */
  onCreated: (list: PluginRow[], name: string) => void;
  onCancel: () => void;
  onError: (message: string | null) => void;
}

export function NewPluginDialog({ plugins, onCreated, onCancel, onError }: Props) {
  const fieldId = useId();
  const hintId = useId();
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);
  const taken = takenPluginName(plugins, name);
  const ready = pluginNameOk(name) && taken === null && !busy;

  const create = () => {
    if (!ready) return;
    setBusy(true);
    pluginCreate(name)
      .then((list) => {
        onError(null);
        onCreated(list, name);
      })
      .catch((e: unknown) => {
        onError(errorText(e));
        setBusy(false);
      });
  };

  return (
    <ConfirmDialog
      title="New plugin"
      body="Vosh makes a folder for it in your plugins folder and opens it here."
      confirmLabel="Create"
      tone="primary"
      confirmDisabled={!ready}
      onConfirm={create}
      onCancel={onCancel}
    >
      <label className="ov-field-label" htmlFor={fieldId}>
        Name
      </label>
      <Field
        id={fieldId}
        mono
        width="100%"
        value={name}
        onChange={setName}
        aria-describedby={hintId}
        onKeyDown={(e) => {
          if (e.key !== 'Enter') return;
          e.preventDefault();
          create();
        }}
      />
      <p id={hintId} className="ov-hint">
        {taken === null ? NAME_HINT : `You already have a plugin named ${taken}.`}
      </p>
    </ConfirmDialog>
  );
}
