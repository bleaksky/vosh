import { emit } from '@tauri-apps/api/event';
import { useState } from 'react';
import {
  copyTheme,
  editCustomTheme,
  removeCustomTheme,
  THEME_SLOT_GROUPS,
} from '../../../../lib/appearanceSettings';
import type { CustomTheme, UiConfig } from '../../../../lib/session';
import {
  activeThemeFor,
  applyTheme,
  applyThemePrefs,
  pickTheme,
  themePrefsOf,
} from '../../../../lib/theme';
import {
  BUILTIN_THEMES,
  customToAppTheme,
  findTheme,
  migrateCustomChrome,
  setCustomThemes,
  themeTokens,
} from '../../../../lib/themes';
import { ConfirmDialog } from '../../../ConfirmDialog';
import type { UpdateConfig } from '../../legacy/useSettingsAutoSave';
import { Button, Field, PlusIcon, Row, Select } from '../../ui';
import { ColorBlock, ColorGroup } from './ColorGrid';

interface CustomThemeRowsProps {
  config: UiConfig;
  update: UpdateConfig;
}

/** Custom themes under Advanced: make one from the theme you see, then
 *  name it and change any color. Edits to the theme on screen show at
 *  once in every window. */
export function CustomThemeRows({ config, update }: CustomThemeRowsProps) {
  const [editId, setEditId] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const customs = config.custom_themes;
  const shown = activeThemeFor(config);
  // The theme the editor holds: the one you chose, else the custom
  // theme on screen, else the first.
  const editing =
    customs.find((t) => t.id === editId) ??
    customs.find((t) => t.id === shown) ??
    customs[0] ??
    null;

  const create = () => {
    const taken = [...BUILTIN_THEMES.map((t) => t.id), ...customs.map((t) => t.id)];
    const theme = copyTheme(findTheme(shown), taken);
    const list = [...customs, theme];
    setCustomThemes(list.map(customToAppTheme));
    const next = pickTheme({ ...config, custom_themes: list }, theme.id);
    applyThemePrefs(next);
    update({ custom_themes: list, ...themePrefsOf(next) }, { now: true });
    setEditId(theme.id);
  };

  const edit = (id: string, patch: Partial<CustomTheme>) => {
    const list = editCustomTheme(customs, id, patch);
    setCustomThemes(list.map(customToAppTheme));
    if (id === shown && (patch.xterm !== undefined || patch.chrome !== undefined)) {
      // The save sends the new catalog but not the theme id, which did
      // not change. Send both now, catalog first, so the main window
      // repaints with the new colors while you drag the picker. A name
      // or description edit waits for the save.
      applyTheme(id);
      void emit('vosh://custom-themes-changed', list)
        .then(() => emit('vosh://theme-changed', id))
        .catch(() => {});
    }
    update({ custom_themes: list });
  };

  const remove = (id: string) => {
    const next = removeCustomTheme(config, id);
    setCustomThemes(next.custom_themes.map(customToAppTheme));
    applyThemePrefs(next);
    update({ custom_themes: next.custom_themes, ...themePrefsOf(next) }, { now: true });
    setEditId(null);
  };

  const theme = editing ? customToAppTheme(editing) : null;
  const derived = theme ? (themeTokens(theme) as unknown as Record<string, string>) : null;

  return (
    <>
      <Row
        anchor="custom-theme"
        label="Custom themes"
        description="Start from the theme you see now, then change any color."
      >
        <Button icon={<PlusIcon />} onClick={create}>
          New custom theme
        </Button>
      </Row>
      {editing && theme && derived && (
        <>
          <Row label="Theme to edit">
            <Select
              width={240}
              value={editing.id}
              options={customs.map((t) => ({ value: t.id, label: t.label || t.id }))}
              onChange={setEditId}
            />
            <Button variant="danger" onClick={() => setConfirmDelete(true)}>
              Delete…
            </Button>
          </Row>
          <Row label="Name">
            <Field value={editing.label} onChange={(label) => edit(editing.id, { label })} />
          </Row>
          <Row label="Description">
            <Field
              value={editing.description}
              onChange={(description) => edit(editing.id, { description })}
            />
          </Row>
          <ColorBlock>
            {THEME_SLOT_GROUPS.map((group) => (
              <ColorGroup
                key={group.heading}
                heading={group.heading}
                slots={group.slots.map((slot) => ({
                  ...slot,
                  value:
                    group.source === 'chrome'
                      ? (derived[slot.key] ?? '')
                      : ((theme.xterm as unknown as Record<string, string>)[slot.key] ?? ''),
                }))}
                onChange={(key, value) => {
                  // A chrome edit pins that token, and converts a theme
                  // saved in the old chrome shape on its first edit.
                  if (group.source === 'chrome') {
                    const chrome = { ...migrateCustomChrome(editing.chrome), [key]: value };
                    edit(editing.id, { chrome: chrome as Record<string, string> });
                  } else {
                    edit(editing.id, { xterm: { ...editing.xterm, [key]: value } });
                  }
                }}
              />
            ))}
          </ColorBlock>
          {confirmDelete && (
            <ConfirmDialog
              title={`Delete ${editing.label || 'this theme'}?`}
              body="Vosh removes the theme and its colors. You cannot undo this."
              confirmLabel="Delete"
              onConfirm={() => {
                setConfirmDelete(false);
                remove(editing.id);
              }}
              onCancel={() => setConfirmDelete(false)}
            />
          )}
        </>
      )}
    </>
  );
}
