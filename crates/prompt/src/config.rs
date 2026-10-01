//! The `[prompt]` table of a profile file (section 5 of the build spec).
//!
//! ```toml
//! [prompt]
//! draw = true
//! template = "%{c:100,100,100}[%c_reset%s_italic%hp(…"
//! previous_templates = []
//!
//! [prompt.capture]
//! kind = "regex"
//! lines = ['\[(?<hp>\d+)/(?<maxhp>\d+)hp …\]']
//! settle = false
//! source = "migrated"
//! ```
//!
//! Only the source strings are stored. Shapes are compiled at load, so a
//! compiler fix needs no migration. Older builds keep the switch and the
//! template in `[ui] prompt_template_enabled` and `prompt_template`, which
//! [`PromptConfig::from_legacy`] reads when a file has no `[prompt]`.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

use crate::presets::DEFAULT_DESIGN;

/// How many earlier designs `previous_templates` keeps.
pub const PREVIOUS_TEMPLATES: usize = 2;

/// The `[prompt]` table: whether Vosh draws your prompt, the design it
/// draws, the designs the card found before, and how Vosh reads the
/// game's prompt.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptConfig {
    /// Draw the template in place of the game's prompt.
    #[serde(default)]
    pub draw: bool,
    /// The design Vosh draws.
    #[serde(default)]
    pub template: String,
    /// At most two designs the card opened with, newest first, so trying
    /// a preset never loses the one you had.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_templates: Vec<String>,
    /// How Vosh reads the game's prompt. None reads nothing and hides
    /// nothing.
    #[serde(
        default,
        skip_serializing_if = "CaptureConfig::is_none",
        deserialize_with = "lenient_capture"
    )]
    pub capture: CaptureConfig,
    /// Where your prompt shows: in the text as the game sends it, lifted
    /// on a band in the text, or pinned above the command line. A table
    /// that keeps the text leaves the key out.
    #[serde(
        default,
        skip_serializing_if = "PromptShow::is_text",
        deserialize_with = "lenient_show"
    )]
    pub show: PromptShow,
}

/// `[prompt] show`, where your prompt shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PromptShow {
    /// In the text, where the game sends it. Today's behavior.
    #[default]
    Text,
    /// Every prompt stays in the text on a raised band.
    Lifted,
    /// Prompts leave the text, and the latest shows on a band above the
    /// command line. They are still logged and still reach Prompts
    /// triggers.
    Pinned,
}

impl PromptShow {
    pub fn is_text(&self) -> bool {
        *self == Self::Text
    }

    /// The name the table and the Settings bridge use.
    pub fn name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Lifted => "lifted",
            Self::Pinned => "pinned",
        }
    }

    /// The value `name` stands for, or None for a name this build does
    /// not know.
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "text" => Some(Self::Text),
            "lifted" => Some(Self::Lifted),
            "pinned" => Some(Self::Pinned),
            _ => None,
        }
    }
}

impl PromptConfig {
    /// The table a file with no `[prompt]` stands for, from the two
    /// `[ui]` keys older builds wrote.
    pub fn from_legacy(enabled: bool, template: &str) -> Self {
        Self {
            draw: enabled,
            template: template.to_string(),
            ..Self::default()
        }
    }

    /// The table a fresh profile starts with: Vosh's default design,
    /// [`DEFAULT_DESIGN`], with drawing off and your prompt in the text,
    /// so turning drawing on draws it.
    pub fn fresh() -> Self {
        Self {
            template: DEFAULT_DESIGN.to_string(),
            ..Self::default()
        }
    }

    /// True when the table says nothing a default one does not, so a
    /// profile file leaves it out.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The card opened on the saved design. When it differs from the
    /// newest earlier design, it goes first and the oldest beyond
    /// [`PREVIOUS_TEMPLATES`] drops. An empty design is never kept.
    /// Returns whether the list changed.
    pub fn note_opened(&mut self) -> bool {
        if self.template.is_empty() || self.previous_templates.first() == Some(&self.template) {
            return false;
        }
        self.previous_templates.insert(0, self.template.clone());
        self.previous_templates.truncate(PREVIOUS_TEMPLATES);
        true
    }

    /// Put Vosh's default design, [`DEFAULT_DESIGN`], in place of the one
    /// you have. Yours goes first among the earlier designs, as when the
    /// card opens, so the card can offer it back. The switch, the place
    /// and the capture stay. Returns false when the design already is the
    /// default.
    pub fn use_default_design(&mut self) -> bool {
        if self.template == DEFAULT_DESIGN {
            return false;
        }
        self.note_opened();
        self.template = DEFAULT_DESIGN.to_string();
        true
    }

    /// Take the switch and the template a Settings save carries, each
    /// only when it differs from what this table holds, so a save that
    /// carries them unchanged leaves the table alone. Turning drawing on
    /// with no design takes Vosh's default, [`DEFAULT_DESIGN`]. Returns
    /// whether either changed.
    pub fn take_switch_and_template(&mut self, draw: bool, template: &str) -> bool {
        let turned_on = draw && !self.draw;
        let mut changed = false;
        if self.draw != draw {
            self.draw = draw;
            changed = true;
        }
        if self.template != template {
            self.template = template.to_string();
            changed = true;
        }
        if turned_on && self.template.is_empty() {
            self.template = DEFAULT_DESIGN.to_string();
        }
        changed
    }

    /// Take where your prompt shows from a Settings save, when it differs
    /// from what this table holds. A name this build does not know
    /// changes nothing. Returns whether it changed.
    pub fn take_show(&mut self, show: &str) -> bool {
        match PromptShow::parse(show) {
            Some(show) if show != self.show => {
                self.show = show;
                true
            }
            _ => false,
        }
    }
}

/// `[prompt.capture]`, how Vosh reads the game's prompt. The `kind` key
/// picks the variant.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CaptureConfig {
    /// Vosh reads nothing and hides nothing. A table this build cannot
    /// read, a kind it does not know among them, reads as none too.
    #[default]
    None,
    /// Aabahran's PROMPT codes, which Vosh compiles into patterns.
    Aabahran(AabahranCapture),
    /// Patterns you pointed at or wrote, or an old capture trigger's.
    Regex(RegexCapture),
}

impl CaptureConfig {
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// The capture reads Aabahran's codes, which makes the Forsaken Lands
    /// rules hold on any host (D17).
    pub fn is_aabahran(&self) -> bool {
        matches!(self, Self::Aabahran(_))
    }

    /// The capture is still the pattern the move from a capture trigger
    /// wrote, `kind = "regex"` with `source = "migrated"`. A pattern you
    /// set with `#prompt {regex}` has source typed, and `#unprompt` leaves
    /// none, so neither counts. The first PROMPT the game shows under the
    /// Forsaken Lands rules switches it to Aabahran's codes (D10).
    pub fn is_migrated(&self) -> bool {
        matches!(self, Self::Regex(capture) if capture.source == Some(CaptureSource::Migrated))
    }
}

/// The game's PROMPT and fight prompt settings, as the game stores them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AabahranCapture {
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub fprompt: String,
    /// Take the settings the game sends when you change them.
    #[serde(default = "default_true")]
    pub follow_game: bool,
    /// When Vosh learned these settings, as RFC 3339 local time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seen_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<CaptureSource>,
}

impl Default for AabahranCapture {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            fprompt: String::new(),
            follow_game: true,
            seen_at: None,
            source: None,
        }
    }
}

/// Patterns, one per line of the prompt, top line first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegexCapture {
    #[serde(default)]
    pub lines: Vec<String>,
    /// A partial the last line matches is the prompt at once, without
    /// waiting for a line end, GA or EOR. `capture::settle` works it out
    /// from the last line.
    #[serde(default)]
    pub settle: bool,
    /// The variable each group feeds, keyed by the group's name or, for a
    /// group with no name, its number. Only groups that differ from their
    /// own name are listed. A group mapped to an empty name is left out.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub names: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seen_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<CaptureSource>,
}

/// Where the capture came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptureSource {
    /// Char.Prompt.
    Gmcp,
    /// What the game printed after one of your sends this session.
    Session,
    /// Your log.
    Log,
    /// You typed or pasted it.
    Typed,
    /// Moved from a capture trigger.
    Migrated,
}

fn default_true() -> bool {
    true
}

/// Read `[prompt.capture]`, or none when it does not read, so a capture
/// table a newer build or a hand edit wrote never keeps the rest of the
/// profile file from loading.
fn lenient_capture<'de, D>(deserializer: D) -> Result<CaptureConfig, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(raw).unwrap_or_default())
}

/// Read `[prompt] show`, or the text for a value this build does not
/// know, so a newer file never fails to load.
fn lenient_show<'de, D>(deserializer: D) -> Result<PromptShow, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = serde_json::Value::deserialize(deserializer)?;
    Ok(raw.as_str().and_then(PromptShow::parse).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    const JAMES: &str = "%{c:100,100,100}[%c_reset%s_italic%hp(%c_hp%pct_hp%c_reset%s_italic%)h %mana(%{c:128,200,255}%pct_mana%c_reset%s_italic%)m %move(%{c:200,255,23}%pct_move%c_reset%s_italic%)v%c_reset%{c:100,100,100}] %c_reset";

    #[test]
    fn a_default_table_reads_from_nothing() {
        let config: PromptConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config, PromptConfig::default());
        assert!(config.is_default());
        assert!(!config.draw);
        assert!(config.capture.is_none());
    }

    #[test]
    fn the_legacy_keys_become_the_switch_and_the_template() {
        let config = PromptConfig::from_legacy(true, JAMES);
        assert!(config.draw);
        assert_eq!(config.template, JAMES);
        assert!(config.previous_templates.is_empty());
        assert!(config.capture.is_none());
        assert!(!config.is_default());
        assert!(PromptConfig::from_legacy(false, "").is_default());
    }

    #[test]
    fn a_regex_capture_round_trips_with_its_kind() {
        let config = PromptConfig {
            draw: true,
            template: JAMES.to_string(),
            previous_templates: vec!["%hp".into()],
            capture: CaptureConfig::Regex(RegexCapture {
                lines: vec![r"\[(?<hp>\d+)/(?<maxhp>\d+)hp\]".into()],
                settle: false,
                names: BTreeMap::from([("1".to_string(), "gold".to_string())]),
                seen_at: None,
                source: Some(CaptureSource::Migrated),
            }),
            show: PromptShow::Text,
        };
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["capture"]["kind"], "regex");
        assert_eq!(json["capture"]["source"], "migrated");
        assert_eq!(json["capture"]["settle"], false);
        let back: PromptConfig = serde_json::from_value(json).unwrap();
        assert_eq!(back, config);
    }

    #[test]
    fn an_aabahran_capture_follows_the_game_unless_it_says_not_to() {
        let config: PromptConfig = serde_json::from_str(
            r#"{"draw":true,"template":"%hp","capture":{"kind":"aabahran","prompt":"%n%P%C[%h/%Hhp]%c","source":"gmcp","seen_at":"2026-09-29T12:58:02-05:00"}}"#,
        )
        .unwrap();
        let CaptureConfig::Aabahran(capture) = &config.capture else {
            panic!("an aabahran capture, got {:?}", config.capture);
        };
        assert!(capture.follow_game);
        assert_eq!(capture.fprompt, "");
        assert_eq!(capture.source, Some(CaptureSource::Gmcp));
        assert!(config.capture.is_aabahran());
    }

    #[test]
    fn only_the_pattern_the_move_wrote_counts_as_migrated() {
        // As the move writes it into a profile file.
        let config: PromptConfig = serde_json::from_str(
            r#"{"draw":true,"template":"%hp","capture":{"kind":"regex","lines":["\\[(?<hp>\\d+)/(?<maxhp>\\d+)hp\\]"],"settle":false,"source":"migrated"}}"#,
        )
        .unwrap();
        assert!(config.capture.is_migrated());

        let regex = |source| {
            CaptureConfig::Regex(RegexCapture {
                lines: vec![r"\[(?<hp>\d+)hp\]".into()],
                source,
                ..RegexCapture::default()
            })
        };
        // #prompt {regex} writes source typed.
        assert!(!regex(Some(CaptureSource::Typed)).is_migrated());
        assert!(!regex(Some(CaptureSource::Gmcp)).is_migrated());
        assert!(!regex(None).is_migrated());
        // #unprompt leaves none.
        assert!(!CaptureConfig::None.is_migrated());
        assert!(!CaptureConfig::Aabahran(AabahranCapture {
            source: Some(CaptureSource::Migrated),
            ..AabahranCapture::default()
        })
        .is_migrated());
    }

    #[test]
    fn a_capture_this_build_cannot_read_is_none_and_keeps_the_rest() {
        for capture in [
            r#"{"kind":"telepathy","lines":["x"]}"#,
            r#"{"kind":"regex","lines":"not a list"}"#,
            r#"{"kind":"aabahran","source":"a carrier pigeon"}"#,
            r#"{"lines":["no kind"]}"#,
        ] {
            let text = format!(r#"{{"draw":true,"template":"%hp","capture":{capture}}}"#);
            let config: PromptConfig = serde_json::from_str(&text).unwrap();
            assert!(config.draw, "{capture}");
            assert_eq!(config.template, "%hp", "{capture}");
            assert!(config.capture.is_none(), "{capture}");
        }
    }

    #[test]
    fn none_leaves_the_capture_and_empty_lists_out() {
        let config = PromptConfig::from_legacy(true, "%hp");
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json, serde_json::json!({"draw": true, "template": "%hp"}));
    }

    #[test]
    fn opening_the_card_keeps_the_design_it_found() {
        let mut config = PromptConfig::from_legacy(true, JAMES);
        assert!(config.note_opened());
        assert_eq!(config.previous_templates, [JAMES]);
        // Opening again on the same design changes nothing.
        assert!(!config.note_opened());
        assert_eq!(config.previous_templates, [JAMES]);

        // You tried Percent and closed the card. Both designs survive.
        config.template = "hp %pct_hp%%".into();
        assert!(config.note_opened());
        assert_eq!(config.previous_templates, ["hp %pct_hp%%", JAMES]);

        // A third design pushes out the oldest.
        config.template = "%hp".into();
        assert!(config.note_opened());
        assert_eq!(config.previous_templates, ["%hp", "hp %pct_hp%%"]);
    }

    #[test]
    fn an_empty_design_is_never_kept() {
        let mut config = PromptConfig::default();
        assert!(!config.note_opened());
        assert!(config.previous_templates.is_empty());
    }

    #[test]
    fn a_settings_save_takes_only_what_changed() {
        let capture = CaptureConfig::Regex(RegexCapture {
            lines: vec!["x".into()],
            ..RegexCapture::default()
        });
        let newer = PromptConfig {
            draw: true,
            template: "%hp".into(),
            previous_templates: vec![JAMES.into()],
            capture: capture.clone(),
            show: PromptShow::Pinned,
        };
        let mut config = newer.clone();
        assert!(!config.take_switch_and_template(true, "%hp"));
        assert_eq!(config, newer);

        assert!(config.take_switch_and_template(false, "%hp"));
        assert!(!config.draw);
        assert_eq!(config.template, "%hp");
        assert_eq!(config.previous_templates, [JAMES]);
        assert_eq!(config.capture, capture);

        assert!(config.take_switch_and_template(false, "%mana"));
        assert_eq!(config.template, "%mana");
        assert_eq!(config.capture, capture);
    }

    #[test]
    fn turning_drawing_on_with_no_design_takes_the_default() {
        let mut config = PromptConfig::default();
        assert!(config.take_switch_and_template(true, ""));
        assert!(config.draw);
        assert_eq!(config.template, DEFAULT_DESIGN);
        assert!(config.previous_templates.is_empty());

        // The same save the window sends again changes nothing.
        let mut again = config.clone();
        assert!(!again.take_switch_and_template(true, DEFAULT_DESIGN));
        assert_eq!(again, config);

        // A design of your own stays as it is.
        let mut yours = PromptConfig::from_legacy(false, JAMES);
        assert!(yours.take_switch_and_template(true, JAMES));
        assert_eq!(yours.template, JAMES);
    }

    #[test]
    fn a_fresh_profile_holds_the_default_design_and_draws_nothing_yet() {
        let fresh = PromptConfig::fresh();
        assert!(!fresh.draw);
        assert_eq!(fresh.template, DEFAULT_DESIGN);
        assert!(fresh.previous_templates.is_empty());
        assert!(fresh.capture.is_none());
        assert_eq!(fresh.show, PromptShow::Text);
        // The file keeps it from the first save.
        assert!(!fresh.is_default());
        let json = serde_json::to_value(&fresh).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"draw": false, "template": DEFAULT_DESIGN})
        );
        let back: PromptConfig = serde_json::from_value(json).unwrap();
        assert_eq!(back, fresh);

        // Turning drawing on draws it.
        let mut on = fresh.clone();
        assert!(on.take_switch_and_template(true, DEFAULT_DESIGN));
        assert_eq!(on.template, DEFAULT_DESIGN);
    }

    #[test]
    fn the_default_design_takes_the_place_of_yours_and_keeps_it() {
        let mut config = PromptConfig {
            show: PromptShow::Pinned,
            ..PromptConfig::from_legacy(true, JAMES)
        };
        assert!(config.use_default_design());
        assert_eq!(config.template, DEFAULT_DESIGN);
        // Yours goes first among the earlier designs, for the card to
        // offer back. The switch and the place stay.
        assert_eq!(config.previous_templates, [JAMES]);
        assert!(config.draw);
        assert_eq!(config.show, PromptShow::Pinned);

        // Again changes nothing.
        assert!(!config.use_default_design());
        assert_eq!(config.previous_templates, [JAMES]);

        // The oldest of two earlier designs drops.
        let mut config = PromptConfig {
            previous_templates: vec!["%hp".into(), "%mana".into()],
            ..PromptConfig::from_legacy(false, "%move")
        };
        assert!(config.use_default_design());
        assert_eq!(config.previous_templates, ["%move", "%hp"]);
        assert!(!config.draw);

        // A design already first among them is not kept twice.
        let mut config = PromptConfig {
            previous_templates: vec![JAMES.into()],
            ..PromptConfig::from_legacy(true, JAMES)
        };
        assert!(config.use_default_design());
        assert_eq!(config.previous_templates, [JAMES]);

        // An empty design is never kept.
        let mut config = PromptConfig::default();
        assert!(config.use_default_design());
        assert_eq!(config.template, DEFAULT_DESIGN);
        assert!(config.previous_templates.is_empty());
    }

    #[test]
    fn only_turning_drawing_on_fills_an_empty_design() {
        // Drawing already on: clearing the field to type a new design
        // leaves it empty.
        let mut config = PromptConfig::from_legacy(true, "%hp");
        assert!(config.take_switch_and_template(true, ""));
        assert_eq!(config.template, "");
        assert!(!config.take_switch_and_template(true, ""));
        assert_eq!(config.template, "");

        // Drawing off: an empty design stays empty until you turn it on.
        let mut config = PromptConfig::from_legacy(true, "%hp");
        assert!(config.take_switch_and_template(false, ""));
        assert_eq!(config.template, "");
        assert!(config.take_switch_and_template(true, ""));
        assert_eq!(config.template, DEFAULT_DESIGN);
    }

    #[test]
    fn where_your_prompt_shows_defaults_to_the_text_and_writes_nothing() {
        let config: PromptConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config.show, PromptShow::Text);
        let json = serde_json::to_value(PromptConfig::from_legacy(true, "%hp")).unwrap();
        assert!(json.get("show").is_none(), "{json}");
        // A table that holds only the default stays out of the file.
        assert!(PromptConfig::default().is_default());
    }

    #[test]
    fn each_place_your_prompt_shows_round_trips() {
        for show in [PromptShow::Text, PromptShow::Lifted, PromptShow::Pinned] {
            let config = PromptConfig {
                show,
                ..PromptConfig::from_legacy(true, "%hp")
            };
            let json = serde_json::to_value(&config).unwrap();
            if show.is_text() {
                assert!(json.get("show").is_none());
            } else {
                assert_eq!(json["show"], show.name());
            }
            let back: PromptConfig = serde_json::from_value(json).unwrap();
            assert_eq!(back, config);
            assert!(!back.is_default());
        }
        let lifted = PromptConfig {
            show: PromptShow::Lifted,
            ..PromptConfig::default()
        };
        assert!(!lifted.is_default());
    }

    #[test]
    fn a_place_this_build_does_not_know_reads_as_the_text() {
        for show in [r#""floating""#, "3", "true", r#"{"where":"up"}"#] {
            let text = format!(r#"{{"draw":true,"template":"%hp","show":{show}}}"#);
            let config: PromptConfig = serde_json::from_str(&text).unwrap();
            assert_eq!(config.show, PromptShow::Text, "{show}");
            assert_eq!(config.template, "%hp", "{show}");
        }
        let config: PromptConfig = serde_json::from_str(r#"{"show":"Pinned"}"#).unwrap();
        assert_eq!(config.show, PromptShow::Pinned);
    }

    #[test]
    fn a_settings_save_takes_where_your_prompt_shows_only_when_it_differs() {
        let mut config = PromptConfig::from_legacy(true, "%hp");
        assert!(!config.take_show("text"));
        assert!(config.take_show("pinned"));
        assert_eq!(config.show, PromptShow::Pinned);
        assert!(!config.take_show("pinned"));
        assert!(!config.take_show("sideways"));
        assert_eq!(config.show, PromptShow::Pinned);
        assert!(config.take_show("lifted"));
        assert_eq!(config.show, PromptShow::Lifted);
        assert_eq!(config.template, "%hp");
    }
}
