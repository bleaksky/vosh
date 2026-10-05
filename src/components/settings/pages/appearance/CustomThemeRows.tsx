import { useEffect, useRef, useState } from 'react';
import {
  copyTheme,
  customFitKey,
  editCustomTheme,
  removeCustomTheme,
  THEME_SLOT_GROUPS,
} from '../../../../theme/appearanceSettings';
import { emitCustomThemesChanged, emitThemeChanged, type CustomTheme } from '../../../../ipc/theme';
import type { UiConfig } from '../../../../ipc/uiConfig';
import {
  activeThemeFor,
  applyTheme,
  applyThemePrefs,
  pickTheme,
  themePrefsOf,
} from '../../../../theme/theme';
import {
  BUILTIN_THEMES,
  customThemeLabel,
  customToAppTheme,
  findTheme,
  migrateCustomChrome,
  setCustomThemes,
  themeTokens,
} from '../../../../theme/themes';
import { ConfirmDialog } from '../../../ConfirmDialog';
import type { UpdateConfig } from '../../legacy/useSettingsAutoSave';
import { Button, Field, PlusIcon, Row, Select } from '../../ui';
import { ColorBlock, ColorGroup } from './ColorGrid';
import { fitAndKeep } from './fitAndKeep';

/** How long the colors rest after an edit before Vosh fits the game
 *  colors to them, so a drag through the picker fits once. */
export const FIT_SETTLE_MS = 600;

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
  // The fit of colors still settling after an edit. Leaving the rows
  // runs it at once, so the fit is kept.
  const refit = useRef<{ timer: ReturnType<typeof setTimeout>; run: () => void } | undefined>(
    undefined,
  );
  useEffect(
    () => () => {
      if (!refit.current) return;
      clearTimeout(refit.current.timer);
      refit.current.run();
    },
    [],
  );
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
    // A copy keeps the fit of the theme it copies, and one with none
    // to keep is fitted now.
    if (!theme.fitted) fitAndKeep(theme, update);
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
      void emitCustomThemesChanged(list)
        .then(() => emitThemeChanged(id))
        .catch(() => {});
    }
    update({ custom_themes: list });
    // A change to a color the fit reads dropped the fit in
    // editCustomTheme. Fit the new colors once they rest.
    const before = customs.find((t) => t.id === id);
    const after = list.find((t) => t.id === id);
    if (before && after && customFitKey(before) !== customFitKey(after)) {
      if (refit.current) clearTimeout(refit.current.timer);
      const run = () => {
        refit.current = undefined;
        fitAndKeep(after, update);
      };
      refit.current = { timer: setTimeout(run, FIT_SETTLE_MS), run };
    }
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
              options={customs.map((t) => ({ value: t.id, label: customThemeLabel(t) }))}
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
              title={`Delete ${theme.label}?`}
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
