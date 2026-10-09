//! The commands behind the Settings config. Settings reads the UI config
//! through them, saves only the fields it names, and lists the system
//! fonts for the font picker. The main window's palette saves a theme
//! pick, and the chat pane's menu its channel colors, without the rest of
//! the config. Each command that reads or changes a profile takes the
//! `profile` it means, which a session must play, and acts on the
//! selected session's when it names none.

use std::sync::Arc;

use tauri::{AppHandle, State};
use vosh_prompt::vitals::VitalsText;

use crate::app::events::{self, CHAT_COLORS_CHANGED};
use crate::app::state::SharedState;
use crate::app::system_fonts::FontEntry;
use crate::disk::save::{persist_profile, save_then_broadcast, SavePolicy};
use crate::profile::open::OpenProfile;
use crate::session::vitals_text;
use crate::sessions::SessionId;

/// The UI config as `ui_get_config` hands it to the page.
#[derive(serde::Serialize)]
pub(crate) struct UiConfigPayload {
    pub theme: String,
    pub follow_system_appearance: bool,
    pub light_theme: String,
    pub dark_theme: String,
    /// `off`, `system` or `game`.
    pub theme_follow: String,
    pub day_theme: String,
    pub night_theme: String,
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
    pub blink_text: Option<bool>,
    pub fit_game_colors: bool,
    /// `typical`, `deuteranopia`, `protanopia` or `tritanopia`.
    pub color_vision: String,
    pub readable_highlights: bool,
    pub screen_reader: bool,
    pub screen_reader_background: bool,
    pub screen_reader_prompt: bool,
    /// 4, 8, 16 or 32.
    pub screen_reader_burst: u32,
    pub collapse_repeats: bool,
    pub collapse_fight_lines: bool,
    pub collapse_attack_lines: bool,
    pub terminal_base_ansi: Option<Vec<String>>,
    pub custom_themes: Vec<crate::profile::ui::CustomTheme>,
    pub split_divider_color: Option<String>,
    pub input_echo_color: Option<String>,
    pub echo_macros: bool,
    /// `off`, `chevron`, `gt` or `own`.
    pub input_echo_mark: String,
    pub input_echo_mark_text: String,
    pub input_echo_mark_color: Option<String>,
    pub input_echo_dim: bool,
    pub input_line_mark: bool,
    pub paste_line_delay_ms: u32,
    pub spellcheck_prompt: bool,
    pub writing_offer: bool,
    pub writing_ask_post: bool,
    pub input_cursor_style: String,
    pub input_caret_blink: bool,
    pub input_caret_color: Option<String>,
    pub input_line_color: Option<String>,
    /// `theme`, `tint` or `own`.
    pub input_line_background: String,
    pub input_line_background_color: Option<String>,
    /// 0 follows the terminal size.
    pub input_line_size: u32,
    pub input_type_colors: bool,
    pub input_type_alias_color: Option<String>,
    pub input_type_hash_color: Option<String>,
    pub input_type_chat_color: Option<String>,
    pub input_type_unknown_color: Option<String>,
    pub vitals_density: String,
    pub vitals_values: String,
    pub vitals_meter: String,
    pub vitals_warn_thirds: bool,
    pub vitals_hide_when_pinned: bool,
    /// One of the twelve styles in `VITALS_STYLES`, or None for Rows and
    /// One line, which `vitals_density` holds.
    pub vitals_style: Option<String>,
    /// `panel` or `status`.
    pub vitals_place: String,
    /// `hp`, `mana` and `move`, each once.
    pub vitals_order: Vec<String>,
    /// The vitals you turned off, and `opponent` for your opponent's row.
    pub vitals_off: Vec<String>,
    /// `top` or `bottom`.
    pub vitals_opponent: String,
    /// Each vital's ANSI slot, 0 to 15. A vital left out takes Default.
    pub vitals_colors: std::collections::BTreeMap<String, u8>,
    pub vitals_text: String,
    /// At most two earlier texts, newest first.
    pub vitals_text_previous: Vec<String>,
    /// Show each hit.
    pub vitals_hit: bool,
    /// The style your 0.7 vitals grew into, `text`, `gauges`, `pips`,
    /// `line` or `rows`, which the gallery marks Yours in 0.7. Left out
    /// when they give no clue. Read only, nothing saves it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vitals_legacy_style: Option<&'static str>,
    /// Your 0.7 template in today's codes, while it was on. Read only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vitals_legacy_text: Option<String>,
    pub chip_style: String,
    pub tick_count: String,
    pub game_time: String,
    pub affects_style: String,
    pub affects_marker: String,
    pub affects_tint: bool,
    pub affects_running_out_hours: u32,
    pub affects_almost_gone_hours: u32,
    /// The share of the terminal column the snoop split takes.
    pub snoop_share: f64,
    pub snoop_folded: bool,
    pub log_sessions: Option<bool>,
    pub scrollback_lines: u32,
    pub writing_card_left: Option<f64>,
    pub writing_card_top: Option<f64>,
    pub writing_card_rows: Option<u32>,
    pub writing_card_cols: Option<u32>,
    pub writing_card_pinned: bool,
}

impl UiConfigPayload {
    /// The snapshot `ui_get_config` hands the frontend.
    pub(crate) fn from_ui(ui: &crate::profile::ui::UiConfig) -> Self {
        Self {
            theme: ui.theme.clone(),
            follow_system_appearance: ui.follow_system_appearance,
            light_theme: ui.light_theme.clone(),
            dark_theme: ui.dark_theme.clone(),
            theme_follow: ui.theme_follow.clone(),
            day_theme: ui.day_theme.clone(),
            night_theme: ui.night_theme.clone(),
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
            screen_reader: ui.screen_reader,
            screen_reader_background: ui.screen_reader_background,
            screen_reader_prompt: ui.screen_reader_prompt,
            screen_reader_burst: ui.screen_reader_burst,
            collapse_repeats: ui.collapse_repeats,
            collapse_fight_lines: ui.collapse_fight_lines,
            collapse_attack_lines: ui.collapse_attack_lines,
            terminal_base_ansi: ui.terminal_base_ansi.clone(),
            custom_themes: ui.custom_themes.clone(),
            split_divider_color: ui.split_divider_color.clone(),
            input_echo_color: ui.input_echo_color.clone(),
            echo_macros: ui.echo_macros,
            input_echo_mark: ui.input_echo_mark.clone(),
            input_echo_mark_text: ui.input_echo_mark_text.clone(),
            input_echo_mark_color: ui.input_echo_mark_color.clone(),
            input_echo_dim: ui.input_echo_dim,
            input_line_mark: ui.input_line_mark,
            paste_line_delay_ms: ui.paste_line_delay_ms,
            spellcheck_prompt: ui.spellcheck_prompt,
            writing_offer: ui.writing_offer,
            writing_ask_post: ui.writing_ask_post,
            input_cursor_style: ui.input_cursor_style.clone(),
            input_caret_blink: ui.input_caret_blink,
            input_caret_color: ui.input_caret_color.clone(),
            input_line_color: ui.input_line_color.clone(),
            input_line_background: ui.input_line_background.clone(),
            input_line_background_color: ui.input_line_background_color.clone(),
            input_line_size: ui.input_line_size,
            input_type_colors: ui.input_type_colors,
            input_type_alias_color: ui.input_type_alias_color.clone(),
            input_type_hash_color: ui.input_type_hash_color.clone(),
            input_type_chat_color: ui.input_type_chat_color.clone(),
            input_type_unknown_color: ui.input_type_unknown_color.clone(),
            vitals_density: ui.vitals_density.clone(),
            vitals_values: ui.vitals_values.clone(),
            vitals_meter: ui.vitals_meter.clone(),
            vitals_warn_thirds: ui.vitals_warn_thirds,
            vitals_hide_when_pinned: ui.vitals_hide_when_pinned,
            vitals_style: ui.vitals_style.clone(),
            vitals_place: ui.vitals_place.clone(),
            vitals_order: ui.vitals_order.clone(),
            vitals_off: ui.vitals_off.clone(),
            vitals_opponent: ui.vitals_opponent.clone(),
            vitals_colors: ui.vitals_colors.clone(),
            vitals_text: ui.vitals_text.clone(),
            vitals_text_previous: ui.vitals_text_previous.clone(),
            vitals_hit: ui.vitals_hit,
            vitals_legacy_style: ui.vitals.legacy_style(),
            vitals_legacy_text: ui.vitals.legacy_text(),
            chip_style: ui.chip_style.clone(),
            tick_count: ui.tick_count.clone(),
            game_time: ui.game_time.clone(),
            affects_style: ui.affects_style.clone(),
            affects_marker: ui.affects_marker.clone(),
            affects_tint: ui.affects_tint,
            affects_running_out_hours: ui.affects_running_out_hours,
            affects_almost_gone_hours: ui.affects_almost_gone_hours,
            snoop_share: ui.snoop_share,
            snoop_folded: ui.snoop_folded,
            log_sessions: ui.log_sessions,
            scrollback_lines: ui.scrollback_lines,
            writing_card_left: ui.writing_card_left,
            writing_card_top: ui.writing_card_top,
            writing_card_rows: ui.writing_card_rows,
            writing_card_cols: ui.writing_card_cols,
            writing_card_pinned: ui.writing_card_pinned,
        }
    }
}

/// The live UI config. Your prompt is not in it. The prompt section
/// reads and writes the `[prompt]` table through the prompt commands, and
/// `[ui]` keeps only a copy of its switch and design for an older build.
#[tauri::command]
pub(crate) async fn ui_get_config(
    state: State<'_, SharedState>,
    profile: Option<String>,
) -> Result<UiConfigPayload, String> {
    let p = state.lock_named(profile).await?;
    Ok(UiConfigPayload::from_ui(&p.ui))
}

/// One field of the UI config with its new value, as the page sends it,
/// `{"field": <name>, "value": ...}`. Each variant is the setter for the
/// field of [`UiConfigPayload`] it names, in the payload's order.
/// `tracked_affects` saves through `tracked_affects_set` instead.
#[derive(serde::Deserialize)]
#[serde(tag = "field", content = "value", rename_all = "snake_case")]
pub(crate) enum UiField {
    Theme(String),
    FollowSystemAppearance(bool),
    LightTheme(String),
    DarkTheme(String),
    /// Follow system appearance stays true only for `system`.
    ThemeFollow(String),
    DayTheme(String),
    NightTheme(String),
    AutoUpdate(bool),
    FontFamily(String),
    FontSize(u32),
    TerminalLineHeight(String),
    PanelFont(String),
    PanelFontSize(u32),
    EnabledPresets(Vec<String>),
    KeepLastCommand(bool),
    ThemeTerminalColors(Option<bool>),
    BrightBold(bool),
    BlinkText(Option<bool>),
    FitGameColors(bool),
    ColorVision(String),
    ReadableHighlights(bool),
    ScreenReader(bool),
    ScreenReaderBackground(bool),
    ScreenReaderPrompt(bool),
    ScreenReaderBurst(u32),
    CollapseRepeats(bool),
    CollapseFightLines(bool),
    CollapseAttackLines(bool),
    TerminalBaseAnsi(Option<Vec<String>>),
    CustomThemes(Vec<crate::profile::ui::CustomTheme>),
    SplitDividerColor(Option<String>),
    InputEchoColor(Option<String>),
    EchoMacros(bool),
    /// Mark before your commands keeps `input_echo_caret` in step for an older
    /// build.
    InputEchoMark(String),
    InputEchoMarkText(String),
    InputEchoMarkColor(Option<String>),
    InputEchoDim(bool),
    InputLineMark(bool),
    PasteLineDelayMs(u32),
    SpellcheckPrompt(bool),
    WritingOffer(bool),
    WritingAskPost(bool),
    InputCursorStyle(String),
    InputCaretBlink(bool),
    InputCaretColor(Option<String>),
    InputLineColor(Option<String>),
    InputLineBackground(String),
    InputLineBackgroundColor(Option<String>),
    InputLineSize(u32),
    InputTypeColors(bool),
    InputTypeAliasColor(Option<String>),
    InputTypeHashColor(Option<String>),
    InputTypeChatColor(Option<String>),
    InputTypeUnknownColor(Option<String>),
    VitalsDensity(String),
    VitalsValues(String),
    VitalsMeter(String),
    VitalsWarnThirds(bool),
    VitalsHideWhenPinned(bool),
    VitalsStyle(Option<String>),
    VitalsPlace(String),
    VitalsOrder(Vec<String>),
    VitalsOff(Vec<String>),
    VitalsOpponent(String),
    #[serde(deserialize_with = "crate::profile::ui::deserialize_vitals_colors")]
    VitalsColors(std::collections::BTreeMap<String, u8>),
    /// The text it replaces goes first among the earlier texts.
    VitalsText(String),
    VitalsTextPrevious(Vec<String>),
    VitalsHit(bool),
    ChipStyle(String),
    TickCount(String),
    GameTime(String),
    AffectsStyle(String),
    AffectsMarker(String),
    AffectsTint(bool),
    #[serde(deserialize_with = "crate::profile::ui::deserialize_affects_running_out_hours")]
    AffectsRunningOutHours(u32),
    #[serde(deserialize_with = "crate::profile::ui::deserialize_affects_almost_gone_hours")]
    AffectsAlmostGoneHours(u32),
    SnoopShare(f64),
    SnoopFolded(bool),
    LogSessions(Option<bool>),
    ScrollbackLines(u32),
    WritingCardLeft(Option<f64>),
    WritingCardTop(Option<f64>),
    WritingCardRows(Option<u32>),
    WritingCardCols(Option<u32>),
    WritingCardPinned(bool),
}

/// Save the fields a page names and leave every other one as it is, so
/// two windows that each change a field keep both changes. With no
/// profile it writes the selected session's. It sends no event, since
/// the page that saved tells the windows, apart from the vitals text a
/// new `vitals_text` draws in each session that watches it.
#[tauri::command]
pub(crate) async fn ui_set_fields<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, SharedState>,
    fields: Vec<UiField>,
    profile: Option<String>,
) -> Result<(), String> {
    for (session, drawn) in set_fields(state.inner(), fields, profile).await? {
        crate::sessions::emit_for(&app, session, events::VITALS_TEXT, &drawn);
    }
    Ok(())
}

/// Write `fields` onto the profile named `profile`, which a session must
/// play, or onto the selected session's when it names none, and save it.
/// Returns the vitals text a new `vitals_text` drew in each session on
/// the profile that watches it.
async fn set_fields(
    state: &SharedState,
    fields: Vec<UiField>,
    profile: Option<String>,
) -> Result<Vec<(SessionId, VitalsText)>, String> {
    let sessions = state.all_sessions();
    let (open, drawn, resized, marked) = {
        let mut p = state.lock_named(profile).await?;
        let text = p.ui.vitals_text.clone();
        let lines = p.ui.scrollback_lines;
        let mark = crate::input::echo_mark(&p.ui);
        apply_fields(&mut p.ui, fields);
        // A new Scrollback size reaches every session on the profile.
        let resized: Vec<_> = if p.ui.scrollback_lines == lines {
            Vec::new()
        } else {
            p.players(&sessions).cloned().collect()
        };
        // So does a new mark or mark color, for the native grid.
        let mark = Some(crate::input::echo_mark(&p.ui)).filter(|now| *now != mark);
        let marked: Vec<_> = match mark {
            Some(mark) => p.players(&sessions).map(|s| (s.id, mark.clone())).collect(),
            None => Vec::new(),
        };
        let mut drawn = Vec::new();
        if p.ui.vitals_text != text {
            let now = tokio::time::Instant::now();
            for session in p.players(&sessions) {
                let c = session.connection.lock();
                if let Some(text) = vitals_text::render(session, &p, &c, now) {
                    drawn.push((session.id, text));
                }
            }
        }
        (
            p.open().clone(),
            drawn,
            (resized, p.ui.scrollback_lines),
            marked,
        )
    };
    let (resized, lines) = resized;
    for session in resized {
        crate::logs::keep_scrollback_lines(&session, lines).await;
    }
    for (session, mark) in marked {
        crate::input::keep_echo_mark(session, mark);
    }
    persist_profile(state, &open).await;
    Ok(drawn)
}

/// Write each field onto `ui` through its coercer. The affects
/// thresholds are held in order once every field is in, so the order of
/// the fields never matters.
fn apply_fields(ui: &mut crate::profile::ui::UiConfig, fields: Vec<UiField>) {
    use crate::profile::ui as cfg;
    for field in fields {
        match field {
            UiField::Theme(v) => ui.theme = v,
            UiField::FollowSystemAppearance(v) => cfg::set_follow_system_appearance(ui, v),
            UiField::LightTheme(v) => ui.light_theme = cfg::coerce_light_theme(v),
            UiField::DarkTheme(v) => ui.dark_theme = cfg::normalize_dark_theme(v),
            UiField::ThemeFollow(v) => cfg::set_theme_follow(ui, v),
            UiField::DayTheme(v) => ui.day_theme = cfg::normalize_day_night_theme(v),
            UiField::NightTheme(v) => ui.night_theme = cfg::normalize_day_night_theme(v),
            UiField::AutoUpdate(v) => ui.auto_update = v,
            UiField::FontFamily(v) => ui.font_family = v,
            UiField::FontSize(v) => ui.font_size = cfg::coerce_font_size(v),
            UiField::TerminalLineHeight(v) => {
                ui.terminal_line_height = cfg::coerce_terminal_line_height(v);
            }
            UiField::PanelFont(v) => ui.panel_font = cfg::normalize_panel_font(v),
            UiField::PanelFontSize(v) => ui.panel_font_size = cfg::coerce_panel_font_size(v),
            UiField::EnabledPresets(v) => ui.enabled_presets = cfg::normalize_enabled_presets(v),
            UiField::KeepLastCommand(v) => ui.keep_last_command = v,
            UiField::ThemeTerminalColors(v) => ui.theme_terminal_colors = v,
            UiField::BrightBold(v) => ui.bright_bold = v,
            UiField::BlinkText(v) => ui.blink_text = v,
            UiField::FitGameColors(v) => ui.fit_game_colors = v,
            UiField::ColorVision(v) => ui.color_vision = cfg::coerce_color_vision(v),
            UiField::ReadableHighlights(v) => ui.readable_highlights = v,
            UiField::ScreenReader(v) => ui.screen_reader = v,
            UiField::ScreenReaderBackground(v) => ui.screen_reader_background = v,
            UiField::ScreenReaderPrompt(v) => ui.screen_reader_prompt = v,
            UiField::ScreenReaderBurst(v) => {
                ui.screen_reader_burst = cfg::coerce_screen_reader_burst(v);
            }
            UiField::CollapseRepeats(v) => ui.collapse_repeats = v,
            UiField::CollapseFightLines(v) => ui.collapse_fight_lines = v,
            UiField::CollapseAttackLines(v) => ui.collapse_attack_lines = v,
            UiField::TerminalBaseAnsi(v) => ui.terminal_base_ansi = v,
            UiField::CustomThemes(v) => ui.custom_themes = v,
            UiField::SplitDividerColor(v) => {
                ui.split_divider_color = cfg::normalize_optional_color(v);
            }
            UiField::InputEchoColor(v) => ui.input_echo_color = cfg::normalize_optional_color(v),
            UiField::EchoMacros(v) => ui.echo_macros = v,
            UiField::InputEchoMark(v) => cfg::set_input_echo_mark(ui, v),
            UiField::InputEchoMarkText(v) => {
                ui.input_echo_mark_text = cfg::coerce_input_echo_mark_text(v);
            }
            UiField::InputEchoMarkColor(v) => {
                ui.input_echo_mark_color = cfg::normalize_optional_color(v);
            }
            UiField::InputEchoDim(v) => ui.input_echo_dim = v,
            UiField::InputLineMark(v) => ui.input_line_mark = v,
            UiField::PasteLineDelayMs(v) => {
                ui.paste_line_delay_ms = cfg::coerce_paste_line_delay_ms(v);
            }
            UiField::SpellcheckPrompt(v) => ui.spellcheck_prompt = v,
            UiField::WritingOffer(v) => ui.writing_offer = v,
            UiField::WritingAskPost(v) => ui.writing_ask_post = v,
            UiField::InputCursorStyle(v) => {
                ui.input_cursor_style = cfg::coerce_input_cursor_style(v);
            }
            UiField::InputCaretBlink(v) => ui.input_caret_blink = v,
            UiField::InputCaretColor(v) => ui.input_caret_color = cfg::normalize_optional_color(v),
            UiField::InputLineColor(v) => ui.input_line_color = cfg::normalize_optional_color(v),
            UiField::InputLineBackground(v) => {
                ui.input_line_background = cfg::coerce_input_line_background(v);
            }
            UiField::InputLineBackgroundColor(v) => {
                ui.input_line_background_color = cfg::normalize_optional_color(v);
            }
            UiField::InputLineSize(v) => ui.input_line_size = cfg::coerce_input_line_size(v),
            UiField::InputTypeColors(v) => ui.input_type_colors = v,
            UiField::InputTypeAliasColor(v) => {
                ui.input_type_alias_color = cfg::normalize_optional_color(v);
            }
            UiField::InputTypeHashColor(v) => {
                ui.input_type_hash_color = cfg::normalize_optional_color(v);
            }
            UiField::InputTypeChatColor(v) => {
                ui.input_type_chat_color = cfg::normalize_optional_color(v);
            }
            UiField::InputTypeUnknownColor(v) => {
                ui.input_type_unknown_color = cfg::normalize_optional_color(v);
            }
            UiField::VitalsDensity(v) => ui.vitals_density = cfg::coerce_vitals_density(v),
            UiField::VitalsValues(v) => ui.vitals_values = cfg::coerce_vitals_values(v),
            UiField::VitalsMeter(v) => ui.vitals_meter = cfg::coerce_vitals_meter(v),
            UiField::VitalsWarnThirds(v) => ui.vitals_warn_thirds = v,
            UiField::VitalsHideWhenPinned(v) => ui.vitals_hide_when_pinned = v,
            UiField::VitalsStyle(v) => ui.vitals_style = cfg::coerce_vitals_style(v),
            UiField::VitalsPlace(v) => ui.vitals_place = cfg::coerce_vitals_place(v),
            UiField::VitalsOrder(v) => ui.vitals_order = cfg::coerce_vitals_order(v),
            UiField::VitalsOff(v) => ui.vitals_off = cfg::coerce_vitals_off(v),
            UiField::VitalsOpponent(v) => ui.vitals_opponent = cfg::coerce_vitals_opponent(v),
            UiField::VitalsColors(v) => ui.vitals_colors = v,
            UiField::VitalsText(v) => cfg::replace_vitals_text(ui, v),
            UiField::VitalsTextPrevious(v) => {
                ui.vitals_text_previous = cfg::normalize_vitals_text_previous(v);
            }
            UiField::VitalsHit(v) => ui.vitals_hit = v,
            UiField::ChipStyle(v) => ui.chip_style = cfg::coerce_chip_style(v),
            UiField::TickCount(v) => ui.tick_count = cfg::coerce_tick_count(v),
            UiField::GameTime(v) => ui.game_time = cfg::coerce_game_time(v),
            UiField::AffectsStyle(v) => ui.affects_style = cfg::coerce_affects_style(v),
            UiField::AffectsMarker(v) => ui.affects_marker = cfg::coerce_affects_marker(v),
            UiField::AffectsTint(v) => ui.affects_tint = v,
            UiField::AffectsRunningOutHours(v) => ui.affects_running_out_hours = v,
            UiField::AffectsAlmostGoneHours(v) => ui.affects_almost_gone_hours = v,
            UiField::SnoopShare(v) => ui.snoop_share = cfg::coerce_snoop_share(v),
            UiField::SnoopFolded(v) => ui.snoop_folded = v,
            UiField::LogSessions(v) => ui.log_sessions = v,
            UiField::ScrollbackLines(v) => ui.scrollback_lines = cfg::coerce_scrollback_lines(v),
            UiField::WritingCardLeft(v) => ui.writing_card_left = cfg::coerce_writing_card_edge(v),
            UiField::WritingCardTop(v) => ui.writing_card_top = cfg::coerce_writing_card_edge(v),
            UiField::WritingCardRows(v) => ui.writing_card_rows = cfg::coerce_writing_card_rows(v),
            UiField::WritingCardCols(v) => ui.writing_card_cols = cfg::coerce_writing_card_cols(v),
            UiField::WritingCardPinned(v) => ui.writing_card_pinned = v,
        }
    }
    (ui.affects_running_out_hours, ui.affects_almost_gone_hours) =
        cfg::coerce_affects_thresholds(ui.affects_running_out_hours, ui.affects_almost_gone_hours);
}

/// Replace a profile's theme choice without touching the rest of the UI
/// config, for the main window's palette. The caller applies and
/// broadcasts the theme itself. While Switch themes follows the system or
/// the game, a pick fills the slot that is showing, light or dark, day or
/// night, so the caller also sends that slot.
#[tauri::command]
pub(crate) async fn ui_set_theme(
    state: State<'_, SharedState>,
    theme: String,
    light_theme: Option<String>,
    dark_theme: Option<String>,
    day_theme: Option<String>,
    night_theme: Option<String>,
    profile: Option<String>,
) -> Result<(), String> {
    let slots = ThemeSlots {
        light: light_theme,
        dark: dark_theme,
        day: day_theme,
        night: night_theme,
    };
    let open = {
        let mut p = state.lock_named(profile).await?;
        if !apply_theme_pick(&mut p.ui, theme, slots) {
            return Ok(());
        }
        p.open().clone()
    };
    let shared: SharedState = state.inner().clone();
    persist_profile(&shared, &open).await;
    Ok(())
}

/// The slots a palette pick can fill beside the theme. None leaves a
/// slot alone.
#[derive(Default)]
struct ThemeSlots {
    light: Option<String>,
    dark: Option<String>,
    day: Option<String>,
    night: Option<String>,
}

/// Write a theme pick onto the live UI config. A missing or blank slot
/// leaves that slot alone. Returns whether anything changed, so an
/// unchanged pick skips the save.
fn apply_theme_pick(
    ui: &mut crate::profile::ui::UiConfig,
    theme: String,
    slots: ThemeSlots,
) -> bool {
    let mut changed = false;
    let mut set = |slot: &mut String, value: String| {
        if !value.is_empty() && *slot != value {
            *slot = value;
            changed = true;
        }
    };
    set(&mut ui.theme, theme);
    for (slot, pick) in [
        (&mut ui.light_theme, slots.light),
        (&mut ui.dark_theme, slots.dark),
        (&mut ui.day_theme, slots.day),
        (&mut ui.night_theme, slots.night),
    ] {
        if let Some(v) = pick {
            set(slot, v);
        }
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

/// The chat pane's channel colors.
#[tauri::command]
pub(crate) async fn ui_get_chat_colors(
    state: State<'_, SharedState>,
    profile: Option<String>,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    let p = state.lock_named(profile).await?;
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
    profile: Option<String>,
) -> Result<(), String> {
    let (open, changed) = {
        let mut p = state.lock_named(profile).await?;
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
    profile: Option<String>,
) -> Result<(), String> {
    let (open, changed) = {
        let mut p = state.lock_named(profile).await?;
        let changed = reset_chat_colors(&mut p.ui);
        (p.open().clone(), changed)
    };
    send_chat_colors(&app, state.inner(), &open, changed).await;
    Ok(())
}

/// Save `open` and tell every window, when a chat color moved and `open`
/// is in front.
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

    /// The payload fields with no setter in `ui_set_fields`. The tracked
    /// affects have a setter of their own, and the 0.7 vitals are read
    /// only.
    const READ_ONLY: [&str; 3] = [
        "tracked_affects",
        "vitals_legacy_style",
        "vitals_legacy_text",
    ];

    /// The setter for the field `key` with `value`, as the page sends it.
    fn setter(key: &str, value: &serde_json::Value) -> super::UiField {
        serde_json::from_value(serde_json::json!({ "field": key, "value": value })).unwrap()
    }

    /// Send `ui` out through `ui_get_config`, across the JSON bridge, and
    /// back onto a fresh config with every field through its setter in
    /// `ui_set_fields`. The tracked affects have a setter of their own.
    fn through_payload(ui: &UiConfig) -> UiConfig {
        let json = serde_json::to_value(UiConfigPayload::from_ui(ui)).unwrap();
        let fields = json
            .as_object()
            .unwrap()
            .iter()
            .filter(|(key, _)| !READ_ONLY.contains(&key.as_str()))
            .map(|(key, value)| setter(key, value))
            .collect();
        let mut out = UiConfig::default();
        super::apply_fields(&mut out, fields);
        out
    }

    #[test]
    fn each_setter_writes_its_field_alone_and_two_windows_keep_both() {
        // One value for each field ui_set_fields takes, none of them the
        // default. src/ipc/uiConfig.test.ts reads the same file.
        let text = include_str!("../../../fixtures/ui-config/fields.json");
        let fixture: serde_json::Value = serde_json::from_str(text).unwrap();
        let values = fixture["fields"].as_object().unwrap();
        let sent = |ui: &UiConfig| serde_json::to_value(UiConfigPayload::from_ui(ui)).unwrap();
        let defaults = sent(&UiConfig::default());
        // A field the payload gains without a setter fails here.
        let keys: Vec<&String> = defaults
            .as_object()
            .unwrap()
            .keys()
            .filter(|key| !READ_ONLY.contains(&key.as_str()))
            .collect();
        let names: Vec<&String> = values.keys().collect();
        assert_eq!(names, keys);
        // Follow system appearance on is the system mode of Switch themes,
        // so that setter writes the mode too.
        let write = |want: &mut serde_json::Value, f: &str| {
            want[f] = values[f].clone();
            if f == "follow_system_appearance" {
                want["theme_follow"] = "system".into();
            }
        };
        for (at, f) in names.iter().enumerate() {
            let mut ui = UiConfig::default();
            super::apply_fields(&mut ui, vec![setter(f, &values[*f])]);
            let mut want = defaults.clone();
            write(&mut want, f);
            assert_eq!(sent(&ui), want, "{f} alone");
            // Another window writes the next field onto the same profile.
            let g = names[(at + 1) % names.len()];
            super::apply_fields(&mut ui, vec![setter(g, &values[g])]);
            write(&mut want, g);
            assert_eq!(sent(&ui), want, "{f}, then {g}");
        }
        // Game turns the switch off, so 0.8.1 reads it as off.
        let mut ui = UiConfig::default();
        super::apply_fields(
            &mut ui,
            vec![
                setter("follow_system_appearance", &true.into()),
                setter("theme_follow", &"game".into()),
            ],
        );
        assert_eq!(sent(&ui)["follow_system_appearance"], false);
        assert_eq!(sent(&ui)["theme_follow"], "game");
    }

    #[tokio::test]
    async fn a_named_profile_takes_the_fields_and_the_selected_one_keeps_its_own() {
        use std::sync::Arc;

        use crate::app::state::{AppState, SharedState};
        use crate::profile::live::Profile;
        use crate::profile::set::ProfileSet;

        // Every file a save writes lands in the profile set's folder.
        let dir = tempfile::tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.create("Orla").unwrap();
        let orla_file = set.profile_path("Orla");
        let state: SharedState = Arc::new(AppState::default());
        state.set_profiles(set).await;
        let orla = state.add_open_profile("Orla", Profile::default());
        state.open_session(orla.clone());
        let game_time = || vec![setter("game_time", &"12h".into())];

        super::set_fields(&state, game_time(), Some("Orla".into()))
            .await
            .unwrap();
        assert_eq!(orla.lock().await.ui.game_time, "12h");
        assert_eq!(state.selected_profile().await.ui.game_time, "24h");
        let saved = ProfileConfig::from_toml(&std::fs::read_to_string(&orla_file).unwrap());
        assert_eq!(saved.unwrap().ui.game_time, "12h");

        // No profile named writes the selected session's.
        let tick_count = vec![setter("tick_count", &"down".into())];
        super::set_fields(&state, tick_count, None).await.unwrap();
        assert_eq!(state.selected_profile().await.ui.tick_count, "down");
        assert_eq!(orla.lock().await.ui.tick_count, "up");

        assert_eq!(
            super::set_fields(&state, game_time(), Some("Maren".into())).await,
            Err("Maren closed before Vosh could save this change.".to_string())
        );
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn a_new_mark_reaches_the_native_grid_of_each_session_on_the_profile() {
        use std::sync::Arc;

        use crate::app::state::{AppState, SharedState};
        use crate::native::grid;
        use crate::profile::live::Profile;
        use crate::profile::set::ProfileSet;

        // The grid map is shared with the other tests.
        let _grid = grid::lock_shared_grid_for_test();
        let dir = tempfile::tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.create("Orla").unwrap();
        let state: SharedState = Arc::new(AppState::default());
        state.set_profiles(set).await;
        let orla = state.add_open_profile("Orla", Profile::default());
        let session = state.open_session(orla);
        let echo = |mark: &str, command: &str| {
            grid::feed_session_output(session.id, &text(b"Your choice> "), None);
            grid::feed_local(session.id, format!("{mark}{command}\r\n").as_bytes());
        };
        let gt = "\x1b[90m> \x1b[0m";
        // The grid leaves out the chevron until the mark changes.
        echo(gt, "1");
        let fields = vec![setter("input_echo_mark", &"gt".into())];
        super::set_fields(&state, fields, Some("Orla".into()))
            .await
            .unwrap();
        echo(gt, "2");
        // A new mark color changes the bytes it leaves out.
        let fields = vec![setter("input_echo_mark_color", &"#c6a46a".into())];
        super::set_fields(&state, fields, Some("Orla".into()))
            .await
            .unwrap();
        echo("\x1b[38;2;198;164;106m> \x1b[0m", "3");
        let rows = grid::screen_rows(session.id).unwrap().rows;
        assert_eq!(
            rows[..3],
            ["Your choice> > 1", "Your choice> 2", "Your choice> 3"]
        );
    }

    /// Game output of `bytes`, as the session hands it to the grid.
    fn text(bytes: &[u8]) -> vosh_prompt::stage::Output {
        let mut out = vosh_prompt::stage::Output::new(false);
        out.text(bytes);
        out
    }

    #[test]
    fn a_theme_pick_that_names_a_profile_writes_that_profile() {
        use std::sync::Arc;

        use tauri::test::{mock_builder, mock_context, noop_assets};
        use tauri::Manager;

        use crate::app::state::{AppState, SharedState};
        use crate::profile::live::Profile;
        use crate::profile::set::ProfileSet;
        use crate::profile::shared::{Scope, ScopeConfig};

        let dir = tempfile::tempdir().unwrap();
        let mut set = ProfileSet::load_or_migrate(dir.path().to_path_buf()).unwrap();
        set.create("Orla").unwrap();
        // Each profile keeps a theme of its own, since a shared one would
        // reach every open profile through global.toml.
        let scope = ScopeConfig {
            theme: Scope::Profile,
            ..*set.scope()
        };
        set.set_scope(scope).unwrap();
        let orla_file = set.profile_path("Orla");
        let app = mock_builder().build(mock_context(noop_assets())).unwrap();
        app.manage::<SharedState>(Arc::new(AppState::default()));
        let state: SharedState = app.state::<SharedState>().inner().clone();
        tauri::async_runtime::block_on(async {
            state.set_profiles(set).await;
            let orla = state.add_open_profile("Orla", Profile::default());
            state.open_session(orla.clone());
            let shown = state.selected_profile().await.ui.theme.clone();

            super::ui_set_theme(
                app.state(),
                "nord".into(),
                None,
                None,
                None,
                None,
                Some("Orla".into()),
            )
            .await
            .unwrap();
            assert_eq!(orla.lock().await.ui.theme, "nord");
            assert_eq!(state.selected_profile().await.ui.theme, shown);
            let saved = ProfileConfig::from_toml(&std::fs::read_to_string(&orla_file).unwrap());
            assert_eq!(saved.unwrap().ui.theme, "nord");
        });
    }

    #[test]
    fn ui_defaults_match_the_ones_the_page_fills() {
        // normalizeUiConfig in src/ipc/uiConfig.ts fills a missing field
        // with each of these values, and src/ipc/uiConfig.test.ts reads
        // the same file to hold it there.
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

    #[test]
    fn your_0_7_vitals_mark_the_style_they_grew_into() {
        let sent = |toml: &str| {
            let ui = ProfileConfig::from_toml(toml).unwrap().ui;
            let json = serde_json::to_value(UiConfigPayload::from_ui(&ui)).unwrap();
            (
                json.get("vitals_legacy_style").cloned(),
                json.get("vitals_legacy_text").cloned(),
            )
        };
        // Layout gauges with a template on, so 0.7 drew the template.
        let full = include_str!("../../../fixtures/config/profile.full.toml");
        assert_eq!(
            sent(full),
            (
                Some("text".into()),
                Some("%hp/%maxhp %mana/%maxmn %move/%maxmv".into())
            )
        );
        // Every profile saved layout ember, which gives no mark.
        let default = include_str!("../../../fixtures/config/profile.default.toml");
        assert_eq!(sent(default), (None, None));

        for (layout, style) in [
            ("gauges", Some("gauges")),
            ("pips", Some("pips")),
            ("strip", Some("line")),
            ("inline", Some("line")),
            ("stacked", Some("rows")),
            ("ember", None),
            ("sparkle", None),
        ] {
            let toml = format!("[ui.vitals]\nlayout = \"{layout}\"\n");
            assert_eq!(sent(&toml), (style.map(Into::into), None), "{layout}");
        }
        // The shipped template, on, at a bar width of its own.
        let toml = "[ui.vitals]\ntemplate_enabled = true\nbar_width = 12\ntemplate = \"%bar_hp %pct_mn\"\n";
        assert_eq!(
            sent(toml),
            (Some("text".into()), Some("%{hp:bar:12} %pct_mana%%".into()))
        );
    }

    #[test]
    fn the_settings_payload_carries_nothing_of_your_prompt() {
        let (p, _) = prompt_profile();
        let json = serde_json::to_value(UiConfigPayload::from_ui(&p.ui)).unwrap();
        let keys = json.as_object().unwrap();
        for key in ["prompt_template_enabled", "prompt_template", "prompt_show"] {
            assert!(!keys.contains_key(key), "{key} reaches Settings");
        }
    }

    #[test]
    fn a_settings_save_leaves_the_prompt_table_and_its_copy_alone() {
        let (mut p, mut c) = prompt_profile();
        let mut config = c.prompt.config().clone();
        config.show = vosh_prompt::PromptShow::Pinned;
        crate::prompt::take_config(&mut p, &mut c, config);
        let table = p.prompt.clone();
        super::apply_fields(&mut p.ui, vec![setter("font_size", &16.into())]);
        assert_eq!(p.ui.font_size, 16);
        assert_eq!(p.prompt, table);

        // The file keeps the table, and [ui] its copy of the switch and
        // the design for an older build.
        let file = crate::profile::file::ProfileConfig::from_profile(&p);
        assert_eq!(file.prompt_config(), table);
        assert!(file.ui.prompt_template_enabled);
        assert_eq!(file.ui.prompt_template, "%hp");
    }

    #[test]
    fn an_unknown_field_name_refuses_the_whole_save() {
        // Your prompt saves through the prompt commands, the tracked
        // affects through tracked_affects_set and the chat colors through
        // the pane menu, so none of them is a field here.
        for key in [
            "prompt_template_enabled",
            "prompt_template",
            "prompt_show",
            "tracked_affects",
            "chat_colors",
        ] {
            let fields = serde_json::json!([
                { "field": "font_size", "value": 16 },
                { "field": key, "value": null },
            ]);
            let read = serde_json::from_value::<Vec<super::UiField>>(fields);
            assert!(read.is_err(), "{key}");
        }
    }

    #[test]
    fn follow_system_appearance_round_trips() {
        let mut ui = UiConfig::default();
        crate::profile::ui::set_follow_system_appearance(&mut ui, true);
        assert!(through_payload(&ui).follow_system_appearance);
        assert!(!through_payload(&UiConfig::default()).follow_system_appearance);
    }

    #[test]
    fn the_switch_themes_keys_round_trip() {
        let mut ui = UiConfig::default();
        for mode in ["off", "system", "game"] {
            crate::profile::ui::set_theme_follow(&mut ui, mode.into());
            let back = through_payload(&ui);
            assert_eq!(back.theme_follow, mode);
            assert_eq!(back.follow_system_appearance, mode == "system");
        }
        crate::profile::ui::set_theme_follow(&mut ui, "dusk".into());
        assert_eq!(through_payload(&ui).theme_follow, "off");
        ui.day_theme = "solarized-light".into();
        ui.night_theme = "tokyo-night".into();
        let back = through_payload(&ui);
        assert_eq!(back.day_theme, "solarized-light");
        assert_eq!(back.night_theme, "tokyo-night");
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
    fn the_panel_font_round_trips() {
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
    }

    #[test]
    fn the_panel_size_round_trips() {
        let mut ui = UiConfig::default();
        assert_eq!(through_payload(&ui).panel_font_size, 12);
        for pick in [0, 11, 16] {
            ui.panel_font_size = pick;
            assert_eq!(through_payload(&ui).panel_font_size, pick);
        }
        ui.panel_font_size = 200;
        assert_eq!(through_payload(&ui).panel_font_size, 64);
    }

    #[test]
    fn the_command_line_look_round_trips() {
        let mut ui = UiConfig::default();
        let back = through_payload(&ui);
        assert!(back.input_caret_blink);
        assert_eq!(back.input_line_background, "theme");
        assert_eq!(back.input_line_size, 0);
        ui.input_caret_blink = false;
        ui.input_caret_color = Some("#c6a46a".into());
        ui.input_line_color = Some("#d8dee9".into());
        ui.input_line_background = "own".into();
        ui.input_line_background_color = Some("#1d1f21".into());
        ui.input_line_size = 18;
        let back = through_payload(&ui);
        assert!(!back.input_caret_blink);
        assert_eq!(back.input_caret_color.as_deref(), Some("#c6a46a"));
        assert_eq!(back.input_line_color.as_deref(), Some("#d8dee9"));
        assert_eq!(back.input_line_background, "own");
        assert_eq!(back.input_line_background_color.as_deref(), Some("#1d1f21"));
        assert_eq!(back.input_line_size, 18);
        ui.input_caret_color = Some("  ".into());
        ui.input_line_background = "glass".into();
        ui.input_line_size = 3;
        let back = through_payload(&ui);
        assert_eq!(back.input_caret_color, None);
        assert_eq!(back.input_line_background, "theme");
        assert_eq!(back.input_line_size, 6);
    }

    #[test]
    fn coloring_as_you_type_round_trips() {
        let mut ui = UiConfig::default();
        let back = through_payload(&ui);
        assert!(!back.input_type_colors);
        assert_eq!(back.input_type_alias_color, None);
        ui.input_type_colors = true;
        ui.input_type_alias_color = Some("#8abeb7".into());
        ui.input_type_hash_color = Some("#b294bb".into());
        ui.input_type_chat_color = Some(" #f0c674 ".into());
        ui.input_type_unknown_color = Some(String::new());
        let back = through_payload(&ui);
        assert!(back.input_type_colors);
        assert_eq!(back.input_type_alias_color.as_deref(), Some("#8abeb7"));
        assert_eq!(back.input_type_hash_color.as_deref(), Some("#b294bb"));
        assert_eq!(back.input_type_chat_color.as_deref(), Some("#f0c674"));
        assert_eq!(back.input_type_unknown_color, None);
    }

    #[test]
    fn a_theme_pick_writes_only_what_it_names() {
        use super::ThemeSlots;

        let mut ui = UiConfig::default();
        assert!(super::apply_theme_pick(
            &mut ui,
            "nord".into(),
            ThemeSlots::default()
        ));
        assert_eq!(ui.theme, "nord");
        assert_eq!(ui.light_theme, "vellum");
        assert_eq!(ui.dark_theme, "");

        // A pick while following the system fills the dark slot.
        let dark = || ThemeSlots {
            light: Some("vellum".into()),
            dark: Some("tokyo-night".into()),
            ..ThemeSlots::default()
        };
        assert!(super::apply_theme_pick(&mut ui, "nord".into(), dark()));
        assert_eq!(ui.theme, "nord");
        assert_eq!(ui.dark_theme, "tokyo-night");

        // The same pick again changes nothing, and a blank slot is left alone.
        let blank_light = ThemeSlots {
            light: Some(String::new()),
            ..dark()
        };
        assert!(!super::apply_theme_pick(
            &mut ui,
            "nord".into(),
            blank_light
        ));
        assert_eq!(ui.light_theme, "vellum");
    }

    #[test]
    fn a_theme_pick_while_following_the_game_fills_day_or_night() {
        use super::ThemeSlots;

        let mut ui = UiConfig::default();
        let night = ThemeSlots {
            night: Some("tokyo-night".into()),
            ..ThemeSlots::default()
        };
        assert!(super::apply_theme_pick(
            &mut ui,
            "tokyo-night".into(),
            night
        ));
        assert_eq!(ui.night_theme, "tokyo-night");
        assert_eq!(ui.day_theme, "");
        let day = ThemeSlots {
            day: Some("solarized-light".into()),
            ..ThemeSlots::default()
        };
        assert!(super::apply_theme_pick(
            &mut ui,
            "solarized-light".into(),
            day
        ));
        assert_eq!(ui.day_theme, "solarized-light");
        assert_eq!(ui.night_theme, "tokyo-night");
        assert_eq!(ui.light_theme, "vellum");
        assert_eq!(ui.dark_theme, "");
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
    fn the_vitals_styles_keys_round_trip() {
        use std::collections::BTreeMap;
        let fresh = UiConfig::default();
        let back = through_payload(&fresh);
        assert_eq!(back.vitals_style, None);
        assert_eq!(back.vitals_order, fresh.vitals_order);
        assert_eq!(back.vitals_text_previous, Vec::<String>::new());

        assert!(!back.vitals_hit);

        let ui = UiConfig {
            vitals_style: Some("text".into()),
            vitals_place: "status".into(),
            vitals_order: vec!["mana".into(), "move".into(), "hp".into()],
            vitals_off: vec!["hp".into(), "opponent".into()],
            vitals_opponent: "bottom".into(),
            vitals_colors: BTreeMap::from([("move".into(), 10)]),
            vitals_text: "%hp/%maxhp %mn/%maxmn %mv/%maxmv".into(),
            vitals_text_previous: vec!["%hp(%pct_hp)h".into(), "%mv(%pct_mv)v".into()],
            vitals_hit: true,
            ..UiConfig::default()
        };
        let back = through_payload(&ui);
        assert!(back.vitals_hit);
        assert_eq!(back.vitals_style, ui.vitals_style);
        assert_eq!(back.vitals_place, ui.vitals_place);
        assert_eq!(back.vitals_order, ui.vitals_order);
        assert_eq!(back.vitals_off, ui.vitals_off);
        assert_eq!(back.vitals_opponent, ui.vitals_opponent);
        assert_eq!(back.vitals_colors, ui.vitals_colors);
        assert_eq!(back.vitals_text, ui.vitals_text);
        assert_eq!(back.vitals_text_previous, ui.vitals_text_previous);
    }

    #[test]
    fn the_vitals_setters_coerce_junk_and_a_new_text_keeps_the_old_one() {
        use std::collections::BTreeMap;
        let mut ui = UiConfig::default();
        let fields = vec![
            setter("vitals_style", &"rows".into()),
            setter("vitals_place", &"footer".into()),
            setter("vitals_order", &serde_json::json!(["move", "move", "tp"])),
            setter("vitals_off", &serde_json::json!(["opponent", "tp"])),
            setter("vitals_opponent", &"middle".into()),
            setter(
                "vitals_colors",
                &serde_json::json!({ "hp": 300, "mana": 4, "move": "red", "tp": 2 }),
            ),
        ];
        super::apply_fields(&mut ui, fields);
        assert_eq!(ui.vitals_style, None);
        assert_eq!(ui.vitals_place, "panel");
        assert_eq!(ui.vitals_order, ["move", "hp", "mana"]);
        assert_eq!(ui.vitals_off, ["opponent"]);
        assert_eq!(ui.vitals_opponent, "top");
        assert_eq!(ui.vitals_colors, BTreeMap::from([("mana".into(), 4)]));

        let text = |t: &str| vec![setter("vitals_text", &t.into())];
        super::apply_fields(&mut ui, text("%hp/%maxhp"));
        super::apply_fields(&mut ui, text("%mn/%maxmn"));
        super::apply_fields(&mut ui, text("%mv/%maxmv"));
        super::apply_fields(&mut ui, text("%mv/%maxmv"));
        assert_eq!(ui.vitals_text, "%mv/%maxmv");
        assert_eq!(ui.vitals_text_previous, ["%mn/%maxmn", "%hp/%maxhp"]);
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
    fn the_screen_reader_fields_round_trip_and_the_burst_holds_to_its_four() {
        let mut ui = UiConfig::default();
        let back = through_payload(&ui);
        assert!(
            !back.screen_reader && !back.screen_reader_background && !back.screen_reader_prompt
        );
        assert_eq!(back.screen_reader_burst, 8);
        ui.screen_reader = true;
        ui.screen_reader_background = true;
        ui.screen_reader_prompt = true;
        ui.screen_reader_burst = 32;
        let back = through_payload(&ui);
        assert!(back.screen_reader && back.screen_reader_background && back.screen_reader_prompt);
        assert_eq!(back.screen_reader_burst, 32);
        ui.screen_reader_burst = 7;
        assert_eq!(through_payload(&ui).screen_reader_burst, 8);
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

    /// The affects thresholds after `fields` land on the defaults.
    fn thresholds_after(fields: Vec<super::UiField>) -> (u32, u32) {
        let mut ui = UiConfig::default();
        super::apply_fields(&mut ui, fields);
        (ui.affects_running_out_hours, ui.affects_almost_gone_hours)
    }

    #[test]
    fn a_page_that_sends_odd_affects_thresholds_still_saves() {
        let read = |running_out: serde_json::Value, almost_gone: serde_json::Value| {
            thresholds_after(vec![
                setter("affects_running_out_hours", &running_out),
                setter("affects_almost_gone_hours", &almost_gone),
            ])
        };
        assert_eq!(thresholds_after(Vec::new()), (2, 1));
        assert_eq!(read(4.6.into(), (-3).into()), (5, 0));
        assert_eq!(read("6".into(), serde_json::Value::Null), (6, 1));
    }

    #[test]
    fn the_affects_thresholds_end_the_same_in_either_order() {
        let running_out = || setter("affects_running_out_hours", &5.into());
        let almost_gone = || setter("affects_almost_gone_hours", &4.into());
        assert_eq!(thresholds_after(vec![running_out(), almost_gone()]), (5, 4));
        assert_eq!(thresholds_after(vec![almost_gone(), running_out()]), (5, 4));
    }

    #[test]
    fn the_config_settings_reads_carries_no_chat_colors() {
        // Only the pane menu reads and writes them, through its own
        // commands.
        let mut ui = UiConfig::default();
        ui.chat_colors.insert("say".into(), "brightBlue".into());
        let json = serde_json::to_value(UiConfigPayload::from_ui(&ui)).unwrap();
        assert!(json.get("chat_colors").is_none());
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
