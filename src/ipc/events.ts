// Every Tauri event the page hears or sends, each name once. A constant
// is named by its event's path in upper snake case, as the app names the
// ones it sends in src-tauri/src/app/events.rs, so an event goes by one
// name on both sides, and the contract test in
// src-tauri/src/tests/ipc_contract.rs holds them to it. The GMCP
// packages have no constant, since onGmcpPackage in session.ts builds
// each name from the package's, as the session does.

// The events the app sends, in the order of events.rs, which says what
// each carries and who hears it. Settings sends a few of them too.

export const OUTPUT = 'session://output';
export const STATE = 'session://state';
export const INPUT_MODE = 'session://input-mode';
export const TARGET = 'session://target';
export const TICK = 'session://tick';
export const ROUTED = 'session://routed';
export const HIDDEN = 'session://hidden';
export const PROMPT_VARS = 'session://prompt-vars';
export const PROMPT_STATUS = 'session://prompt-status';
export const PROMPT_STATE = 'session://prompt-state';
export const GAME_PROMPT_SEEN = 'session://game-prompt-seen';
export const PROMPT_GAG_WITHOUT_READER = 'session://prompt-gag-without-reader';
export const ALERT = 'session://alert';
export const MARK = 'session://mark';
export const RECONNECT = 'session://reconnect';

export const LUA_OUTPUT = 'session://lua-output';
export const LUA_PANES = 'session://lua-panes';

export const TRIGGERS_CHANGED = 'vosh://triggers-changed';
export const ALIASES_CHANGED = 'vosh://aliases-changed';
export const PROMPT_CONFIG_CHANGED = 'vosh://prompt-config-changed';
export const MACRO_GROUPS_CHANGED = 'vosh://macro-groups-changed';
export const GROUPS_CHANGED = 'vosh://groups-changed';
export const MACROS_CHANGED = 'vosh://macros-changed';
export const TIMERS_CHANGED = 'vosh://timers-changed';

export const PLUGINS_CHANGED = 'vosh://plugins-changed';

export const PROFILES_CHANGED = 'vosh://profiles-changed';
export const PROFILE_SWITCHED = 'vosh://profile-switched';
export const PROFILE_CHANGED = 'vosh://profile-changed';
export const SESSION_IDENTITY_CHANGED = 'vosh://session-identity-changed';
export const SESSION_SELECTED = 'vosh://session-selected';
export const SESSIONS_CHANGED = 'vosh://sessions-changed';
export const CUSTOM_THEMES_CHANGED = 'vosh://custom-themes-changed';
export const LOADOUTS_CHANGED = 'vosh://loadouts-changed';
export const MIGRATION_APPLIED = 'vosh://migration-applied';
export const AFFECT_FULL_CHANGED = 'vosh://affect-full-changed';

export const UI_CONFIG_REPLACED = 'vosh://ui-config-replaced';
export const PANE_LAYOUT_CHANGED = 'vosh://pane-layout-changed';
export const TRACKED_AFFECTS_CHANGED = 'vosh://tracked-affects-changed';
export const TICK_COUNT_CHANGED = 'vosh://tick-count-changed';
export const GAME_TIME_CHANGED = 'vosh://game-time-changed';
export const CHIP_STYLE_CHANGED = 'vosh://chip-style-changed';
export const AFFECTS_DISPLAY_CHANGED = 'vosh://affects-display-changed';
export const CHAT_COLORS_CHANGED = 'vosh://chat-colors-changed';
export const TICK_CONFIG_CHANGED = 'vosh://tick-config-changed';

export const HELP_OPEN = 'vosh://help-open';
export const FLUSH_PENDING_WRITES = 'vosh://flush-pending-writes';
export const APP_MENU = 'vosh://app-menu';
export const SETTINGS_FIND = 'vosh://settings-find';
export const HELP_FIND = 'vosh://help-find';

export const NATIVE_GRID_SIZE = 'vosh://native-grid-size';
export const NATIVE_SCROLL = 'vosh://native-scroll';
export const NATIVE_COPIED = 'vosh://native-copied';
export const TERMINAL_CLICKED = 'vosh://terminal-clicked';
export const TERMINAL_CURSOR = 'vosh://terminal-cursor';

// The events the windows send each other.

// A Settings save sends every window each value that changed, in the
// order broadcastUiConfigChanges in uiConfigBroadcast.ts sends them. The two
// theme events also go out on their own, as on a palette pick.

/** Carries the four theme fields after a save or a palette pick, so a
 *  window that keeps its own copy (Settings, the palette) stays current. */
export const THEME_PREFS_CHANGED = 'vosh://theme-prefs-changed';
export const THEME_CHANGED = 'vosh://theme-changed';
/** The terminal font, its size, and the panel font and size, which
 *  Settings sends every window together. */
export const FONT_CHANGED = 'vosh://font-changed';
export const TERMINAL_LINE_HEIGHT_CHANGED = 'vosh://terminal-line-height-changed';
export const KEEP_LAST_CHANGED = 'vosh://keep-last-changed';
export const THEME_TERMINAL_COLORS_CHANGED = 'vosh://theme-terminal-colors-changed';
export const BRIGHT_BOLD_CHANGED = 'vosh://bright-bold-changed';
export const BLINK_TEXT_CHANGED = 'vosh://blink-text-changed';
export const FIT_GAME_COLORS_CHANGED = 'vosh://fit-game-colors-changed';
export const COLOR_VISION_CHANGED = 'vosh://color-vision-changed';
export const READABLE_HIGHLIGHTS_CHANGED = 'vosh://readable-highlights-changed';
export const BASE_ANSI_CHANGED = 'vosh://base-ansi-changed';
export const SPLIT_DIVIDER_CHANGED = 'vosh://split-divider-changed';
export const INPUT_ECHO_COLOR_CHANGED = 'vosh://input-echo-color-changed';
export const ECHO_MACROS_CHANGED = 'vosh://echo-macros-changed';
export const INPUT_ECHO_CARET_CHANGED = 'vosh://input-echo-caret-changed';
export const PASTE_LINE_DELAY_CHANGED = 'vosh://paste-line-delay-changed';
export const SPELLCHECK_PROMPT_CHANGED = 'vosh://spellcheck-prompt-changed';
export const INPUT_CURSOR_STYLE_CHANGED = 'vosh://input-cursor-style-changed';
export const VITALS_DENSITY_CHANGED = 'vosh://vitals-density-changed';
export const VITALS_OPTIONS_CHANGED = 'vosh://vitals-options-changed';

/** Takes an open Settings window to a target, from openSettingsTab. */
export const SETTINGS_GOTO_TAB = 'vosh://settings-goto-tab';
/** Takes an open Help window to a target, from openHelpTopic. */
export const HELP_GOTO = 'vosh://help-goto';
/** Carries the Connect target you saved to every window. */
export const CONNECTION_TARGET_CHANGED = 'vosh://connection-target-changed';
/** Asks the main window to open the prompt card, from openPromptCard. */
export const PROMPT_CARD_OPEN = 'vosh://prompt-card-open';
