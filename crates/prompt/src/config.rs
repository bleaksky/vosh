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

    /// Take the switch and the template a Settings save carries, each
    /// only when it differs from what this table holds, so a save that
    /// carries them unchanged leaves the table alone. Returns whether
    /// either changed.
    pub fn take_switch_and_template(&mut self, draw: bool, template: &str) -> bool {
        let mut changed = false;
        if self.draw != draw {
            self.draw = draw;
            changed = true;
        }
        if self.template != template {
            self.template = template.to_string();
            changed = true;
        }
        changed
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
}
