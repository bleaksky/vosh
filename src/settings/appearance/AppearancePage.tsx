import { useEffect, useId, useRef, useState, useSyncExternalStore } from 'react';
import {
  BUNDLED_FONTS,
  colorVisionNote,
  fontChoices,
  pairChoices,
  panelFontChoices,
  panelSizeChoices,
  sizeChoices,
  themeCaption,
} from '../../theme/appearanceSettings';
import { toColorVision, type ColorVision } from '../../theme/gameFit';
import { normalizePanelFont } from '../../panel/panelFont';
import { normalizePanelSize } from '../../panel/panelSize';
import type { CustomTheme } from '../../ipc/theme';
import {
  listSystemFonts,
  type SystemFontEntry,
  type TerminalLineHeight,
  type ThemeFollow,
  type UiConfig,
} from '../../ipc/uiConfig';
import type { Appearance } from '../../theme/chrome';
import type { SettingsTarget } from '../../lib/settingsNav';
import {
  getDaylight,
  startDaylightStore,
  subscribeDaylight,
} from '../../stores/session/daylightStore';
import {
  activeThemeFor,
  applyThemePrefs,
  daylightShown,
  pickTheme,
  systemPrefersDark,
  themeFollowOf,
  themePrefsOf,
} from '../../theme/theme';
import { parseThemeFile, ThemeFileError } from '../../theme/themeImport';
import { galleryThemes } from '../../theme/themeThumb';
import {
  BUILTIN_THEMES,
  customToAppTheme,
  findTheme,
  resolveThemeTerminalColors,
  RETIRED_THEMES,
  setCustomThemes,
  themeShownBy,
} from '../../theme/themes';
import { useSettingsAutoSave } from '../useSettingsAutoSave';
import type { SettingsPageProps } from '../pageTypes';
import { Button, Card, Row, Section, Segmented, Select, Toggle } from '../../ui';
import { AdvancedAppearance } from './AdvancedAppearance';
import { CollapseRows } from './CollapseRows';
import { ThemeGallery } from './ThemeGallery';
import { fitAndKeep } from './fitAndKeep';

// Appearance. Theme holds Import… and the gallery of every theme, with
// a Vision switch that previews the tiles as each color vision sees
// them, a caption that describes the theme on screen and credits its
// colors, then Switch themes, which follows the system with a light and
// dark pair or the game's day with a day and night pair. Terminal text
// holds the font, the size, the line height, whether MUD text takes the
// theme's colors, whether play fits the game's colors to the theme,
// which color vision the game text and the window's status colors
// follow, whether Vosh keeps the colors your triggers set readable on
// the theme, and whether a line the same as the one before it shows
// once with a count. While that is on, two rows under it choose whether
// the lines of a fight collapse, and whether attack lines do. A link to
// either row shows them even while it is off, so search lands on them.
// Panel text holds the font and the size that every pane and the status
// line draw in, so each section sets one thing. A quiet Advanced row at
// the end holds the rest. Every change saves on its own.

const LINE_HEIGHTS = [
  { value: 'compact', label: 'Compact' },
  { value: 'default', label: 'Default' },
  { value: 'loose', label: 'Loose' },
] as const;

const THEME_FOLLOW_CHOICES = [
  { value: 'off', label: 'Off' },
  { value: 'system', label: 'With the system' },
  { value: 'game', label: 'With the game' },
] as const;

const COLOR_VISIONS = [
  { value: 'typical', label: 'Typical' },
  { value: 'deuteranopia', label: 'Deuteranopia' },
  { value: 'protanopia', label: 'Protanopia' },
  { value: 'tritanopia', label: 'Tritanopia' },
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

/** The rows under Collapse repeated lines. */
const COLLAPSE_ANCHORS: ReadonlySet<string> = new Set(['collapse-fights', 'collapse-attacks']);

/** The rows under Collapse repeated lines show while it is on, and a
 *  link to either shows them while it is off, waiting. */
function showsCollapseRows(config: UiConfig, target: SettingsTarget): boolean {
  return (
    config.collapse_repeats || (target.anchor !== undefined && COLLAPSE_ANCHORS.has(target.anchor))
  );
}

/** The rows of the pair each Switch themes mode switches between. */
const PAIR_ANCHORS: Readonly<Record<'system' | 'game', readonly string[]>> = {
  system: ['light-theme', 'dark-theme'],
  game: ['day-theme', 'night-theme'],
};

/** A pair shows while Switch themes follows its mode, and a link to
 *  either of its rows shows it in any mode, so search lands on it. */
function showsPair(mode: 'system' | 'game', follow: ThemeFollow, target: SettingsTarget): boolean {
  return (
    follow === mode || (target.anchor !== undefined && PAIR_ANCHORS[mode].includes(target.anchor))
  );
}

/** The system Switch themes names. */
function systemName(): string {
  const platform = typeof document === 'undefined' ? '' : document.documentElement.dataset.platform;
  if (platform === 'macos') return 'macOS';
  if (platform === 'windows') return 'Windows';
  return 'your system';
}

/** What Switch themes says under its label in each mode. */
function switchThemesLine(mode: ThemeFollow): string | undefined {
  if (mode === 'system') {
    return `Vosh switches between your light and dark theme when ${systemName()} does.`;
  }
  if (mode === 'game') return "Turns at the game's dawn and dusk, about every 6 minutes.";
  return undefined;
}

const hearNothing = () => () => undefined;

export function AppearancePage({ target, navSeq, config, setConfig, onError }: SettingsPageProps) {
  const { update } = useSettingsAutoSave(setConfig, onError);
  const [advancedOpen, setAdvancedOpen] = useState(() => opensAdvanced(target));
  const [importError, setImportError] = useState<string | null>(null);
  // The vision the gallery previews. It shows the Color vision you
  // picked until you switch it, and it never saves.
  const [preview, setPreview] = useState<ColorVision | null>(null);
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

  // The installed fonts fill the Font and Panel font selects. The
  // backend reads them once per launch and keeps the list.
  useEffect(() => {
    let cancelled = false;
    void listSystemFonts().then((list) => {
      if (!cancelled) setInstalledFonts(list);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  // The page hears the game's day or night from the start, so choosing
  // With the game knows whether the game already said, and keeps your
  // theme when it did.
  useEffect(() => {
    startDaylightStore();
  }, []);

  // A custom theme that keeps no fit is fitted once the page opens on
  // your config, and keeps the fit: one imported before Vosh kept fits,
  // one Vosh 0.8.1 saved, which drops the fit, and one whose fit Settings
  // closed before it could keep.
  const loaded = config !== null;
  useEffect(() => {
    for (const theme of configRef.current?.custom_themes ?? []) {
      if (!theme.fitted) fitAndKeep(theme, update);
    }
    // The page asks once, when your config is there.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [loaded]);

  // With the game the gallery rings the theme the game's day or night
  // shows, so the page draws again at each turn.
  const follow = config ? themeFollowOf(config) : 'off';
  useSyncExternalStore(follow === 'game' ? subscribeDaylight : hearNothing, getDaylight);

  if (!config) return null;

  const themes = galleryThemes(BUILTIN_THEMES, config.custom_themes.map(customToAppTheme));
  // The id of the theme a saved pick shows. A retired id shows its
  // successor, which the gallery and the selects mark as chosen.
  const shownId = (id: string) => themeShownBy(themes, id)?.id ?? id;
  const shown = shownId(activeThemeFor(config));
  const lightTheme = shownId(config.light_theme);
  const darkTheme = shownId(config.dark_theme);
  // A blank day or night slot shows your theme.
  const dayTheme = shownId(config.day_theme || config.theme);
  const nightTheme = shownId(config.night_theme || config.theme);
  // An id no theme has draws the fallback theme, so the caption names it.
  const shownTheme = themes.find((t) => t.id === shown) ?? findTheme(shown);
  const caption = themeCaption(shownTheme);
  const visionNote = colorVisionNote(
    config.color_vision,
    resolveThemeTerminalColors(config.theme_terminal_colors),
  );
  // With the system the arrow keys stay among the themes the OS shows
  // now, so stepping through the gallery never fills the other slot and
  // each step lands on the radio it checks. With the game a pick fills
  // the slot showing, whatever its appearance, so every theme counts.
  const arrowAppearance: Appearance | undefined =
    follow === 'system' ? (systemPrefersDark() ? 'dark' : 'light') : undefined;

  // A pick follows pickTheme. Off it becomes your theme. With the system
  // it fills the light or dark entry, and shows only when that matches
  // the OS. With the game it fills the day or night entry showing now.
  const pick = (id: string) => {
    const next = pickTheme(config, id);
    applyThemePrefs(next);
    update(themePrefsOf(next), { now: true });
  };

  // Every theme row saves the seven theme fields together, as
  // THEME_PREFS_CHANGED carries them, so a dark theme the page seeded
  // from your theme saves too.
  const setPrefs = (patch: Partial<UiConfig>) => {
    const next = { ...config, ...patch };
    applyThemePrefs(next);
    update(themePrefsOf(next), { now: true });
  };

  // Choosing With the game starts a blank day or night slot on the
  // theme showing, so nothing changes until you pick. Before the game
  // says day or night your theme shows, so it takes the theme showing
  // too.
  const setFollow = (mode: ThemeFollow) => {
    const patch: Partial<UiConfig> = {
      theme_follow: mode,
      follow_system_appearance: mode === 'system',
    };
    if (mode === 'game') {
      if (config.day_theme === '') patch.day_theme = shown;
      if (config.night_theme === '') patch.night_theme = shown;
      if (daylightShown() === null) patch.theme = shown;
    }
    setPrefs(patch);
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
    // A saved pick may still name a retired id, so an import never takes one.
    const taken = [
      ...BUILTIN_THEMES.map((t) => t.id),
      ...RETIRED_THEMES.keys(),
      ...current.custom_themes.map((t) => t.id),
    ];
    let theme: CustomTheme;
    try {
      theme = parseThemeFile(file.name, text, taken);
    } catch (e) {
      setImportError(
        e instanceof ThemeFileError ? e.message : 'Vosh could not read that theme file.',
      );
      return;
    }
    // The theme shows at once, and its game colors are fitted once,
    // off the main thread, and kept with it.
    addTheme(theme);
    fitAndKeep(theme, update);
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
            vision={preview ?? config.color_vision}
            onVision={setPreview}
          />
          {caption !== '' && <p className="st-meta st-theme-caption">{caption}</p>}
          <Row anchor="switch-themes" label="Switch themes" description={switchThemesLine(follow)}>
            <Segmented<ThemeFollow>
              options={THEME_FOLLOW_CHOICES}
              value={follow}
              onChange={setFollow}
            />
          </Row>
          {showsPair('system', follow, target) && (
            <>
              <Row anchor="light-theme" label="Light theme">
                <Select
                  value={lightTheme}
                  options={pairChoices(themes, 'light', lightTheme)}
                  onChange={(id) => setPrefs({ light_theme: id })}
                />
              </Row>
              <Row anchor="dark-theme" label="Dark theme">
                <Select
                  value={darkTheme}
                  options={pairChoices(themes, 'dark', darkTheme)}
                  onChange={(id) => setPrefs({ dark_theme: id })}
                />
              </Row>
            </>
          )}
          {showsPair('game', follow, target) && (
            <>
              <Row anchor="day-theme" label="Day theme">
                <Select
                  value={dayTheme}
                  options={pairChoices(themes, null, dayTheme)}
                  onChange={(id) => setPrefs({ day_theme: id })}
                />
              </Row>
              <Row anchor="night-theme" label="Night theme">
                <Select
                  value={nightTheme}
                  options={pairChoices(themes, null, nightTheme)}
                  onChange={(id) => setPrefs({ night_theme: id })}
                />
              </Row>
            </>
          )}
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
            checked={resolveThemeTerminalColors(config.theme_terminal_colors)}
            onChange={(on) => update({ theme_terminal_colors: on }, { now: true })}
          />
        </Row>
        <Row
          anchor="fit-game-colors"
          label="Fit game colors"
          description="While you play, Vosh lifts the game colors that fade on the theme, and Settings keeps the theme as published."
        >
          <Toggle
            checked={config.fit_game_colors}
            onChange={(on) => update({ fit_game_colors: on }, { now: true })}
          />
        </Row>
        <Row
          anchor="color-vision"
          label="Color vision"
          description={
            <>
              Vosh swaps the colors your eyes confuse for colors they tell apart, the way color
              blind modes in games do.
              {visionNote !== '' && (
                <>
                  <br />
                  {visionNote}
                </>
              )}
            </>
          }
        >
          <Select
            value={config.color_vision}
            options={COLOR_VISIONS}
            onChange={(vision) => update({ color_vision: toColorVision(vision) }, { now: true })}
          />
        </Row>
        <Row
          anchor="readable-highlights"
          label="Keep highlight colors readable"
          description="Vosh darkens or lightens a color your triggers set when the theme would make it faint."
        >
          <Toggle
            checked={config.readable_highlights}
            onChange={(on) => update({ readable_highlights: on }, { now: true })}
          />
        </Row>
        <Row
          anchor="collapse-repeats"
          label="Collapse repeated lines"
          description="A line the same as the line before it shows once, with a count in front."
        >
          <Toggle
            checked={config.collapse_repeats}
            onChange={(on) => update({ collapse_repeats: on }, { now: true })}
          />
        </Row>
        {showsCollapseRows(config, target) && <CollapseRows config={config} update={update} />}
      </Section>

      <Section id="panel-text" title="Panel text">
        <Row
          anchor="panel-font"
          label="Font"
          description="Every pane and the status line under the terminal draw in it."
        >
          <Select
            value={normalizePanelFont(config.panel_font)}
            options={panelFontChoices(config.panel_font, installedFonts)}
            onChange={(pick) => update({ panel_font: pick }, { now: true })}
          />
        </Row>
        <Row
          anchor="panel-size"
          label="Size"
          description="The headers, the rows, and the status line grow with it."
        >
          <Select
            value={String(normalizePanelSize(config.panel_font_size))}
            options={panelSizeChoices(config.panel_font_size)}
            onChange={(size) => update({ panel_font_size: Number(size) }, { now: true })}
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
