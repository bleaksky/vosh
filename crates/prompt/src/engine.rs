//! The custom prompt of the live profile: its saved `[prompt]` table and
//! the session's variables (section 5, live state).
//!
//! The table lasts as long as the profile. The variables last as long as
//! the connection, and a profile switch keeps the GMCP packets while it
//! drops the values the last profile's prompt read.

use crate::config::PromptConfig;
use crate::vars::{forsaken_lands, Vars};

/// The live profile's custom prompt.
#[derive(Debug, Clone, Default)]
pub struct PromptEngine {
    config: PromptConfig,
    /// What the session feeds the prompt: script values, the capture, the
    /// latest packet of each GMCP package and the hidden state.
    pub vars: Vars,
    /// The connection is to The Forsaken Lands. False with no connection.
    known_host: bool,
}

impl PromptEngine {
    /// The `[prompt]` table in use.
    pub fn config(&self) -> &PromptConfig {
        &self.config
    }

    /// Take a table for the profile in use, as a load, an import, a
    /// Settings save or an edit hands it over. The session's values stay,
    /// and the Forsaken Lands rules follow the new capture.
    pub fn set_config(&mut self, config: PromptConfig) {
        self.config = config;
        self.apply_rules();
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
        self.known_host = known_host;
        self.apply_rules();
    }

    /// The connection closed. Every value, packet and the new build sign
    /// go with it.
    pub fn disconnect(&mut self) {
        self.vars.disconnect();
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
    use crate::config::{AabahranCapture, CaptureConfig};
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
    fn a_new_table_keeps_the_values() {
        let mut engine = playing();
        engine.set_config(PromptConfig::from_legacy(false, "%mana"));
        assert_eq!(engine.vars.prompt_vars().len(), 2);
        assert!(engine.vars.new_build());
    }
}
