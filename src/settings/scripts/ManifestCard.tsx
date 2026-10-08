import { useId } from 'react';
import { pluginReveal, type PluginManifest } from '../../ipc/scripts';
import { errorText } from '../../lib/text';
import { Button, Card, CardNote, Field, FieldArea, Row, Select } from '../../ui';
import { revealLabel } from '../../lib/revealLabel';

// The Manifest tab of a plugin's page. The fields Vosh keeps in
// manifest.toml, edited in the page's draft. The name is the folder's
// and reads only. Version, Author and Runs first take the Automation
// card's 240 px field column, and Description stacks under its label.
// Folder shows where the plugin lives and opens it in the system's file
// manager.

/** The card's first line. */
export const MANIFEST_NOTE =
  'Vosh keeps these in manifest.toml in the plugin folder. Anyone you share the plugin with sees them before they install it.';

interface Props {
  name: string;
  /** The manifest as the draft holds it. */
  manifest: PluginManifest;
  /** Every Lua file in the folder, which Runs first offers. */
  files: readonly string[];
  /** The folder by its path under the app data, like
   *  `plugins/vitals_alert`. */
  folder: string;
  onChange: (patch: Partial<PluginManifest>) => void;
  /** Runs first picked another file. */
  onEntry: (entry: string) => void;
  onError: (message: string | null) => void;
}

export function ManifestCard({ name, manifest, files, folder, onChange, onEntry, onError }: Props) {
  const descriptionId = useId();
  const platform =
    typeof document === 'undefined' ? undefined : document.documentElement.dataset.platform;
  const reveal = () => {
    pluginReveal(name)
      .then(() => onError(null))
      .catch((e: unknown) => onError(errorText(e)));
  };
  return (
    <Card className="st-auto-card">
      <CardNote>{MANIFEST_NOTE}</CardNote>
      <Row label="Name" description="The folder has the same name.">
        <span className="st-auto-value st-auto-mono">{name}</span>
      </Row>
      <Row label="Version">
        <Field
          width="100%"
          value={manifest.version}
          onChange={(version) => onChange({ version })}
        />
      </Row>
      <Row label="Author">
        <Field width="100%" value={manifest.author} onChange={(author) => onChange({ author })} />
      </Row>
      <div className="st-row st-auto-block">
        <label htmlFor={descriptionId} className="st-row-label">
          Description
        </label>
        <FieldArea
          id={descriptionId}
          width="100%"
          value={manifest.description}
          onChange={(description) => onChange({ description })}
        />
      </div>
      <Row label="Runs first" description="The file Vosh loads when the plugin turns on.">
        <Select
          width="100%"
          value={manifest.entry}
          options={files.map((file) => ({ value: file, label: file }))}
          onChange={onEntry}
        />
      </Row>
      <Row label="Folder" description={<span className="st-plugin-folder">{folder}</span>}>
        <Button onClick={reveal}>{revealLabel(platform)}</Button>
      </Row>
    </Card>
  );
}
