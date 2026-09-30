//! The custom prompt of the live profile: its saved `[prompt]` table and
//! the session's variables (section 5, live state).
//!
//! The table lasts as long as the profile. The variables last as long as
//! the connection, and a profile switch keeps the GMCP packets while it
//! drops the values the last profile's prompt read.

use std::collections::BTreeMap;

use crate::config::PromptConfig;
use crate::stage::Stage;
use crate::vars::{forsaken_lands, Vars};

/// The live profile's custom prompt.
#[derive(Debug, Clone, Default)]
pub struct PromptEngine {
    config: PromptConfig,
    /// What the session feeds the prompt: script values, the capture, the
    /// latest packet of each GMCP package and the hidden state.
    pub vars: Vars,
    /// What Vosh writes around your prompt: the capture compiled from
    /// the table, the open row and the candidates ring.
    pub stage: Stage,
    /// The connection is to The Forsaken Lands. False with no connection.
    known_host: bool,
    /// The prompt vars the webview last heard.
    reported_vars: Option<BTreeMap<String, String>>,
    /// Moves each time the table changes, so a step can tell whether it
    /// changed the table and an open Settings window reads it again.
    revision: u64,
}

impl PromptEngine {
    /// The `[prompt]` table in use.
    pub fn config(&self) -> &PromptConfig {
        &self.config
    }

    /// Take a table for the profile in use, as a load, an import, a
    /// Settings save or an edit hands it over. The session's values stay,
    /// the capture compiles, and the Forsaken Lands rules follow it.
    pub fn set_config(&mut self, config: PromptConfig) {
        if self.config != config {
            self.revision += 1;
        }
        self.config = config;
        self.stage.set_capture(&self.config.capture);
        self.apply_rules();
    }

    /// A count that moves each time the table changes.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Vosh draws your design over a prompt it reads: drawing is on and
    /// the design is not empty.
    pub fn draws(&self) -> bool {
        self.config.draw && !self.config.template.is_empty()
    }

    /// Record a candidate in the ring, on a send or a GA or EOR, with
    /// whether drawing is on and whether the profile has a capture. See
    /// [`Stage::record`].
    pub fn record(&mut self, partial: Option<(&[u8], &str)>, at_ms: i64) {
        let draw = self.draws();
        let capture = !self.config.capture.is_none();
        self.stage.record(partial, at_ms, draw, capture);
    }

    /// The fresh prompt vars for `session://prompt-vars`, when they
    /// changed since the webview last heard them or `always` asks for them
    /// anyway, as a recognized prompt does. None otherwise.
    pub fn take_prompt_vars(&mut self, always: bool) -> Option<BTreeMap<String, String>> {
        let now = self.vars.prompt_vars();
        let changed = match &self.reported_vars {
            Some(last) => *last != now,
            None => !now.is_empty(),
        };
        if !(always || changed) {
            return None;
        }
        self.reported_vars = Some(now.clone());
        Some(now)
    }

    /// Whether the Forsaken Lands rules hold (D17): the host is The
    /// Forsaken Lands, or the capture reads Aabahran's codes.
    pub fn forsaken(&self) -> bool {
        self.vars.forsaken()
    }

    /// A connection opened. It starts with no packets and no values.
    /// `known_host` is whether the host is The Forsaken Lands.
    pub fn connect(&mut self, known_host: bool) {
        self.vars.disconnect();
        self.stage.reset();
        self.reported_vars = None;
        self.known_host = known_host;
        self.apply_rules();
    }

    /// The connection closed. Every value, packet and the new build sign
    /// go with it, and so do the open row and the candidates ring. The
    /// webview clears its copy of the prompt vars on the disconnect.
    pub fn disconnect(&mut self) {
        self.vars.disconnect();
        self.stage.reset();
        self.reported_vars = None;
        self.known_host = false;
        self.apply_rules();
    }

    /// Another profile is taking over the connection. The GMCP packets
    /// and the new build sign stay, since the connection did not change,
    /// and the values the last profile's prompt read go. The next
    /// [`PromptEngine::set_config`] hands over the new profile's table.
    pub fn switch_profile(&mut self) {
        let forsaken = self.rules();
        self.vars.switch_profile(forsaken);
    }

    fn rules(&self) -> bool {
        forsaken_lands(self.known_host, self.config.capture.is_aabahran())
    }

    fn apply_rules(&mut self) {
        let forsaken = self.rules();
        if self.vars.forsaken() != forsaken {
            self.vars.set_forsaken(forsaken);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AabahranCapture, CaptureConfig, RegexCapture};
    use crate::vars::Capture;
    use serde_json::json;

    fn at() -> chrono::DateTime<chrono::FixedOffset> {
        chrono::DateTime::parse_from_rfc3339("2026-09-29T12:58:02-05:00").unwrap()
    }

    fn aabahran() -> PromptConfig {
        PromptConfig {
            capture: CaptureConfig::Aabahran(AabahranCapture::default()),
            ..PromptConfig::default()
        }
    }

    /// A Forsaken Lands connection on the new build, with a prompt read
    /// and a script value set.
    fn playing() -> PromptEngine {
        let mut engine = PromptEngine::default();
        engine.connect(true);
        engine.vars.observe(
            "Char.Prompt",
            json!({"enabled": true, "prompt": "%h ", "fprompt": ""}),
            at(),
        );
        engine
            .vars
            .observe("Char.Vitals", json!({"hp": 850, "maxhp": 900}), at());
        engine.vars.capture(Capture {
            values: [("hp".to_string(), "840".to_string())].into(),
            raw: None,
        });
        engine.vars.set_script("mood", "grim");
        engine
    }

    #[test]
    fn the_revision_moves_only_when_the_table_changes() {
        let mut engine = PromptEngine::default();
        assert_eq!(engine.revision(), 0);
        engine.set_config(PromptConfig::default());
        assert_eq!(engine.revision(), 0, "the same table");
        engine.set_config(aabahran());
        assert_eq!(engine.revision(), 1);
        engine.set_config(aabahran());
        assert_eq!(engine.revision(), 1);
        // A session event leaves the table alone.
        engine.connect(true);
        engine.disconnect();
        assert_eq!(engine.revision(), 1);
    }

    #[test]
    fn the_rules_hold_on_the_host_or_with_an_aabahran_capture() {
        let mut engine = PromptEngine::default();
        assert!(!engine.forsaken());
        engine.connect(true);
        assert!(engine.forsaken());
        engine.disconnect();
        assert!(!engine.forsaken(), "no connection, no host");

        engine.set_config(aabahran());
        assert!(engine.forsaken(), "the capture reads Aabahran's codes");
        engine.connect(false);
        assert!(engine.forsaken());
        engine.set_config(PromptConfig::default());
        assert!(!engine.forsaken());
    }

    #[test]
    fn a_switch_keeps_the_packets_and_drops_what_the_prompt_read() {
        let mut engine = playing();
        assert!(engine.vars.new_build());
        assert_eq!(engine.vars.prompt_vars().len(), 2);

        engine.switch_profile();
        engine.set_config(PromptConfig::from_legacy(true, "%hp"));
        assert!(engine.vars.new_build(), "the connection did not change");
        assert!(engine.vars.gmcp().get("Char.Vitals").is_some());
        assert!(engine.vars.prompt_vars().is_empty());
        assert_eq!(engine.config().template, "%hp");
        assert!(engine.forsaken(), "the host still holds the rules");
    }

    #[test]
    fn a_disconnect_clears_the_session_and_keeps_the_table() {
        let mut engine = playing();
        engine.set_config(PromptConfig::from_legacy(true, "%hp"));
        engine.disconnect();
        assert!(!engine.vars.new_build());
        assert!(engine.vars.gmcp().get("Char.Vitals").is_none());
        assert!(engine.vars.prompt_vars().is_empty());
        assert_eq!(engine.config().template, "%hp");
        assert!(engine.config().draw);
    }

    #[test]
    fn a_connection_starts_over() {
        let mut engine = playing();
        engine.connect(false);
        assert!(!engine.vars.new_build());
        assert!(engine.vars.prompt_vars().is_empty());
        assert!(!engine.forsaken());
    }

    #[test]
    fn a_table_compiles_its_capture_for_the_stage() {
        let mut engine = PromptEngine::default();
        assert!(!engine.stage.has_recognizer());
        engine.set_config(PromptConfig {
            capture: CaptureConfig::Regex(RegexCapture {
                lines: vec![r"<(?<hp>\d+)hp>".into()],
                ..RegexCapture::default()
            }),
            ..PromptConfig::from_legacy(true, "%hp")
        });
        assert!(engine.stage.has_recognizer());
        assert!(engine.draws());
        engine.connect(false);
        assert!(
            engine.stage.has_recognizer(),
            "a connection keeps the capture"
        );
        engine.set_config(PromptConfig::from_legacy(true, "%hp"));
        assert!(!engine.stage.has_recognizer());
        // Drawing needs the switch on and a design.
        engine.set_config(PromptConfig::from_legacy(true, ""));
        assert!(!engine.draws());
        engine.set_config(PromptConfig::from_legacy(false, "%hp"));
        assert!(!engine.draws());
    }

    #[test]
    fn prompt_vars_are_reported_when_they_change() {
        let mut engine = PromptEngine::default();
        engine.connect(false);
        assert_eq!(engine.take_prompt_vars(false), None, "nothing yet");
        assert_eq!(engine.take_prompt_vars(true), Some(BTreeMap::new()));
        engine.vars.set_script("mood", "grim");
        let vars = engine.take_prompt_vars(false).expect("a change");
        assert_eq!(vars.get("mood").map(String::as_str), Some("grim"));
        assert_eq!(engine.take_prompt_vars(false), None, "no change");
        assert!(
            engine.take_prompt_vars(true).is_some(),
            "a prompt asks anyway"
        );
        // A disconnect forgets what the webview heard, since it clears.
        engine.disconnect();
        assert_eq!(engine.take_prompt_vars(false), None);
    }

    #[test]
    fn the_ring_records_whether_drawing_is_on_and_a_capture_exists() {
        let mut engine = PromptEngine::default();
        engine.connect(false);
        let mut out = crate::stage::Output::new(false);
        engine
            .stage
            .line(&mut out, b"Healer> ", "Healer> ", None, b"Healer> \r\n");
        engine.record(None, 5);
        let entry = engine.stage.ring().next().expect("an entry");
        assert!(!entry.draw);
        assert!(!entry.capture);
    }

    #[test]
    fn a_new_table_keeps_the_values() {
        let mut engine = playing();
        engine.set_config(PromptConfig::from_legacy(false, "%mana"));
        assert_eq!(engine.vars.prompt_vars().len(), 2);
        assert!(engine.vars.new_build());
    }
}
