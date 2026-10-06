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
//!
//! A profile with no design of its own follows the game. Its design is
//! Same as the game for your PROMPT and fight prompt codes, written again
//! each time they change, until you change it (`mirror`). A fresh table
//! follows the game, and so does one from a file that does not say
//! otherwise and holds [`DEFAULT_DESIGN`], one of the [`RETIRED_DEFAULTS`],
//! or Same as the game for its codes, as an older build saves a design
//! that followed the game. A profile file keeps the design in `template`
//! too, so an older build draws it, and writes `mirror` only when the
//! design alone does not say it (see [`file_table`]).

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

use crate::aabahran::Who;

/// Vosh's default, the design `#prompt default` and the Vosh's default
/// start put in place. A profile with no design of its own follows the
/// game instead. It mirrors the pinned band of the gallery mockup, with
/// the tank in place of your opponent, as James asked on 2026-09-30.
///
/// Out of a fight it draws one row: your health, mana and moves as
/// current over max, or current alone when nothing gives the max, then
/// the exits in brackets and your gold. In a fight a row comes first
/// with the tank's name, a colon and the tank's health as a gauge ten
/// cells wide, the gauge the mockup gave your opponent. Solo you are the
/// tank, so it names you.
///
/// Only your three numbers and the gauge carry color, by one scale:
/// green above two thirds, yellow above one third, red below. Every
/// label, max and bracket is 256 color 245, a middle gray, and the
/// tank's name keeps the terminal's color. Since the design reads the
/// tank, the game's tank line folds into the tank row, so the prompt
/// takes two rows at most. It ends in a space, as the game's own prompt
/// does, so your echo never touches it.
pub const DEFAULT_DESIGN: &str = concat!(
    // The tank row, only in a fight and only when Vosh knows the tank.
    // The gauge draws only when it knows the tank's health, hidden
    // included, so a PROMPT with %n and no %P never prints it as typed.
    "%{if:fight}%{if:tank}%tank:%{if:tank_hp} %{tank_hp:bar:10}%{end}%nl%{end}%{end}",
    // Your vitals, each current over max and colored by how full, or
    // current alone in the terminal's color when nothing gives the max.
    // One that does not apply, such as mana for a class with none on
    // another game, draws nothing.
    "%{if:hp}%{if:maxhp}%c_hp%{end}%hp%{c:245}%{if:maxhp}/%{maxhp}%{end}hp%c_default%{end}",
    "%{if:mana} %{if:maxmana}%c_mana%{end}%mana%{c:245}%{if:maxmana}/%{maxmana}%{end}mn%c_default%{end}",
    "%{if:move} %{if:maxmove}%c_move%{end}%move%{c:245}%{if:maxmove}/%{maxmove}%{end}mv%c_default%{end}",
    "%{if:exits}  %{c:245}[%c_default%exits%{c:245}]%c_default%{end}",
    "%{if:gold}  %{gold:grouped}%{c:245}g%c_default%{end}",
    " ",
);

/// Vosh's vitals text, the one the Text vitals style starts from and
/// Reset to default puts back (board 5 of the Vitals Styles review). In
/// a fight a row comes first with your opponent and its health on the
/// right. Then your health, mana and moves, each current over max.
pub const DEFAULT_VITALS_TEXT: &str = concat!(
    "%{if:fight}%opponent%{right}%c_yellow%{opponent_hp:pct}%%%c_default%nl%{end}",
    "%{c:hp:game}%hp%c_gray/%{maxhp}hp%c_default ",
    "%mana%c_gray/%{maxmana}mn%c_default ",
    "%move%c_gray/%{maxmove}mv%c_default",
);

/// At a glance, Vosh's default from 2026-09-30 until James asked for the
/// mockup's band. In a fight its top row named your opponent with a
/// gauge, the percent and the game's condition words, then the tank in a
/// group. Its vitals row added your position and language, Wizi and
/// Incog, and the tracked affects you were missing.
pub(crate) const AT_A_GLANCE: &str = concat!(
    // The fight row.
    "%{if:fight}",
    "%opponent %{opponent_hp:bar:10} %{opponent_hp:pct}%% ",
    "%{c:245}%opponent_cond%c_default",
    // Alone you are always the tank, and Group.Info is {}.
    "%{if:group_size}%{if:tank}  %{c:245}tank %c_tank_hp%tank%c_default%{end}%{end}",
    "%nl%{end}",
    // Your vitals, each current over max and colored by how full, or
    // current alone in the terminal's color when nothing gives the max.
    // One that does not apply, such as mana for a class with none on
    // another game, draws nothing.
    "%{if:hp}%{if:maxhp}%c_hp%{end}%hp%{c:245}%{if:maxhp}/%{maxhp}%{end}hp%c_default%{end}",
    "%{if:mana} %{if:maxmana}%c_mana%{end}%mana%{c:245}%{if:maxmana}/%{maxmana}%{end}mn%c_default%{end}",
    "%{if:move} %{if:maxmove}%c_move%{end}%move%{c:245}%{if:maxmove}/%{maxmove}%{end}mv%c_default%{end}",
    // Position and language, one space apart when both show.
    "%{if:pos}  %{c:245}%pos%c_default%{end}",
    "%{if:lang}%{ifnot:pos} %{end} %{c:245}%lang%c_default%{end}",
    "%{if:exits}  %{c:245}[%c_default%exits%{c:245}]%c_default%{end}",
    "%{if:gold}  %{gold:grouped}%{c:245}g%c_default%{end}",
    // Wizi and Incog show only when the game prints the immortal
    // prefix, and only out of a fight.
    "%{ifnot:fight}",
    "%{if:wizi}  %{c:245}wizi %wizi%c_default%{end}",
    "%{if:incog}%{ifnot:wizi} %{end} %{c:245}incog %incog%c_default%{end}",
    "%{end}",
    "%{if:missing}  %{c:245}missing %c_yellow%{missing:names}%c_default%{end}",
    " ",
);

/// At a glance as it first shipped, before the review guarded each vital.
const AT_A_GLANCE_FIRST: &str = concat!(
    "%{if:fight}",
    "%opponent %{opponent_hp:bar:10} %{opponent_hp:pct}%% ",
    "%{c:245}%opponent_cond%c_default",
    "%{if:group_size}%{if:tank}  %{c:245}tank %c_tank_hp%tank%c_default%{end}%{end}",
    "%nl%{end}",
    "%c_hp%hp%{c:245}/%{maxhp}hp%c_default ",
    "%c_mana%mana%{c:245}/%{maxmana}mn%c_default ",
    "%c_move%move%{c:245}/%{maxmove}mv%c_default",
    "%{if:pos}  %{c:245}%pos%c_default%{end}",
    "%{if:lang}%{ifnot:pos} %{end} %{c:245}%lang%c_default%{end}",
    "%{if:exits}  %{c:245}[%c_default%exits%{c:245}]%c_default%{end}",
    "%{if:gold}  %{gold:grouped}%{c:245}g%c_default%{end}",
    "%{ifnot:fight}",
    "%{if:wizi}  %{c:245}wizi %wizi%c_default%{end}",
    "%{if:incog}%{ifnot:wizi} %{end} %{c:245}incog %incog%c_default%{end}",
    "%{end}",
    "%{if:missing}  %{c:245}missing %c_yellow%{missing:names}%c_default%{end}",
    " ",
);

/// The designs earlier builds shipped as Vosh's default, byte for byte.
/// You only ever got one from Vosh, at a fresh profile, by turning
/// drawing on with no design, or with `#prompt default`, so a profile
/// that holds one counts as having no design of its own and follows the
/// game when it loads ([`PromptConfig::counts_as_no_design`]).
pub const RETIRED_DEFAULTS: [&str; 2] = [AT_A_GLANCE, AT_A_GLANCE_FIRST];

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
    /// The design follows the game. Vosh writes it from your PROMPT and
    /// fight prompt codes, as Same as the game, each time they change,
    /// and keeps it empty while the profile reads no codes, so you see
    /// the game's own prompt. Any edit makes the design yours. The page
    /// hears it as `mirror`, left out while false. A profile file writes
    /// it only when the design does not say it (see [`file_table`]).
    #[serde(
        default,
        skip_serializing_if = "is_false",
        deserialize_with = "lenient_bool"
    )]
    pub mirror: bool,
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
    pub(crate) fn is_text(&self) -> bool {
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

    /// The table a fresh profile starts with: drawing off, your prompt
    /// in the text, and a design that follows the game, so turning
    /// drawing on draws your prompt as the game does.
    pub fn fresh() -> Self {
        Self {
            mirror: true,
            ..Self::default()
        }
    }

    /// True when the table says nothing a file with no `[prompt]` does
    /// not, so a profile file leaves it out. Such a file follows the game
    /// with an empty design, and with no codes, no design and drawing off
    /// following the game or not draws the same, so `mirror` does not
    /// count.
    pub fn is_default(&self) -> bool {
        Self {
            mirror: false,
            ..self.clone()
        } == Self::default()
    }

    /// A design that counts as none of your own: empty, Vosh's default,
    /// or a default an earlier build shipped. A profile file that holds
    /// one and does not say otherwise follows the game.
    pub fn counts_as_no_design(template: &str) -> bool {
        template.is_empty() || template == DEFAULT_DESIGN || RETIRED_DEFAULTS.contains(&template)
    }

    /// Write the design again from the codes while it follows the game,
    /// for `who`. Returns whether the design changed. A design of yours
    /// stays as it is.
    pub fn mirror_game(&mut self, who: Who) -> bool {
        if !self.mirror {
            return false;
        }
        let design = game_design(&self.capture, who);
        if design == self.template {
            return false;
        }
        self.template = design;
        true
    }

    /// Follow the game from now on, as turning drawing on with no design
    /// and picking Same as the game do. The design is written from the
    /// codes for `who`.
    pub fn follow_game(&mut self, who: Who) {
        self.mirror = true;
        self.mirror_game(who);
    }

    /// The card opened on the saved design. When it differs from the
    /// newest earlier design, it goes first and the oldest beyond
    /// [`PREVIOUS_TEMPLATES`] drops. An empty design is never kept, and
    /// neither is one that follows the game, since Same as the game
    /// brings it back. Returns whether the list changed.
    pub fn note_opened(&mut self) -> bool {
        if self.mirror
            || self.template.is_empty()
            || self.previous_templates.first() == Some(&self.template)
        {
            return false;
        }
        self.previous_templates.insert(0, self.template.clone());
        self.previous_templates.truncate(PREVIOUS_TEMPLATES);
        true
    }

    /// Put Vosh's default design, [`DEFAULT_DESIGN`], in place of the one
    /// you have, as your choice, so it stops following the game. Yours
    /// goes first among the earlier designs, as when the card opens, so
    /// the card can offer it back. The switch, the place and the capture
    /// stay. Returns false when the design already is the default you
    /// chose.
    pub fn use_default_design(&mut self) -> bool {
        if self.template == DEFAULT_DESIGN && !self.mirror {
            return false;
        }
        self.note_opened();
        self.template = DEFAULT_DESIGN.to_string();
        self.mirror = false;
        true
    }
}

/// The design that follows the game for `capture`: Same as the game for
/// Aabahran's codes when they compile, as the card's start list offers
/// it, or empty, which draws nothing in place of the game's own prompt.
fn game_design(capture: &CaptureConfig, who: Who) -> String {
    let CaptureConfig::Aabahran(codes) = capture else {
        return String::new();
    };
    crate::card::presets::game(&codes.prompt, &codes.fprompt, who).unwrap_or_default()
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

    /// The game the engine plays decides this capture: Aabahran's codes
    /// while they follow the game, or the pattern the move from a capture
    /// trigger wrote, which the first PROMPT the game shows replaces (D10).
    pub(crate) fn game_decides(&self) -> bool {
        match self {
            Self::Aabahran(codes) => codes.follow_game,
            Self::Regex(_) => self.is_migrated(),
            Self::None => false,
        }
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

// serde's skip_serializing_if hands the field by reference.
fn is_false(on: &bool) -> bool {
    !*on
}

/// Read a switch, or off for a value that is not one, so a hand edit
/// never keeps the profile file from loading.
fn lenient_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = serde_json::Value::deserialize(deserializer)?;
    Ok(raw.as_bool().unwrap_or(false))
}

/// The `[prompt]` table as a profile file holds it, for a field of type
/// `Option<PromptConfig>` with `#[serde(with = "...")]`.
///
/// The file writes `mirror` only when the design alone does not say it.
/// A file with no `mirror` follows the game when its design counts as
/// none of your own ([`PromptConfig::counts_as_no_design`]), as a profile
/// an earlier build saved with Vosh's default does, or when the design is
/// Same as the game for the file's codes for a mortal in your own body.
/// An older build drops `mirror` when it saves, so a design that followed
/// the game keeps following it, and so does one an older build started
/// from Same as the game. The file writes true for a design that follows
/// the game but reads as neither, such as one written for an immortal,
/// and false for a design you chose that reads as one, such as Vosh's
/// default from `#prompt default`, an empty design from Start empty, or
/// a design of yours equal to Same as the game for your codes. A file
/// read here holds the design written from its codes for a mortal in
/// your own body, which the live prompt writes again for who you are.
///
/// Every other key reads and writes as the table always has, so an older
/// build reads the file and draws the design in `template`.
pub mod file_table {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::{game_design, CaptureConfig, PromptConfig, PromptShow};
    use crate::aabahran::Who;

    /// Whether a file that holds this design and these codes with no
    /// `mirror` follows the game: the design counts as none of your own,
    /// or it is Same as the game for the codes, for a mortal in your own
    /// body.
    fn follows_unsaid(template: &str, capture: &CaptureConfig) -> bool {
        PromptConfig::counts_as_no_design(template)
            || template == game_design(capture, Who::default())
    }

    /// The table as it is written, key by key in the order the derived
    /// form writes them, with `mirror` last.
    #[derive(Serialize)]
    struct Written<'a> {
        draw: bool,
        template: &'a str,
        #[serde(skip_serializing_if = "<[String]>::is_empty")]
        previous_templates: &'a [String],
        #[serde(skip_serializing_if = "CaptureConfig::is_none")]
        capture: &'a CaptureConfig,
        #[serde(skip_serializing_if = "PromptShow::is_text")]
        show: PromptShow,
        #[serde(skip_serializing_if = "Option::is_none")]
        mirror: Option<bool>,
    }

    impl<'a> Written<'a> {
        fn of(config: &'a PromptConfig) -> Self {
            let PromptConfig {
                draw,
                template,
                previous_templates,
                capture,
                show,
                mirror,
            } = config;
            let said = follows_unsaid(template, capture);
            Self {
                draw: *draw,
                template,
                previous_templates,
                capture,
                show: *show,
                mirror: (*mirror != said).then_some(*mirror),
            }
        }
    }

    pub fn serialize<S: Serializer>(table: &Option<PromptConfig>, s: S) -> Result<S::Ok, S::Error> {
        match table {
            Some(config) => Written::of(config).serialize(s),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<PromptConfig>, D::Error> {
        let raw = serde_json::Value::deserialize(d)?;
        let said = raw.get("mirror").and_then(serde_json::Value::as_bool);
        let mut config: PromptConfig =
            serde_json::from_value(raw).map_err(serde::de::Error::custom)?;
        config.mirror = said.unwrap_or_else(|| follows_unsaid(&config.template, &config.capture));
        config.mirror_game(Who::default());
        Ok(Some(config))
    }
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
    use crate::testkit::designs::JAMES;

    #[test]
    fn the_vitals_text_draws_your_vitals_and_your_opponent_in_a_fight() {
        use crate::render::{render_str, RenderOptions};
        use crate::values::overrides::{Overridden, Overrides, Preview};
        use crate::values::Samples;

        let now = chrono::NaiveDate::from_ymd_opt(2026, 10, 5)
            .unwrap()
            .and_hms_opt(21, 40, 0)
            .unwrap();
        let samples = Samples { now };
        let draw = |over: &Overrides| {
            let values = Overridden::new(&samples, over, now);
            render_str(DEFAULT_VITALS_TEXT, &values, RenderOptions::default()).plain
        };
        let calm = Overrides {
            values: [("fight".to_string(), false.into())].into(),
            lament: false,
        };
        assert_eq!(draw(&calm), "1020/1020hp 800/800mn 930/930mv");
        assert_eq!(
            draw(&Preview::Fight.overrides(&samples)),
            "Blackwatch Guard 60%\n1020/1020hp 800/800mn 930/930mv"
        );
    }

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
        let leftover = &config.previous_templates;
        assert!(leftover.is_empty(), "{leftover:?}");
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
            mirror: false,
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
        let leftover = &config.previous_templates;
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    /// Your PROMPT setting from the test kit, read from the codes the
    /// game sent.
    fn codes(prompt: &str, fprompt: &str) -> CaptureConfig {
        CaptureConfig::Aabahran(AabahranCapture {
            prompt: prompt.into(),
            fprompt: fprompt.into(),
            source: Some(CaptureSource::Gmcp),
            ..AabahranCapture::default()
        })
    }

    /// Same as the game for `prompt` with no fight prompt, for a mortal.
    fn same_as_the_game(prompt: &str) -> String {
        crate::card::presets::game(prompt, "", Who::default()).expect("the codes compile")
    }

    #[test]
    fn a_fresh_profile_follows_the_game_and_draws_nothing_yet() {
        let fresh = PromptConfig::fresh();
        assert!(!fresh.draw);
        assert!(fresh.mirror);
        // No codes yet, so no design and the game's own prompt.
        assert_eq!(fresh.template, "");
        let leftover = &fresh.previous_templates;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert!(fresh.capture.is_none());
        assert_eq!(fresh.show, PromptShow::Text);
        // A file with no [prompt] says the same, so the file leaves it out.
        assert!(fresh.is_default());
        let json = serde_json::to_value(&fresh).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"draw": false, "template": "", "mirror": true})
        );
        let back: PromptConfig = serde_json::from_value(json).unwrap();
        assert_eq!(back, fresh);
    }

    #[test]
    fn a_design_that_follows_the_game_is_same_as_the_game_for_your_codes() {
        let prompt = crate::testkit::mud::PROMPT;
        let mut config = PromptConfig {
            capture: codes(prompt, ""),
            ..PromptConfig::fresh()
        };
        assert!(config.mirror_game(Who::default()));
        assert_eq!(config.template, same_as_the_game(prompt));
        assert_eq!(
            Some(config.template.as_str()),
            crate::card::presets::same_as_the_game(prompt, "", Who::default()).as_deref()
        );
        // Again changes nothing.
        assert!(!config.mirror_game(Who::default()));

        // New codes write it again, a fight prompt included.
        let all = crate::testkit::mud::PROMPT_ALL;
        config.capture = codes(all, "`1%h``hp [%p] > ");
        assert!(config.mirror_game(Who::default()));
        let both = crate::card::presets::game(all, "`1%h``hp [%p] > ", Who::default());
        assert_eq!(Some(config.template.clone()), both);
        assert!(
            config.template.contains("%{if:fight}"),
            "{}",
            config.template
        );

        // With no codes, or a pattern of another game, it is empty, so
        // you see the game's own prompt.
        config.capture = CaptureConfig::None;
        assert!(config.mirror_game(Who::default()));
        assert_eq!(config.template, "");
        config.capture = CaptureConfig::Regex(RegexCapture {
            lines: vec![r"\[(?<hp>\d+)hp\]".into()],
            ..RegexCapture::default()
        });
        assert!(!config.mirror_game(Who::default()));
        assert_eq!(config.template, "");
        // Codes that do not compile leave the game's own prompt too.
        config.capture = codes("<`%h> ", "");
        assert!(crate::card::presets::game("<`%h> ", "", Who::default()).is_none());
        assert!(!config.mirror_game(Who::default()));
        assert_eq!(config.template, "");
    }

    #[test]
    fn a_design_of_yours_never_follows_the_game() {
        let mut config = PromptConfig {
            capture: codes(crate::testkit::mud::PROMPT, ""),
            ..PromptConfig::from_legacy(true, JAMES)
        };
        assert!(!config.mirror);
        assert!(!config.mirror_game(Who::default()));
        assert_eq!(config.template, JAMES);

        // Following the game from now on writes the design at once.
        config.follow_game(Who::default());
        assert!(config.mirror);
        assert_eq!(
            config.template,
            same_as_the_game(crate::testkit::mud::PROMPT)
        );
        assert!(config.draw);
    }

    #[test]
    fn opening_the_card_never_keeps_a_design_that_follows_the_game() {
        let mut config = PromptConfig {
            capture: codes(crate::testkit::mud::PROMPT, ""),
            previous_templates: vec![JAMES.into()],
            ..PromptConfig::fresh()
        };
        config.mirror_game(Who::default());
        assert_eq!(
            config.template,
            same_as_the_game(crate::testkit::mud::PROMPT)
        );
        // Same as the game brings it back, so Yours stays yours.
        assert!(!config.note_opened());
        assert_eq!(config.previous_templates, [JAMES]);
    }

    #[test]
    fn a_design_counts_as_none_of_your_own_when_vosh_wrote_it() {
        // At a glance as it last shipped, then as it first shipped.
        assert_eq!(RETIRED_DEFAULTS.map(str::len), [884, 728]);
        for none in ["", DEFAULT_DESIGN, RETIRED_DEFAULTS[0], RETIRED_DEFAULTS[1]] {
            assert!(PromptConfig::counts_as_no_design(none), "{none}");
        }
        // Any other design is yours, even one a byte away from a default.
        for design in [
            JAMES.to_string(),
            RETIRED_DEFAULTS[0].trim_end().to_string(),
            format!("{} ", RETIRED_DEFAULTS[1]),
            format!("{DEFAULT_DESIGN} "),
            same_as_the_game(crate::testkit::mud::PROMPT),
        ] {
            assert!(!PromptConfig::counts_as_no_design(&design), "{design}");
        }
    }

    /// A file that holds a `[prompt]` table the way a profile file does.
    #[derive(Debug, Default, Serialize, Deserialize)]
    struct File {
        #[serde(default, skip_serializing_if = "Option::is_none", with = "file_table")]
        prompt: Option<PromptConfig>,
    }

    fn through_file(config: &PromptConfig) -> (serde_json::Value, PromptConfig) {
        let written = serde_json::to_value(File {
            prompt: Some(config.clone()),
        })
        .unwrap();
        let back: File = serde_json::from_value(written.clone()).unwrap();
        (written["prompt"].clone(), back.prompt.expect("the table"))
    }

    #[test]
    fn a_file_writes_mirror_only_when_the_design_does_not_say_it() {
        let prompt = crate::testkit::mud::PROMPT;
        // Following the game, with the design in template for an older
        // build to draw. Same as the game for the codes says it.
        let mut follows = PromptConfig {
            draw: true,
            capture: codes(prompt, ""),
            ..PromptConfig::fresh()
        };
        follows.mirror_game(Who::default());
        let (written, back) = through_file(&follows);
        assert!(written.get("mirror").is_none(), "{written}");
        assert_eq!(written["template"], same_as_the_game(prompt));
        assert_eq!(back, follows);

        // Following the game as an immortal, whose design a mortal never
        // has, says it. The file reads back the design for a mortal,
        // which the live prompt writes again for who you are.
        let immortal = Who {
            immortal: true,
            ..Who::default()
        };
        let mut for_an_immortal = PromptConfig {
            draw: true,
            capture: codes("<`(12%u %h> ", ""),
            ..PromptConfig::fresh()
        };
        assert!(for_an_immortal.mirror_game(immortal));
        let (written, back) = through_file(&for_an_immortal);
        assert_eq!(written["mirror"], true);
        assert!(back.mirror);
        assert_eq!(back.template, "");

        // Following the game with no codes says it with an empty design.
        let (written, back) = through_file(&PromptConfig {
            draw: true,
            ..PromptConfig::fresh()
        });
        assert!(written.get("mirror").is_none(), "{written}");
        assert!(back.mirror);

        // Your own design says it is yours.
        let yours = PromptConfig::from_legacy(true, JAMES);
        let (written, back) = through_file(&yours);
        assert!(written.get("mirror").is_none(), "{written}");
        assert_eq!(back, yours);

        // A design of yours that matches Same as the game for your codes
        // says it is yours.
        let matches = PromptConfig {
            mirror: false,
            ..follows.clone()
        };
        let (written, back) = through_file(&matches);
        assert_eq!(written["mirror"], false);
        assert_eq!(back, matches);

        // Vosh's default you chose, and an empty design you chose, say
        // they are yours.
        for design in [DEFAULT_DESIGN, ""] {
            let chosen = PromptConfig::from_legacy(true, design);
            let (written, back) = through_file(&chosen);
            assert_eq!(written["mirror"], false, "{design}");
            assert_eq!(back, chosen, "{design}");
        }

        // Every other key reads as it always has.
        let full = PromptConfig {
            draw: true,
            template: JAMES.into(),
            previous_templates: vec!["%hp".into()],
            capture: codes(prompt, "%h> "),
            show: PromptShow::Pinned,
            mirror: false,
        };
        let (written, back) = through_file(&full);
        assert_eq!(written, serde_json::to_value(&full).unwrap());
        assert_eq!(back, full);
    }

    #[test]
    fn a_file_that_holds_same_as_the_game_for_its_codes_follows_the_game() {
        let prompt = crate::testkit::mud::PROMPT;
        let read = |template: &str, codes: &str| {
            let file = serde_json::json!({"prompt": {
                "draw": true,
                "template": template,
                "capture": {"kind": "aabahran", "prompt": codes, "fprompt": ""},
            }});
            serde_json::from_value::<File>(file)
                .unwrap()
                .prompt
                .expect("the table")
        };
        // An older build drops mirror when it saves a design that
        // followed the game, and Same as the game in its start list
        // wrote the same text.
        let config = read(&same_as_the_game(prompt), prompt);
        assert!(config.mirror);
        assert_eq!(config.template, same_as_the_game(prompt));

        // Your codes moved on while an older build kept the design, so
        // it is a design of yours now.
        let all = crate::testkit::mud::PROMPT_ALL;
        assert_ne!(same_as_the_game(prompt), same_as_the_game(all));
        let config = read(&same_as_the_game(prompt), all);
        assert!(!config.mirror);
        assert_eq!(config.template, same_as_the_game(prompt));
    }

    #[test]
    fn a_file_with_a_default_vosh_shipped_follows_the_game() {
        let prompt = crate::testkit::mud::PROMPT;
        for old in [DEFAULT_DESIGN, RETIRED_DEFAULTS[0], RETIRED_DEFAULTS[1]] {
            // As an earlier build wrote it, drawing on and pinned.
            let file = serde_json::json!({"prompt": {
                "draw": true,
                "template": old,
                "previous_templates": [JAMES],
                "capture": {"kind": "aabahran", "prompt": prompt, "fprompt": ""},
                "show": "pinned",
            }});
            let read: File = serde_json::from_value(file).unwrap();
            let config = read.prompt.expect("the table");
            assert!(config.mirror);
            assert_eq!(config.template, same_as_the_game(prompt));
            // Nobody chose the old text, so it is not kept as an earlier
            // design. The switch, the place and yours stay.
            assert_eq!(config.previous_templates, [JAMES]);
            assert!(config.draw);
            assert_eq!(config.show, PromptShow::Pinned);
        }

        // With no codes it follows the game with no design, so drawing
        // shows the game's own prompt.
        let file = serde_json::json!({"prompt": {"draw": true, "template": DEFAULT_DESIGN}});
        let config = serde_json::from_value::<File>(file)
            .unwrap()
            .prompt
            .unwrap();
        assert!(config.mirror);
        assert_eq!(config.template, "");

        // A file that says the default is yours keeps it.
        let file = serde_json::json!({"prompt": {
            "draw": true, "template": DEFAULT_DESIGN, "mirror": false,
            "capture": {"kind": "aabahran", "prompt": prompt, "fprompt": ""},
        }});
        let config = serde_json::from_value::<File>(file)
            .unwrap()
            .prompt
            .unwrap();
        assert!(!config.mirror);
        assert_eq!(config.template, DEFAULT_DESIGN);

        // A mirror this build cannot read reads as the design says.
        let file = serde_json::json!({"prompt": {"template": JAMES, "mirror": "yes"}});
        let config = serde_json::from_value::<File>(file)
            .unwrap()
            .prompt
            .unwrap();
        assert!(!config.mirror);
        assert_eq!(config.template, JAMES);
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
        let leftover = &config.previous_templates;
        assert!(leftover.is_empty(), "{leftover:?}");

        // A design that follows the game is not kept, and the default
        // you chose stops following it.
        let mut config = PromptConfig {
            capture: codes(crate::testkit::mud::PROMPT, ""),
            ..PromptConfig::fresh()
        };
        config.mirror_game(Who::default());
        assert!(config.use_default_design());
        assert_eq!(config.template, DEFAULT_DESIGN);
        assert!(!config.mirror);
        let leftover = &config.previous_templates;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert!(!config.mirror_game(Who::default()));
        assert_eq!(config.template, DEFAULT_DESIGN);
        assert!(!config.use_default_design());
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
}
