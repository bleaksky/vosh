//! The custom prompt of the live profile: its saved `[prompt]` table and
//! the session's variables (section 5, live state).
//!
//! The table lasts as long as the profile. The variables last as long as
//! the connection, and a profile switch keeps the GMCP packets while it
//! drops the values the last profile's prompt read.

use std::collections::BTreeMap;

use chrono::{DateTime, FixedOffset, SecondsFormat};
use serde::Serialize;
use serde_json::Value as Json;

use crate::aabahran::Who;
use crate::config::{CaptureConfig, CaptureSource, PromptConfig};
use crate::gmcp::{CharPrompt, Observed, CHAR_STATE, CHAR_STATUS};
use crate::stage::Stage;
use crate::template::Template;
use crate::vars::{forsaken_lands, Vars};

/// What told Vosh your prompt settings, in `session://game-prompt-seen`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SeenKind {
    /// Char.Prompt.
    Gmcp,
    /// A line that shows your PROMPT setting.
    Prompt,
    /// A line that shows your fight prompt setting.
    Fprompt,
    /// You turned prompts off in the game.
    Off,
}

/// The game told Vosh your prompt settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GamePromptSeen {
    pub kind: SeenKind,
    /// The setting as the game stores it. For Char.Prompt it is the
    /// PROMPT, or the fight prompt when only that changed.
    pub text: String,
    /// The active profile's capture took it, which raises the toast.
    pub applied: bool,
}

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
    /// Who the prompt is for, from the packets, which decides what
    /// Aabahran's `%u` and `%s` print.
    who: Who,
    /// You turned prompts off in the game.
    prompts_off: bool,
    /// What the game said of your prompt settings since the session
    /// last took it.
    seen: Vec<GamePromptSeen>,
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
        self.compile();
        self.apply_rules();
    }

    /// Compile the table for the stage: the capture for who you are, and
    /// the fields the design reads.
    fn compile(&mut self) {
        self.stage.set_capture_for(&self.config.capture, self.who);
        self.stage
            .set_reads(&Template::parse(&self.config.template).reads());
    }

    /// Keep a GMCP packet. Char.Status and Char.State say who the prompt
    /// is for, and a change compiles the capture again. Char.Prompt is
    /// the game's own word on your prompt settings, which an aabahran
    /// capture follows (D10).
    pub fn observe(&mut self, package: &str, data: Json, at: DateTime<FixedOffset>) -> Observed {
        let observed = self.vars.observe(package, data, at);
        if package.eq_ignore_ascii_case(CHAR_STATUS) || package.eq_ignore_ascii_case(CHAR_STATE) {
            self.follow_who();
        }
        if let Some(prompt) = &observed.prompt {
            let seen = self.follow_char_prompt(prompt, at);
            self.seen.push(seen);
        }
        observed
    }

    /// Follow a Char.Prompt. `enabled` says whether your text prompt is
    /// on. An aabahran capture that follows the game takes settings that
    /// differ from the ones it holds, with the source and the time.
    fn follow_char_prompt(
        &mut self,
        prompt: &CharPrompt,
        at: DateTime<FixedOffset>,
    ) -> GamePromptSeen {
        self.prompts_off = !prompt.enabled;
        let mut text = prompt.prompt.clone();
        let mut applied = false;
        if let CaptureConfig::Aabahran(codes) = &self.config.capture {
            let differs = codes.prompt != prompt.prompt || codes.fprompt != prompt.fprompt;
            if codes.follow_game && differs {
                if codes.prompt == prompt.prompt {
                    text.clone_from(&prompt.fprompt);
                }
                let mut config = self.config.clone();
                if let CaptureConfig::Aabahran(codes) = &mut config.capture {
                    codes.prompt.clone_from(&prompt.prompt);
                    codes.fprompt.clone_from(&prompt.fprompt);
                    codes.source = Some(CaptureSource::Gmcp);
                    codes.seen_at = Some(stamp(at));
                }
                self.set_config(config);
                applied = true;
            }
        }
        GamePromptSeen {
            kind: SeenKind::Gmcp,
            text,
            applied,
        }
    }

    /// Apply the latest Char.Prompt to the table a profile switch just
    /// handed over, by the rule every packet follows. Noted for the
    /// session only when the capture took it.
    pub fn follow_latest(&mut self, at: DateTime<FixedOffset>) {
        let Some(prompt) = self.vars.gmcp().char_prompt().cloned() else {
            return;
        };
        let seen = self.follow_char_prompt(&prompt, at);
        if seen.applied {
            self.seen.push(seen);
        }
    }

    /// What the game said of your prompt settings since the last call,
    /// oldest first, for `session://game-prompt-seen`.
    pub fn take_seen(&mut self) -> Vec<GamePromptSeen> {
        std::mem::take(&mut self.seen)
    }

    /// You turned prompts off in the game.
    pub fn prompts_off(&self) -> bool {
        self.prompts_off
    }

    /// Who the prompt is for.
    pub fn who(&self) -> Who {
        self.who
    }

    fn follow_who(&mut self) {
        let gmcp = self.vars.gmcp();
        let level = gmcp
            .get(CHAR_STATUS)
            .and_then(|s| s.get("level"))
            .and_then(Json::as_i64);
        let language = gmcp
            .get(CHAR_STATE)
            .and_then(|s| s.get("language"))
            .and_then(Json::as_str);
        let who = Who::from_packets(level, language);
        if who != self.who {
            self.who = who;
            self.stage.set_capture_for(&self.config.capture, who);
        }
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
        self.forget_who();
        self.prompts_off = false;
        self.seen.clear();
        self.apply_rules();
    }

    /// A new connection starts as a mortal in your own body.
    fn forget_who(&mut self) {
        if self.who != Who::default() {
            self.who = Who::default();
            self.stage.set_capture_for(&self.config.capture, self.who);
        }
    }

    /// The connection closed. Every value, packet and the new build sign
    /// go with it, and so do the open row and the candidates ring. The
    /// webview clears its copy of the prompt vars on the disconnect.
    pub fn disconnect(&mut self) {
        self.vars.disconnect();
        self.stage.reset();
        self.reported_vars = None;
        self.known_host = false;
        self.forget_who();
        self.prompts_off = false;
        self.seen.clear();
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

/// A time as `seen_at` stores it, RFC 3339 to the second.
fn stamp(at: DateTime<FixedOffset>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Secs, false)
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
    fn an_immortal_reads_pacify_once_char_status_says_so() {
        let mut engine = PromptEngine::default();
        engine.connect(true);
        engine.set_config(PromptConfig {
            capture: CaptureConfig::Aabahran(AabahranCapture {
                prompt: "<%h %u> ".into(),
                ..AabahranCapture::default()
            }),
            ..PromptConfig::default()
        });
        let pacify = |engine: &PromptEngine| {
            engine
                .stage
                .recognize(b"", "<10 pacified> ", crate::stage::End::Settled)
                .expect("the prompt")
                .values
                .contains_key("pacify")
        };
        assert!(!pacify(&engine), "a mortal until the game says");
        engine.observe(
            "Char.Status",
            json!({"name": "Tester", "level": 60, "race": "human", "class": "warrior"}),
            at(),
        );
        assert!(engine.who().immortal);
        assert!(pacify(&engine));
        // A new connection starts as a mortal again.
        engine.connect(true);
        assert!(!engine.who().immortal);
        assert!(!pacify(&engine));
    }

    /// A profile that follows the game's settings with `prompt`.
    fn following(prompt: &str) -> PromptConfig {
        PromptConfig {
            draw: true,
            template: "%hp".into(),
            capture: CaptureConfig::Aabahran(AabahranCapture {
                prompt: prompt.into(),
                ..AabahranCapture::default()
            }),
            ..PromptConfig::default()
        }
    }

    fn char_prompt(enabled: bool, prompt: &str, fprompt: &str) -> serde_json::Value {
        json!({"enabled": enabled, "prompt": prompt, "fprompt": fprompt})
    }

    fn codes(engine: &PromptEngine) -> AabahranCapture {
        match &engine.config().capture {
            CaptureConfig::Aabahran(codes) => codes.clone(),
            other => panic!("an aabahran capture, got {other:?}"),
        }
    }

    #[test]
    fn a_changed_char_prompt_updates_the_capture_and_is_noted_once() {
        let mut engine = PromptEngine::default();
        engine.connect(true);
        engine.set_config(following("<%hhp> "));
        let revision = engine.revision();
        engine.observe(
            "Char.Prompt",
            char_prompt(true, "%n%P%C<%hhp %mm %vmv> ", ""),
            at(),
        );
        assert!(engine.vars.new_build(), "the first packet is the sign");
        let got = codes(&engine);
        assert_eq!(got.prompt, "%n%P%C<%hhp %mm %vmv> ");
        assert_eq!(got.source, Some(CaptureSource::Gmcp));
        assert_eq!(got.seen_at.as_deref(), Some("2026-09-29T12:58:02-05:00"));
        assert!(
            engine.revision() > revision,
            "Settings reads the table again"
        );
        assert_eq!(
            engine.take_seen(),
            [GamePromptSeen {
                kind: SeenKind::Gmcp,
                text: "%n%P%C<%hhp %mm %vmv> ".into(),
                applied: true,
            }]
        );
        assert!(engine.take_seen().is_empty());
        // The capture reads the new codes at once.
        assert!(engine
            .stage
            .recognize(b"", "<10hp 20m 30mv> ", crate::stage::End::Settled)
            .is_some());

        // The same settings again change nothing and raise no toast.
        let revision = engine.revision();
        engine.observe(
            "Char.Prompt",
            char_prompt(true, "%n%P%C<%hhp %mm %vmv> ", ""),
            at(),
        );
        assert_eq!(engine.revision(), revision);
        assert_eq!(
            engine.take_seen(),
            [GamePromptSeen {
                kind: SeenKind::Gmcp,
                text: "%n%P%C<%hhp %mm %vmv> ".into(),
                applied: false,
            }]
        );

        // A new fight prompt alone names the fight prompt.
        engine.observe(
            "Char.Prompt",
            char_prompt(true, "%n%P%C<%hhp %mm %vmv> ", "`1%h``hp [%p] > "),
            at(),
        );
        assert_eq!(codes(&engine).fprompt, "`1%h``hp [%p] > ");
        assert_eq!(engine.take_seen()[0].text, "`1%h``hp [%p] > ");
    }

    #[test]
    fn enabled_raises_and_clears_prompts_off_and_keeps_the_codes() {
        let mut engine = PromptEngine::default();
        engine.connect(true);
        engine.set_config(following("%n%P%C<%hhp %mm %vmv> "));
        engine.observe(
            "Char.Prompt",
            char_prompt(false, "%n%P%C<%hhp %mm %vmv> ", ""),
            at(),
        );
        assert!(engine.prompts_off());
        assert_eq!(codes(&engine).prompt, "%n%P%C<%hhp %mm %vmv> ");
        assert!(!engine.take_seen()[0].applied);
        engine.observe(
            "Char.Prompt",
            char_prompt(true, "%n%P%C<%hhp %mm %vmv> ", ""),
            at(),
        );
        assert!(!engine.prompts_off());
        engine.observe(
            "Char.Prompt",
            char_prompt(false, "%n%P%C<%hhp %mm %vmv> ", ""),
            at(),
        );
        engine.disconnect();
        assert!(
            !engine.prompts_off(),
            "a new connection starts with prompts on"
        );
    }

    #[test]
    fn only_an_aabahran_capture_that_follows_the_game_takes_a_char_prompt() {
        for config in [
            PromptConfig::from_legacy(true, "%hp"),
            PromptConfig {
                capture: CaptureConfig::Regex(RegexCapture {
                    lines: vec![r"<(?<hp>\d+)hp>".into()],
                    ..RegexCapture::default()
                }),
                ..PromptConfig::default()
            },
            PromptConfig {
                capture: CaptureConfig::Aabahran(AabahranCapture {
                    prompt: "<%hhp> ".into(),
                    follow_game: false,
                    ..AabahranCapture::default()
                }),
                ..PromptConfig::default()
            },
        ] {
            let mut engine = PromptEngine::default();
            engine.connect(true);
            engine.set_config(config.clone());
            engine.observe("Char.Prompt", char_prompt(true, "%h %m ", ""), at());
            assert_eq!(*engine.config(), config);
            assert!(!engine.take_seen()[0].applied);
        }
    }

    #[test]
    fn a_switch_applies_the_latest_char_prompt_to_the_new_profile() {
        let mut engine = PromptEngine::default();
        engine.connect(true);
        engine.set_config(PromptConfig::from_legacy(true, "%hp"));
        engine.observe("Char.Prompt", char_prompt(true, "%h %m ", ""), at());
        let _ = engine.take_seen();

        engine.switch_profile();
        engine.set_config(following("<%hhp> "));
        engine.follow_latest(at());
        assert_eq!(codes(&engine).prompt, "%h %m ");
        assert_eq!(engine.take_seen().len(), 1);

        // A profile that reads nothing saves nothing from it.
        engine.switch_profile();
        engine.set_config(PromptConfig::from_legacy(true, "%hp"));
        engine.follow_latest(at());
        assert!(engine.config().capture.is_none());
        assert!(engine.take_seen().is_empty());

        // Without a packet this session there is nothing to apply.
        let mut engine = PromptEngine::default();
        engine.connect(true);
        engine.set_config(following("<%hhp> "));
        engine.follow_latest(at());
        assert_eq!(codes(&engine).prompt, "<%hhp> ");
    }

    #[test]
    fn every_char_prompt_fixture_is_taken_as_sent() {
        for file in [
            "char-prompt.gmcp",
            "char-prompt-fight.gmcp",
            "char-prompt-off.gmcp",
        ] {
            let path = format!(
                "{}/../../fixtures/gmcp/aabahran/{file}",
                env!("CARGO_MANIFEST_DIR")
            );
            let text = std::fs::read_to_string(&path).unwrap();
            let (package, body) = text.trim().split_once(' ').unwrap();
            let data: serde_json::Value = serde_json::from_str(body).unwrap();
            let mut engine = PromptEngine::default();
            engine.connect(true);
            engine.set_config(following(""));
            engine.observe(package, data.clone(), at());
            let got = codes(&engine);
            assert_eq!(got.prompt, data["prompt"].as_str().unwrap(), "{file}");
            assert_eq!(got.fprompt, data["fprompt"].as_str().unwrap(), "{file}");
            assert!(engine.stage.has_recognizer(), "{file} compiles as sent");
            assert_eq!(
                engine.prompts_off(),
                !data["enabled"].as_bool().unwrap(),
                "{file}"
            );
        }
    }

    #[test]
    fn a_new_table_keeps_the_values() {
        let mut engine = playing();
        engine.set_config(PromptConfig::from_legacy(false, "%mana"));
        assert_eq!(engine.vars.prompt_vars().len(), 2);
        assert!(engine.vars.new_build());
    }
}
