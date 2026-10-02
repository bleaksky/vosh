import { useEffect, useId, useRef, useState } from 'react';
import {
  BUNDLED_FONTS,
  fontChoices,
  pairChoices,
  sizeChoices,
} from '../../../lib/appearanceSettings';
import {
  listSystemFonts,
  resolveThemeTerminalColors,
  type CustomTheme,
  type SystemFontEntry,
  type TerminalLineHeight,
  type UiConfig,
} from '../../../lib/session';
import type { Appearance } from '../../../lib/chrome';
import type { SettingsTarget } from '../../../lib/settingsNav';
import {
  activeThemeFor,
  applyThemePrefs,
  pickTheme,
  systemPrefersDark,
  themePrefsOf,
} from '../../../lib/theme';
import { parseThemeFile, ThemeFileError } from '../../../lib/themeImport';
import { galleryThemes } from '../../../lib/themeThumb';
import { BUILTIN_THEMES, customToAppTheme, setCustomThemes } from '../../../lib/themes';
import { useSettingsAutoSave } from '../legacy/useSettingsAutoSave';
import type { SettingsPageProps } from '../pageTypes';
import { Button, Card, Row, Section, Segmented, Select, Toggle } from '../ui';
import { AdvancedAppearance } from './appearance/AdvancedAppearance';
import { ThemeGallery } from './appearance/ThemeGallery';

// Appearance, from the approved board (SettingsAppearance.dc.html).
// Theme holds Import… and the gallery of every theme, then follow
// system appearance and the light and dark pair it switches between.
// Terminal text holds the font, size, line height, and whether MUD
// text takes the theme's colors. A quiet Advanced row at the end holds
// what the board leaves out. Every change saves on its own.

const LINE_HEIGHTS = [
  { value: 'compact', label: 'Compact' },
  { value: 'default', label: 'Default' },
  { value: 'loose', label: 'Loose' },
] as const;

// The four formats parseThemeFile reads. macOS lists every file anyway,
// which Ghostty's theme files need, since they have no extension.
const THEME_FILE_TYPES = '.itermcolors,.conf,.toml,.yml,.yaml';

const ADVANCED_ANCHORS: ReadonlySet<string> = new Set([
  'custom-theme',
  'base-palette',
  'bright-bold',
  'blink-text',
  'font-stack',
]);

/** A target that lands inside Advanced opens it. */
function opensAdvanced(target: SettingsTarget): boolean {
  return (
    target.section === 'advanced' ||
    (target.anchor !== undefined && ADVANCED_ANCHORS.has(target.anchor))
  );
}

/** The system the follow row names. */
function systemName(): string {
  const platform = typeof document === 'undefined' ? '' : document.documentElement.dataset.platform;
  if (platform === 'macos') return 'macOS';
  if (platform === 'windows') return 'Windows';
  return 'your system';
}

export function AppearancePage({ target, navSeq, config, setConfig, onError }: SettingsPageProps) {
  const { update } = useSettingsAutoSave(setConfig, onError);
  const [advancedOpen, setAdvancedOpen] = useState(() => opensAdvanced(target));
  const [importError, setImportError] = useState<string | null>(null);
  const [installedFonts, setInstalledFonts] = useState<SystemFontEntry[]>([]);
  const fileRef = useRef<HTMLInputElement | null>(null);
  const hintId = useId();
  // The latest config for the import, which reads the file first.
  const configRef = useRef<UiConfig | null>(config);
  useEffect(() => {
    configRef.current = config;
  }, [config]);

  useEffect(() => {
    if (opensAdvanced(target)) setAdvancedOpen(true);
    // navSeq marks each navigation, even to the same target.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [navSeq]);

  // The installed fonts fill the Font select. The backend reads them
  // once per launch and keeps the list.
  useEffect(() => {
    let cancelled = false;
    void listSystemFonts().then((list) => {
      if (!cancelled) setInstalledFonts(list);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  if (!config) return null;

  const themes = galleryThemes(BUILTIN_THEMES, config.custom_themes.map(customToAppTheme));
  const shown = activeThemeFor(config);
  // While follow is on the arrow keys stay among the themes the OS
  // shows now, so stepping through the gallery never fills the other
  // slot and each step lands on the radio it checks.
  const arrowAppearance: Appearance | undefined = config.follow_system_appearance
    ? systemPrefersDark()
      ? 'dark'
      : 'light'
    : undefined;

  // A pick follows pickTheme. While follow is off it becomes your theme.
  // While follow is on it fills the light or dark entry, and shows only
  // when that matches the OS, which is what activeThemeFor resolves.
  const pick = (id: string) => {
    const next = pickTheme(config, id);
    applyThemePrefs(next);
    update(themePrefsOf(next), { now: true });
  };

  const setPrefs = (patch: Partial<UiConfig>) => {
    applyThemePrefs({ ...config, ...patch });
    update(patch, { now: true });
  };

  const addTheme = (theme: CustomTheme) => {
    const current = configRef.current;
    if (!current) return;
    const list = [...current.custom_themes, theme];
    setCustomThemes(list.map(customToAppTheme));
    const next = pickTheme({ ...current, custom_themes: list }, theme.id);
    applyThemePrefs(next);
    update({ custom_themes: list, ...themePrefsOf(next) }, { now: true });
  };

  const importFile = async (file: File) => {
    setImportError(null);
    let text: string;
    try {
      text = await file.text();
    } catch {
      setImportError('Vosh could not open that file.');
      return;
    }
    const current = configRef.current;
    if (!current) return;
    const taken = [...BUILTIN_THEMES.map((t) => t.id), ...current.custom_themes.map((t) => t.id)];
    try {
      addTheme(parseThemeFile(file.name, text, taken));
    } catch (e) {
      setImportError(
        e instanceof ThemeFileError ? e.message : 'Vosh could not read that theme file.',
      );
    }
  };

  const fontValue = config.font_family || BUNDLED_FONTS[0].value;

  return (
    <>
      <Section
        id="theme"
        title="Theme"
        card={false}
        help={{ topic: 'make-it-yours.switch-themes', subject: 'themes' }}
        actions={
          <div className="st-import" data-st-anchor="import-theme" data-st-flash="">
            <span id={hintId} className="st-meta">
              Vosh reads Ghostty, iTerm2, Kitty, and Alacritty themes.
            </span>
            <Button aria-describedby={hintId} onClick={() => fileRef.current?.click()}>
              Import…
            </Button>
            <input
              ref={fileRef}
              type="file"
              accept={THEME_FILE_TYPES}
              className="st-visually-hidden"
              tabIndex={-1}
              aria-hidden="true"
              onChange={(e) => {
                const file = e.target.files?.[0];
                // Clear the pick so the same file can load again.
                e.target.value = '';
                if (file) void importFile(file);
              }}
            />
          </div>
        }
      >
        {importError && (
          <p className="st-import-error" role="alert">
            {importError}
          </p>
        )}
        <Card>
          <ThemeGallery
            themes={themes}
            selected={shown}
            onPick={pick}
            appearance={arrowAppearance}
          />
          <Row
            anchor="follow-system"
            label="Follow system appearance"
            description={`Vosh switches between your light and dark theme when ${systemName()} does.`}
          >
            <Toggle
              checked={config.follow_system_appearance}
              onChange={(on) => setPrefs({ follow_system_appearance: on })}
            />
          </Row>
          <Row anchor="light-theme" label="Light theme">
            <Select
              value={config.light_theme}
              options={pairChoices(themes, 'light', config.light_theme)}
              onChange={(id) => setPrefs({ light_theme: id })}
            />
          </Row>
          <Row anchor="dark-theme" label="Dark theme">
            <Select
              value={config.dark_theme}
              options={pairChoices(themes, 'dark', config.dark_theme)}
              onChange={(id) => setPrefs({ dark_theme: id })}
            />
          </Row>
        </Card>
      </Section>

      <Section id="text" title="Terminal text">
        <Row anchor="font" label="Font">
          <Select
            value={fontValue}
            options={fontChoices(fontValue, installedFonts)}
            onChange={(family) => update({ font_family: family }, { now: true })}
          />
        </Row>
        <Row anchor="size" label="Size">
          <Select
            value={String(config.font_size)}
            options={sizeChoices(config.font_size)}
            onChange={(size) => update({ font_size: Number(size) }, { now: true })}
          />
        </Row>
        <Row anchor="line-height" label="Line height">
          <Segmented<TerminalLineHeight>
            options={LINE_HEIGHTS}
            value={config.terminal_line_height}
            onChange={(value) => update({ terminal_line_height: value }, { now: true })}
          />
        </Row>
        <Row
          anchor="theme-colors"
          label="Use the theme's colors for MUD text"
          description="Turn this off to keep the exact colors your MUD sends."
        >
          <Toggle
            checked={resolveThemeTerminalColors(config.theme, config.theme_terminal_colors)}
            onChange={(on) => update({ theme_terminal_colors: on }, { now: true })}
          />
        </Row>
      </Section>

      <AdvancedAppearance
        config={config}
        update={update}
        open={advancedOpen}
        onToggle={() => setAdvancedOpen((open) => !open)}
      />
    </>
  );
}
