//! The commands behind the Settings config. Settings reads and saves
//! the whole UI config through them and lists the system fonts for the
//! font picker. The main window's palette saves a theme pick, and the
//! chat pane's menu its channel colors, without the rest of the config.

use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::app::events::CHAT_COLORS_CHANGED;
use crate::app::state::SharedState;
use crate::app::system_fonts::FontEntry;
use crate::disk::save::{persist_profile, save_then_broadcast, SavePolicy};
use crate::profile::open::OpenProfile;

/// The Settings payload. Every field falls back to the default a fresh
/// profile has, so a page that leaves one out still saves (D12).
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(crate) struct UiConfigPayload {
    pub theme: String,
    pub follow_system_appearance: bool,
    pub light_theme: String,
    pub dark_theme: String,
    pub auto_update: bool,
    pub font_family: String,
    pub font_size: u32,
    pub terminal_line_height: String,
    /// Empty for As designed, `terminal`, `system`, or a font list.
    pub panel_font: String,
    /// The panel size in pixels, or 0 for the terminal size.
    pub panel_font_size: u32,
    pub tracked_affects: Vec<crate::profile::ui::TrackedAffect>,
    pub enabled_presets: Vec<String>,
    pub keep_last_command: bool,
    pub theme_terminal_colors: Option<bool>,
    pub bright_bold: bool,
    /// None until you choose.
    #[serde(default)]
    pub blink_text: Option<bool>,
    pub fit_game_colors: bool,
    /// `typical`, `deuteranopia`, `protanopia` or `tritanopia`.
    pub color_vision: String,
    pub readable_highlights: bool,
    pub collapse_repeats: bool,
    pub collapse_fight_lines: bool,
    pub collapse_attack_lines: bool,
    pub terminal_base_ansi: Option<Vec<String>>,
    pub custom_themes: Vec<crate::profile::ui::CustomTheme>,
    pub split_divider_color: Option<String>,
    pub input_echo_color: Option<String>,
    pub echo_macros: bool,
    pub input_echo_caret: bool,
    pub paste_line_delay_ms: u32,
    pub spellcheck_prompt: bool,
    pub input_cursor_style: String,
    pub vitals_density: String,
    pub vitals_values: String,
    pub vitals_meter: String,
    pub vitals_warn_thirds: bool,
    pub vitals_hide_when_pinned: bool,
    pub chip_style: String,
    pub tick_count: String,
    pub game_time: String,
    pub affects_style: String,
    pub affects_marker: String,
    pub affects_tint: bool,
    #[serde(deserialize_with = "crate::profile::ui::deserialize_affects_running_out_hours")]
    pub affects_running_out_hours: u32,
    #[serde(deserialize_with = "crate::profile::ui::deserialize_affects_almost_gone_hours")]
    pub affects_almost_gone_hours: u32,
    /// The [`AppState::ui_config_generation`] this copy was read at. Never
    /// reaches disk. A save without one (a config that never came from
    /// the backend) applies.
    ///
    /// [`AppState::ui_config_generation`]: crate::app::state::AppState::ui_config_generation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<u64>,
}

impl Default for UiConfigPayload {
    fn default() -> Self {
        Self::from_ui(&crate::profile::ui::UiConfig::default())
    }
}

impl UiConfigPayload {
    /// The snapshot `ui_get_config` hands the frontend.
    pub(crate) fn from_ui(ui: &crate::profile::ui::UiConfig) -> Self {
        Self {
            theme: ui.theme.clone(),
            follow_system_appearance: ui.follow_system_appearance,
            light_theme: ui.light_theme.clone(),
            dark_theme: ui.dark_theme.clone(),
            auto_update: ui.auto_update,
            font_family: ui.font_family.clone(),
            font_size: ui.font_size,
            terminal_line_height: ui.terminal_line_height.clone(),
            panel_font: ui.panel_font.clone(),
            panel_font_size: ui.panel_font_size,
            tracked_affects: ui.tracked_affects.clone(),
            enabled_presets: ui.enabled_presets.clone(),
            keep_last_command: ui.keep_last_command,
            theme_terminal_colors: ui.theme_terminal_colors,
            bright_bold: ui.bright_bold,
            blink_text: ui.blink_text,
            fit_game_colors: ui.fit_game_colors,
            color_vision: ui.color_vision.clone(),
            readable_highlights: ui.readable_highlights,
            collapse_repeats: ui.collapse_repeats,
            collapse_fight_lines: ui.collapse_fight_lines,
            collapse_attack_lines: ui.collapse_attack_lines,
            terminal_base_ansi: ui.terminal_base_ansi.clone(),
            custom_themes: ui.custom_themes.clone(),
            split_divider_color: ui.split_divider_color.clone(),
            input_echo_color: ui.input_echo_color.clone(),
            echo_macros: ui.echo_macros,
            input_echo_caret: ui.input_echo_caret,
            paste_line_delay_ms: ui.paste_line_delay_ms,
            spellcheck_prompt: ui.spellcheck_prompt,
            input_cursor_style: ui.input_cursor_style.clone(),
            vitals_density: ui.vitals_density.clone(),
            vitals_values: ui.vitals_values.clone(),
            vitals_meter: ui.vitals_meter.clone(),
            vitals_warn_thirds: ui.vitals_warn_thirds,
            vitals_hide_when_pinned: ui.vitals_hide_when_pinned,
            chip_style: ui.chip_style.clone(),
            tick_count: ui.tick_count.clone(),
            game_time: ui.game_time.clone(),
            affects_style: ui.affects_style.clone(),
            affects_marker: ui.affects_marker.clone(),
            affects_tint: ui.affects_tint,
            affects_running_out_hours: ui.affects_running_out_hours,
            affects_almost_gone_hours: ui.affects_almost_gone_hours,
            generation: None,
        }
    }

    /// Write every field onto the live UI config, normalizing as it
    /// goes. `ui_set_config` calls this, and each Settings tab saves the
    /// whole snapshot, so a field left out here would reset on the next
    /// save from any tab. `dock_layout` stays out on purpose, since only
    /// the conversion from the old dock to panes reads it. So do the old
    /// `vitals`, `moons_position` and `side_panels_fill_height`, which
    /// nothing reads and every save writes back as loaded (D12, D14).
    pub(crate) fn apply_to(self, ui: &mut crate::profile::ui::UiConfig) {
        let UiConfigPayload {
            theme,
            follow_system_appearance,
            light_theme,
            dark_theme,
            auto_update,
            font_family,
            font_size,
            terminal_line_height,
            panel_font,
            panel_font_size,
            tracked_affects,
            enabled_presets,
            keep_last_command,
            theme_terminal_colors,
            bright_bold,
            blink_text,
            fit_game_colors,
            color_vision,
            readable_highlights,
            collapse_repeats,
            collapse_fight_lines,
            collapse_attack_lines,
            terminal_base_ansi,
            custom_themes,
            split_divider_color,
            input_echo_color,
            echo_macros,
            input_echo_caret,
            paste_line_delay_ms,
            spellcheck_prompt,
            input_cursor_style,
            vitals_density,
            vitals_values,
            vitals_meter,
            vitals_warn_thirds,
            vitals_hide_when_pinned,
            chip_style,
            tick_count,
            game_time,
            affects_style,
            affects_marker,
            affects_tint,
            affects_running_out_hours,
            affects_almost_gone_hours,
            generation: _,
        } = self;
        ui.theme = theme;
        ui.follow_system_appearance = follow_system_appearance;
        ui.light_theme = crate::profile::ui::coerce_light_theme(light_theme);
        ui.dark_theme = crate::profile::ui::normalize_dark_theme(dark_theme);
        ui.auto_update = auto_update;
        ui.font_family = font_family;
        ui.font_size = crate::profile::ui::coerce_font_size(font_size);
        ui.terminal_line_height =
            crate::profile::ui::coerce_terminal_line_height(terminal_line_height);
        ui.panel_font = crate::profile::ui::normalize_panel_font(panel_font);
        ui.panel_font_size = crate::profile::ui::coerce_panel_font_size(panel_font_size);
        ui.tracked_affects = crate::profile::ui::normalize_tracked_affects(tracked_affects);
        ui.enabled_presets = crate::profile::ui::normalize_enabled_presets(enabled_presets);
        ui.keep_last_command = keep_last_command;
        ui.theme_terminal_colors = theme_terminal_colors;
        ui.bright_bold = bright_bold;
        ui.blink_text = blink_text;
        ui.fit_game_colors = fit_game_colors;
        ui.color_vision = crate::profile::ui::coerce_color_vision(color_vision);
        ui.readable_highlights = readable_highlights;
        ui.collapse_repeats = collapse_repeats;
        ui.collapse_fight_lines = collapse_fight_lines;
        ui.collapse_attack_lines = collapse_attack_lines;
        ui.terminal_base_ansi = terminal_base_ansi;
        ui.custom_themes = custom_themes;
        ui.split_divider_color = crate::profile::ui::normalize_optional_color(split_divider_color);
        ui.input_echo_color = crate::profile::ui::normalize_optional_color(input_echo_color);
        ui.echo_macros = echo_macros;
        ui.input_echo_caret = input_echo_caret;
        ui.paste_line_delay_ms =
            crate::profile::ui::coerce_paste_line_delay_ms(paste_line_delay_ms);
        ui.spellcheck_prompt = spellcheck_prompt;
        ui.input_cursor_style = crate::profile::ui::coerce_input_cursor_style(input_cursor_style);
        ui.vitals_density = crate::profile::ui::coerce_vitals_density(vitals_density);
        ui.vitals_values = crate::profile::ui::coerce_vitals_values(vitals_values);
        ui.vitals_meter = crate::profile::ui::coerce_vitals_meter(vitals_meter);
        ui.vitals_warn_thirds = vitals_warn_thirds;
        ui.vitals_hide_when_pinned = vitals_hide_when_pinned;
        ui.chip_style = crate::profile::ui::coerce_chip_style(chip_style);
        ui.tick_count = crate::profile::ui::coerce_tick_count(tick_count);
        ui.game_time = crate::profile::ui::coerce_game_time(game_time);
        ui.affects_style = crate::profile::ui::coerce_affects_style(affects_style);
        ui.affects_marker = crate::profile::ui::coerce_affects_marker(affects_marker);
        ui.affects_tint = affects_tint;
        (ui.affects_running_out_hours, ui.affects_almost_gone_hours) =
            crate::profile::ui::coerce_affects_thresholds(
                affects_running_out_hours,
                affects_almost_gone_hours,
            );
    }
}

/// The live UI config and the [`AppState::ui_config_generation`] it was
/// read at.
///
/// [`AppState::ui_config_generation`]: crate::app::state::AppState::ui_config_generation
#[tauri::command]
pub(crate) async fn ui_get_config(
    state: State<'_, SharedState>,
) -> Result<UiConfigPayload, String> {
    let p = state.selected_session().lock_profile().await;
    Ok(ui_config_of(&p, state.ui_config_generation()))
}

/// What `ui_get_config` hands the webview for the live profile `p`, read
/// at `generation`. Your prompt is not in it: the prompt section reads and
/// writes the `[prompt]` table through the prompt commands, and `[ui]`
/// keeps only a copy of its switch and design for an older build.
fn ui_config_of(p: &crate::profile::live::Profile, generation: u64) -> UiConfigPayload {
    let mut payload = UiConfigPayload::from_ui(&p.ui);
    payload.generation = Some(generation);
    payload
}

/// Save the whole UI config. A copy read before the live config was last
/// replaced is refused and returns false, and the caller reads the
/// config again.
#[tauri::command]
pub(crate) async fn ui_set_config(
    state: State<'_, SharedState>,
    config: UiConfigPayload,
) -> Result<bool, String> {
    let open = {
        let mut p = state.selected_session().lock_profile().await;
        if !apply_ui_config(&mut p.ui, config, state.ui_config_generation()) {
            return Ok(false);
        }
        p.open().clone()
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&shared, &open).await;
    Ok(true)
}

/// Write a whole config save onto `ui`, unless it was read at a
/// generation other than `current`. Returns whether it applied.
fn apply_ui_config(
    ui: &mut crate::profile::ui::UiConfig,
    config: UiConfigPayload,
    current: u64,
) -> bool {
    if config.generation.is_some_and(|g| g != current) {
        return false;
    }
    config.apply_to(ui);
    true
}

/// Replace the active profile's theme choice without touching the rest
/// of the UI config. The main window's palette picks a theme while the
/// Settings window may hold its own full snapshot, so a whole config
/// write from one would overwrite the other's newer fields. The caller
/// applies and broadcasts the theme itself. While follow system
/// appearance is on, a pick fills the light or dark slot instead, so the
/// caller also sends the pair.
#[tauri::command]
pub(crate) async fn ui_set_theme(
    state: State<'_, SharedState>,
    theme: String,
    light_theme: Option<String>,
    dark_theme: Option<String>,
) -> Result<(), String> {
    let open = {
        let mut p = state.selected_session().lock_profile().await;
        if !apply_theme_pick(&mut p.ui, theme, light_theme, dark_theme) {
            return Ok(());
        }
        p.open().clone()
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&shared, &open).await;
    Ok(())
}

/// Write a theme pick onto the live UI config. A missing or blank pair
/// entry leaves that slot alone. Returns whether anything changed, so an
/// unchanged pick skips the save.
fn apply_theme_pick(
    ui: &mut crate::profile::ui::UiConfig,
    theme: String,
    light_theme: Option<String>,
    dark_theme: Option<String>,
) -> bool {
    let mut changed = false;
    let mut set = |slot: &mut String, value: String| {
        if !value.is_empty() && *slot != value {
            *slot = value;
            changed = true;
        }
    };
    set(&mut ui.theme, theme);
    if let Some(v) = light_theme {
        set(&mut ui.light_theme, v);
    }
    if let Some(v) = dark_theme {
        set(&mut ui.dark_theme, v);
    }
    changed
}

/// Every system font family, for the font picker.
///
/// Async so it never runs on the main thread. Tauri runs a sync
/// command there on macOS, and the first enumeration of a launch took
/// 2 to 3 s, which froze every window until it finished.
#[tauri::command]
pub(crate) async fn fonts_list() -> Vec<FontEntry> {
    crate::app::system_fonts::list().await
}

/// The 16 ANSI slots a chat channel can take, in the frontend's names.
const CHAT_COLOR_SLOTS: [&str; 16] = [
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "white",
    "brightBlack",
    "brightRed",
    "brightGreen",
    "brightYellow",
    "brightBlue",
    "brightMagenta",
    "brightCyan",
    "brightWhite",
];

/// The chat pane's channel colors for the live profile.
#[tauri::command]
pub(crate) async fn ui_get_chat_colors(
    state: State<'_, SharedState>,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    let p = state.selected_session().lock_profile().await;
    Ok(p.ui.chat_colors.clone())
}

/// Recolor one chat channel from the pane menu, or give it back its
/// default with no color. Like the affects display picks, it touches
/// nothing else in the UI config. Nothing is saved or sent when the
/// pick changes nothing.
#[tauri::command]
pub(crate) async fn ui_set_chat_color(
    app: AppHandle,
    state: State<'_, SharedState>,
    channel: String,
    color: Option<String>,
) -> Result<(), String> {
    let (open, changed) = {
        let mut p = state.selected_session().lock_profile().await;
        let changed = apply_chat_color(&mut p.ui, channel, color);
        (p.open().clone(), changed)
    };
    send_chat_colors(&app, state.inner(), &open, changed).await;
    Ok(())
}

/// Give every chat channel its default color again.
#[tauri::command]
pub(crate) async fn ui_reset_chat_colors(
    app: AppHandle,
    state: State<'_, SharedState>,
) -> Result<(), String> {
    let (open, changed) = {
        let mut p = state.selected_session().lock_profile().await;
        let changed = reset_chat_colors(&mut p.ui);
        (p.open().clone(), changed)
    };
    send_chat_colors(&app, state.inner(), &open, changed).await;
    Ok(())
}

/// Save `open` and tell every window, when a chat color moved.
async fn send_chat_colors(
    app: &AppHandle,
    state: &SharedState,
    open: &Arc<OpenProfile>,
    changed: Option<std::collections::BTreeMap<String, String>>,
) {
    let Some(colors) = changed else {
        return;
    };
    save_then_broadcast(
        app,
        state,
        open,
        SavePolicy::Now,
        CHAT_COLORS_CHANGED,
        &colors,
    )
    .await;
}

/// Write a chat color pick onto the live UI config. The channel matches
/// in lowercase. A color that is not one of the 16 slots clears the
/// channel back to its default. Returns the new table when anything
/// changed.
fn apply_chat_color(
    ui: &mut crate::profile::ui::UiConfig,
    channel: String,
    color: Option<String>,
) -> Option<std::collections::BTreeMap<String, String>> {
    let channel = channel.trim().to_lowercase();
    if channel.is_empty() {
        return None;
    }
    let color = color.filter(|c| CHAT_COLOR_SLOTS.contains(&c.as_str()));
    let changed = match color {
        Some(color) => ui.chat_colors.insert(channel, color.clone()).as_ref() != Some(&color),
        None => ui.chat_colors.remove(&channel).is_some(),
    };
    changed.then(|| ui.chat_colors.clone())
}

/// Clear every chat color. Returns the empty table when there was any.
fn reset_chat_colors(
    ui: &mut crate::profile::ui::UiConfig,
) -> Option<std::collections::BTreeMap<String, String>> {
    if ui.chat_colors.is_empty() {
        return None;
    }
    ui.chat_colors.clear();
    Some(std::collections::BTreeMap::new())
}

#[cfg(test)]
mod tests {
    use super::UiConfigPayload;
    use crate::profile::file::ProfileConfig;
    use crate::profile::ui::UiConfig;
    use crate::prompt::tests::prompt_profile;

    /// Send `ui` the way Settings does: out through `ui_get_config`,
    /// across the JSON bridge, and back through `ui_set_config` onto a
    /// fresh config.
    fn through_payload(ui: &UiConfig) -> UiConfig {
        let json = serde_json::to_string(&UiConfigPayload::from_ui(ui)).unwrap();
        let payload: UiConfigPayload = serde_json::from_str(&json).unwrap();
        let mut out = UiConfig::default();
        payload.apply_to(&mut out);
        out
    }

    #[test]
    fn ui_defaults_match_the_ones_the_page_fills() {
        // normalizeUiConfig in src/lib/session.ts reads the same file and
        // fills a missing field with each of these values.
        let text = include_str!("../../../fixtures/ui-config/defaults.json");
        let fixture: serde_json::Value = serde_json::from_str(text).unwrap();
        let passed: Vec<&str> = fixture["passed_through"]
            .as_array()
            .unwrap()
            .iter()
            .map(|key| key.as_str().unwrap())
            .collect();
        // No [ui] table, an empty one, and an empty vitals table each take
        // their defaults by another path.
        for (from, ui) in [
            ("no [ui]", ProfileConfig::from_toml("").unwrap().ui),
            ("[ui]", ProfileConfig::from_toml("[ui]\n").unwrap().ui),
            (
                "[ui.vitals]",
                ProfileConfig::from_toml("[ui.vitals]\n").unwrap().ui,
            ),
            ("UiConfig::default", UiConfig::default()),
        ] {
            let mut sent = serde_json::to_value(UiConfigPayload::from_ui(&ui)).unwrap();
            let fields = sent.as_object_mut().unwrap();
            for key in &passed {
                assert!(fields.remove(*key).is_some(), "{from}: {key}");
            }
            assert_eq!(sent, fixture["defaults"], "{from}");
        }
    }

    #[test]
    fn blinking_text_keeps_your_choice_and_none_until_you_make_one() {
        // None is no choice, which the page reads from the system's reduce
        // motion setting.
        let mut ui = UiConfig::default();
        assert_eq!(through_payload(&ui).blink_text, None);
        for choice in [true, false] {
            ui.blink_text = Some(choice);
            assert_eq!(through_payload(&ui).blink_text, Some(choice));
        }
    }

    /// A whole config save read at `generation`, holding `ui`.
    fn save_of(ui: &UiConfig, generation: Option<u64>) -> UiConfigPayload {
        let mut payload = UiConfigPayload::from_ui(ui);
        payload.generation = generation;
        payload
    }

    #[test]
    fn a_save_read_before_a_replace_leaves_the_new_profile_alone() {
        // The loaded profile counts up with the value alone.
        let mut live = UiConfig {
            tick_count: "up".into(),
            chip_style: "value_only".into(),
            ..UiConfig::default()
        };
        // Settings read the old profile at generation 3, and you moved
        // the font size after #profile load took it to 4.
        let old = UiConfig {
            tick_count: "down".into(),
            chip_style: "icon_value".into(),
            font_size: 16,
            ..UiConfig::default()
        };
        assert!(!super::apply_ui_config(
            &mut live,
            save_of(&old, Some(3)),
            4
        ));
        assert_eq!(live.tick_count, "up");
        assert_eq!(live.chip_style, "value_only");
        assert_eq!(live.font_size, UiConfig::default().font_size);
    }

    #[test]
    fn a_save_read_since_the_last_replace_applies() {
        let mut live = UiConfig::default();
        let edited = UiConfig {
            font_size: 16,
            tick_count: "up".into(),
            ..UiConfig::default()
        };
        assert!(super::apply_ui_config(
            &mut live,
            save_of(&edited, Some(4)),
            4
        ));
        assert_eq!(live.font_size, 16);
        assert_eq!(live.tick_count, "up");
        // A config that never came from the backend carries none.
        let mut live = UiConfig::default();
        assert!(super::apply_ui_config(&mut live, save_of(&edited, None), 4));
        assert_eq!(live.font_size, 16);
    }

    #[test]
    fn a_settings_payload_that_leaves_fields_out_still_reads() {
        let mut json = serde_json::to_value(UiConfigPayload::default()).unwrap();
        let fields = json.as_object_mut().unwrap();
        fields.remove("font_size");
        fields.remove("theme");
        fields.insert("font_family".into(), "Iosevka".into());
        let payload: UiConfigPayload = serde_json::from_value(json).unwrap();
        let defaults = crate::profile::ui::UiConfig::default();
        assert_eq!(payload.font_family, "Iosevka");
        assert_eq!(payload.font_size, defaults.font_size);
        assert_eq!(payload.theme, defaults.theme);
        let empty: UiConfigPayload = serde_json::from_str("{}").unwrap();
        assert_eq!(empty.font_size, defaults.font_size);
    }

    #[test]
    fn the_settings_payload_carries_nothing_of_your_prompt() {
        let (p, _) = prompt_profile();
        let json = serde_json::to_value(super::ui_config_of(&p, 5)).unwrap();
        let keys = json.as_object().unwrap();
        for key in ["prompt_template_enabled", "prompt_template", "prompt_show"] {
            assert!(!keys.contains_key(key), "{key} reaches Settings");
        }
        assert_eq!(json["generation"], 5);
    }

    #[test]
    fn a_settings_save_leaves_the_prompt_table_and_its_copy_alone() {
        let (mut p, mut c) = prompt_profile();
        let mut config = c.prompt.config().clone();
        config.show = vosh_prompt::PromptShow::Pinned;
        crate::prompt::take_config(&mut p, &mut c, config);
        let table = p.prompt.clone();
        let mut save = super::ui_config_of(&p, 4);
        save.font_size = 16;
        assert!(super::apply_ui_config(&mut p.ui, save, 4));
        assert_eq!(p.ui.font_size, 16);
        assert_eq!(p.prompt, table);

        // A window from before the prompt section still sends the three
        // fields. They are read past and change nothing.
        let mut json = serde_json::to_value(super::ui_config_of(&p, 4)).unwrap();
        let fields = json.as_object_mut().unwrap();
        fields.insert("prompt_template_enabled".into(), false.into());
        fields.insert("prompt_template".into(), "stale".into());
        fields.insert("prompt_show".into(), "text".into());
        let old: UiConfigPayload = serde_json::from_value(json).unwrap();
        assert!(super::apply_ui_config(&mut p.ui, old, 4));
        assert_eq!(p.prompt, table);

        // The file keeps the table, and [ui] its copy of the switch and
        // the design for an older build.
        let file = crate::profile::file::ProfileConfig::from_profile(&p);
        assert_eq!(file.prompt_config(), table);
        assert!(file.ui.prompt_template_enabled);
        assert_eq!(file.ui.prompt_template, "%hp");
    }

    #[test]
    fn the_generation_travels_with_the_config_but_stays_optional() {
        let json = serde_json::to_value(save_of(&UiConfig::default(), Some(7))).unwrap();
        assert_eq!(json["generation"], serde_json::json!(7));
        let json = serde_json::to_value(save_of(&UiConfig::default(), None)).unwrap();
        assert!(json.get("generation").is_none());
        let back: UiConfigPayload = serde_json::from_value(json).unwrap();
        assert_eq!(back.generation, None);
    }

    #[test]
    fn follow_system_appearance_round_trips() {
        let ui = UiConfig {
            follow_system_appearance: true,
            ..UiConfig::default()
        };
        assert!(through_payload(&ui).follow_system_appearance);
        assert!(!through_payload(&UiConfig::default()).follow_system_appearance);
    }

    #[test]
    fn light_theme_round_trips() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.light_theme, "vellum");
        ui.light_theme = "classic-vivid".into();
        assert_eq!(through_payload(&ui).light_theme, "classic-vivid");
        // A blank pick saves as the default light theme.
        ui.light_theme = "  ".into();
        assert_eq!(through_payload(&ui).light_theme, "vellum");
    }

    #[test]
    fn dark_theme_round_trips() {
        let mut ui = UiConfig::default();
        // Unset until the first save, so the frontend can seed it from
        // the current theme.
        assert_eq!(ui.dark_theme, "");
        assert_eq!(through_payload(&ui).dark_theme, "");
        ui.dark_theme = "nord".into();
        assert_eq!(through_payload(&ui).dark_theme, "nord");
    }

    #[test]
    fn terminal_line_height_round_trips() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.terminal_line_height, "default");
        for id in ["compact", "default", "loose"] {
            ui.terminal_line_height = id.into();
            assert_eq!(through_payload(&ui).terminal_line_height, id);
        }
        ui.terminal_line_height = "roomy".into();
        assert_eq!(through_payload(&ui).terminal_line_height, "default");
    }

    #[test]
    fn the_panel_font_round_trips_and_a_page_without_it_keeps_as_designed() {
        let mut ui = UiConfig::default();
        assert_eq!(through_payload(&ui).panel_font, "");
        for pick in ["terminal", "system", "\"Iosevka\", Menlo, monospace"] {
            ui.panel_font = pick.into();
            assert_eq!(through_payload(&ui).panel_font, pick);
        }
        ui.panel_font = " SYSTEM ".into();
        assert_eq!(through_payload(&ui).panel_font, "system");
        ui.panel_font = " Terminal ".into();
        assert_eq!(through_payload(&ui).panel_font, "terminal");
        // A page from before the row sends no panel font, which reads as
        // As designed.
        let payload: UiConfigPayload = serde_json::from_str("{\"font_size\": 16}").unwrap();
        let mut out = UiConfig::default();
        payload.apply_to(&mut out);
        assert_eq!(out.panel_font, "");
        assert_eq!(out.font_size, 16);
    }

    #[test]
    fn the_panel_size_round_trips_and_a_page_without_it_keeps_12() {
        let mut ui = UiConfig::default();
        assert_eq!(through_payload(&ui).panel_font_size, 12);
        for pick in [0, 11, 16] {
            ui.panel_font_size = pick;
            assert_eq!(through_payload(&ui).panel_font_size, pick);
        }
        ui.panel_font_size = 200;
        assert_eq!(through_payload(&ui).panel_font_size, 64);
        // A page from before the row sends no panel size, which reads as
        // 12, the size the panes drew at, and never as the terminal size.
        let payload: UiConfigPayload = serde_json::from_str("{\"font_size\": 16}").unwrap();
        let mut out = UiConfig::default();
        payload.apply_to(&mut out);
        assert_eq!(out.panel_font_size, 12);
        assert_eq!(out.font_size, 16);
    }

    #[test]
    fn a_theme_pick_writes_only_what_it_names() {
        let mut ui = UiConfig::default();
        assert!(super::apply_theme_pick(&mut ui, "nord".into(), None, None));
        assert_eq!(ui.theme, "nord");
        assert_eq!(ui.light_theme, "vellum");
        assert_eq!(ui.dark_theme, "");

        // A pick while following the system fills the dark slot.
        assert!(super::apply_theme_pick(
            &mut ui,
            "nord".into(),
            Some("vellum".into()),
            Some("tokyo-night".into()),
        ));
        assert_eq!(ui.theme, "nord");
        assert_eq!(ui.dark_theme, "tokyo-night");

        // The same pick again changes nothing, and a blank slot is left alone.
        assert!(!super::apply_theme_pick(
            &mut ui,
            "nord".into(),
            Some(String::new()),
            Some("tokyo-night".into()),
        ));
        assert_eq!(ui.light_theme, "vellum");
    }

    #[test]
    fn vitals_density_round_trips() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.vitals_density, "rows");
        for id in ["rows", "line"] {
            ui.vitals_density = id.into();
            assert_eq!(through_payload(&ui).vitals_density, id);
        }
        // An unknown density saves as rows.
        ui.vitals_density = "grid".into();
        assert_eq!(through_payload(&ui).vitals_density, "rows");
    }

    #[test]
    fn vitals_values_round_trips() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.vitals_values, "current-max");
        for id in ["current-max", "current", "percent"] {
            ui.vitals_values = id.into();
            assert_eq!(through_payload(&ui).vitals_values, id);
        }
        // An unknown form saves as current and max.
        ui.vitals_values = "both".into();
        assert_eq!(through_payload(&ui).vitals_values, "current-max");
    }

    #[test]
    fn vitals_meter_round_trips() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.vitals_meter, "line");
        for id in ["line", "bar", "none"] {
            ui.vitals_meter = id.into();
            assert_eq!(through_payload(&ui).vitals_meter, id);
        }
        // An unknown meter saves as the line.
        ui.vitals_meter = "gauge".into();
        assert_eq!(through_payload(&ui).vitals_meter, "line");
    }

    #[test]
    fn vitals_warn_thirds_round_trips() {
        let mut ui = UiConfig::default();
        assert!(!ui.vitals_warn_thirds);
        assert!(!through_payload(&ui).vitals_warn_thirds);
        ui.vitals_warn_thirds = true;
        assert!(through_payload(&ui).vitals_warn_thirds);
    }

    #[test]
    fn fit_game_colors_round_trips() {
        let mut ui = UiConfig::default();
        assert!(ui.fit_game_colors);
        assert!(through_payload(&ui).fit_game_colors);
        ui.fit_game_colors = false;
        assert!(!through_payload(&ui).fit_game_colors);
    }

    #[test]
    fn color_vision_round_trips_and_coerces_an_unknown_one() {
        let mut ui = UiConfig::default();
        assert_eq!(through_payload(&ui).color_vision, "typical");
        for vision in crate::profile::ui::COLOR_VISIONS {
            ui.color_vision = vision.into();
            assert_eq!(through_payload(&ui).color_vision, vision);
        }
        ui.color_vision = "deutan".into();
        assert_eq!(through_payload(&ui).color_vision, "typical");
    }

    #[test]
    fn readable_highlights_round_trips() {
        let mut ui = UiConfig::default();
        assert!(ui.readable_highlights);
        assert!(through_payload(&ui).readable_highlights);
        ui.readable_highlights = false;
        assert!(!through_payload(&ui).readable_highlights);
    }

    #[test]
    fn collapse_repeats_round_trips() {
        let mut ui = UiConfig::default();
        assert!(!ui.collapse_repeats);
        assert!(!through_payload(&ui).collapse_repeats);
        ui.collapse_repeats = true;
        assert!(through_payload(&ui).collapse_repeats);
    }

    #[test]
    fn collapse_fight_and_attack_lines_round_trip() {
        let mut ui = UiConfig::default();
        assert!(ui.collapse_fight_lines);
        assert!(!ui.collapse_attack_lines);
        let back = through_payload(&ui);
        assert!(back.collapse_fight_lines);
        assert!(!back.collapse_attack_lines);
        ui.collapse_fight_lines = false;
        ui.collapse_attack_lines = true;
        let back = through_payload(&ui);
        assert!(!back.collapse_fight_lines);
        assert!(back.collapse_attack_lines);
        // A page from before the two choices saves without them, and the
        // profile takes their defaults.
        let payload: UiConfigPayload =
            serde_json::from_str(r#"{"collapse_repeats":true}"#).unwrap();
        let mut out = UiConfig {
            collapse_fight_lines: false,
            collapse_attack_lines: true,
            ..UiConfig::default()
        };
        payload.apply_to(&mut out);
        assert!(out.collapse_repeats);
        assert!(out.collapse_fight_lines);
        assert!(!out.collapse_attack_lines);
    }

    #[test]
    fn vitals_hide_when_pinned_round_trips() {
        let mut ui = UiConfig::default();
        assert!(ui.vitals_hide_when_pinned);
        assert!(through_payload(&ui).vitals_hide_when_pinned);
        ui.vitals_hide_when_pinned = false;
        assert!(!through_payload(&ui).vitals_hide_when_pinned);
    }

    #[test]
    fn tick_count_round_trips() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.tick_count, "up");
        for id in ["up", "down", "down_past_zero"] {
            ui.tick_count = id.into();
            assert_eq!(through_payload(&ui).tick_count, id);
        }
        // An unknown direction saves as counting up.
        ui.tick_count = "sideways".into();
        assert_eq!(through_payload(&ui).tick_count, "up");
    }

    #[test]
    fn game_time_round_trips() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.game_time, "24h");
        for clock in ["24h", "12h"] {
            ui.game_time = clock.into();
            assert_eq!(through_payload(&ui).game_time, clock);
        }
        // An unknown clock saves as the 24 hour one.
        ui.game_time = "noon".into();
        assert_eq!(through_payload(&ui).game_time, "24h");
    }

    #[test]
    fn affects_style_round_trips() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.affects_style, "timers");
        for id in ["timers", "countdown", "chips", "chips_drain"] {
            ui.affects_style = id.into();
            assert_eq!(through_payload(&ui).affects_style, id);
        }
        // An unknown layout saves as Timers first.
        ui.affects_style = "grid".into();
        assert_eq!(through_payload(&ui).affects_style, "timers");
    }

    #[test]
    fn affects_marker_round_trips() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.affects_marker, "dot");
        for id in ["dot", "square", "plus_minus", "none"] {
            ui.affects_marker = id.into();
            assert_eq!(through_payload(&ui).affects_marker, id);
        }
        // An unknown mark saves as the dot.
        ui.affects_marker = "check".into();
        assert_eq!(through_payload(&ui).affects_marker, "dot");
    }

    #[test]
    fn affects_tint_round_trips() {
        let mut ui = UiConfig::default();
        assert!(!ui.affects_tint);
        assert!(!through_payload(&ui).affects_tint);
        ui.affects_tint = true;
        assert!(through_payload(&ui).affects_tint);
    }

    #[test]
    fn affects_thresholds_round_trip() {
        let mut ui = UiConfig::default();
        assert_eq!(ui.affects_running_out_hours, 2);
        assert_eq!(ui.affects_almost_gone_hours, 1);
        for (running_out, almost_gone) in [(2, 1), (5, 2), (0, 0), (3, 3), (99, 0), (99, 99)] {
            ui.affects_running_out_hours = running_out;
            ui.affects_almost_gone_hours = almost_gone;
            let read = through_payload(&ui);
            assert_eq!(read.affects_running_out_hours, running_out);
            assert_eq!(read.affects_almost_gone_hours, almost_gone);
        }
    }

    #[test]
    fn a_save_holds_the_affects_thresholds_to_whole_hours_in_order() {
        // Almost gone never goes over running out, which wins.
        let mut ui = UiConfig {
            affects_running_out_hours: 3,
            affects_almost_gone_hours: 7,
            ..UiConfig::default()
        };
        let saved = through_payload(&ui);
        assert_eq!(saved.affects_running_out_hours, 3);
        assert_eq!(saved.affects_almost_gone_hours, 3);
        // Neither goes past 99.
        ui.affects_running_out_hours = 500;
        ui.affects_almost_gone_hours = 120;
        let saved = through_payload(&ui);
        assert_eq!(saved.affects_running_out_hours, 99);
        assert_eq!(saved.affects_almost_gone_hours, 99);
    }

    #[test]
    fn a_page_that_sends_odd_affects_thresholds_still_saves() {
        let read = |json: &str| {
            let payload: UiConfigPayload = serde_json::from_str(json).unwrap();
            let mut ui = UiConfig::default();
            payload.apply_to(&mut ui);
            (ui.affects_running_out_hours, ui.affects_almost_gone_hours)
        };
        assert_eq!(read("{}"), (2, 1));
        assert_eq!(
            read(r#"{"affects_running_out_hours": 4.6, "affects_almost_gone_hours": -3}"#),
            (5, 0)
        );
        assert_eq!(
            read(r#"{"affects_running_out_hours": "6", "affects_almost_gone_hours": null}"#),
            (6, 1)
        );
    }

    #[test]
    fn chat_colors_stay_with_each_character_and_out_of_the_whole_config_save() {
        let mut ui = UiConfig::default();
        ui.chat_colors.insert("say".into(), "brightBlue".into());
        // A whole config save from Settings carries no chat colors, so it
        // never writes an old copy back over a pick from the pane menu.
        let json = serde_json::to_value(UiConfigPayload::from_ui(&ui)).unwrap();
        assert!(json.get("chat_colors").is_none());
        let mut live = ui.clone();
        UiConfigPayload::from_ui(&UiConfig::default()).apply_to(&mut live);
        assert_eq!(
            live.chat_colors.get("say").map(String::as_str),
            Some("brightBlue")
        );
    }

    #[test]
    fn a_chat_color_pick_writes_only_its_channel() {
        let mut ui = UiConfig::default();
        let colors = super::apply_chat_color(&mut ui, " Say ".into(), Some("brightBlue".into()))
            .expect("a new color changes the table");
        assert_eq!(colors.get("say").map(String::as_str), Some("brightBlue"));
        assert_eq!(colors.len(), 1);

        // The same pick again changes nothing, so nothing is saved or sent.
        assert_eq!(
            super::apply_chat_color(&mut ui, "say".into(), Some("brightBlue".into())),
            None
        );

        let colors = super::apply_chat_color(&mut ui, "tell".into(), Some("red".into()))
            .expect("another channel joins");
        assert_eq!(colors.len(), 2);

        // Default, or a color that is not one of the 16, clears the channel.
        let colors = super::apply_chat_color(&mut ui, "say".into(), None)
            .expect("default clears the channel");
        assert!(!colors.contains_key("say"));
        let colors = super::apply_chat_color(&mut ui, "tell".into(), Some("sparkle".into()))
            .expect("an unknown color clears the channel");
        assert!(colors.is_empty());
        assert_eq!(super::apply_chat_color(&mut ui, "tell".into(), None), None);

        // A blank channel names nothing.
        assert_eq!(
            super::apply_chat_color(&mut ui, "  ".into(), Some("red".into())),
            None
        );
    }

    #[test]
    fn reset_all_clears_every_chat_color_once() {
        let mut ui = UiConfig::default();
        assert_eq!(super::reset_chat_colors(&mut ui), None);
        ui.chat_colors.insert("say".into(), "blue".into());
        ui.chat_colors.insert("yell".into(), "red".into());
        assert_eq!(
            super::reset_chat_colors(&mut ui),
            Some(std::collections::BTreeMap::new())
        );
        assert!(ui.chat_colors.is_empty());
    }
}
