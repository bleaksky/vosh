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

use crate::aabahran::observer::{self, ReplyKind};
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

/// How long after one of your own sends the game's reply to `prompt` or
/// `fprompt` counts, in milliseconds.
pub const OBSERVE_MS: i64 = observer::WINDOW_MS;

/// The settings the game showed after one of your own sends this
/// session, without Char.Prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSetting {
    pub prompt: Option<String>,
    pub fprompt: Option<String>,
    /// When the last of them came.
    pub at: DateTime<FixedOffset>,
}

/// How many pulses in a row without your prompt make it not matching.
pub const MISSES: u32 = 3;

/// Whether Vosh reads your prompt, for `session://prompt-status` and
/// `#prompt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Nothing reads your prompt in this profile.
    NoCapture,
    Matching,
    /// Three pulses in a row brought no prompt Vosh reads.
    NotMatching,
    /// You turned prompts off in the game, so no prompt comes and none
    /// is missed (D28).
    PromptsOff,
}

/// `session://prompt-status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusReport {
    pub status: Status,
    /// When Vosh last read your prompt, RFC 3339 local time.
    pub last_match_at: Option<String>,
}

/// What counts misses between prompts (section 4).
#[derive(Debug, Clone, Default)]
struct Misses {
    /// Pulses or sends in a row that brought no prompt Vosh read.
    count: u32,
    /// A prompt was read since the pulse started, or since your send.
    matched: bool,
    /// A pulse has started this session, so the next one ends it.
    in_pulse: bool,
    /// The game sent text since your last send.
    text: bool,
    last_match_at: Option<DateTime<FixedOffset>>,
    reported: Option<StatusReport>,
}

/// What the observer keeps between lines (section 3).
#[derive(Debug, Clone, Default)]
struct Observer {
    /// When you last sent a line, in milliseconds since the epoch.
    sent_at: Option<i64>,
    /// Your last send was `prompt off`.
    sent_off: bool,
    /// "You will no longer see prompts." came since your last send or
    /// the last pulse.
    off_line: bool,
    seen: Option<SessionSetting>,
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
    observer: Observer,
    misses: Misses,
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
        if self.config.capture != config.capture {
            // A new capture starts with no misses.
            self.misses.count = 0;
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
        if observed.pulse {
            self.observer.off_line = false;
            self.pulse_started();
        }
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
        if let CaptureConfig::Aabahran(codes) = &self.config.capture {
            if codes.prompt == prompt.prompt && codes.fprompt != prompt.fprompt {
                text.clone_from(&prompt.fprompt);
            }
        }
        let applied = self.take_settings(
            Some(&prompt.prompt),
            Some(&prompt.fprompt),
            CaptureSource::Gmcp,
            at,
        );
        GamePromptSeen {
            kind: SeenKind::Gmcp,
            text,
            applied,
        }
    }

    /// Hand settings the game showed to an aabahran capture that follows
    /// the game, when they differ from the ones it holds. Returns whether
    /// it took them.
    fn take_settings(
        &mut self,
        prompt: Option<&str>,
        fprompt: Option<&str>,
        source: CaptureSource,
        at: DateTime<FixedOffset>,
    ) -> bool {
        let CaptureConfig::Aabahran(codes) = &self.config.capture else {
            return false;
        };
        let differs = prompt.is_some_and(|p| p != codes.prompt)
            || fprompt.is_some_and(|f| f != codes.fprompt);
        if !codes.follow_game || !differs {
            return false;
        }
        let mut config = self.config.clone();
        if let CaptureConfig::Aabahran(codes) = &mut config.capture {
            if let Some(prompt) = prompt {
                codes.prompt = prompt.to_string();
            }
            if let Some(fprompt) = fprompt {
                codes.fprompt = fprompt.to_string();
            }
            codes.source = Some(source);
            codes.seen_at = Some(stamp(at));
        }
        self.set_config(config);
        true
    }

    /// Note a line you sent: the observer's window opens, and `prompt
    /// off` in it means the reply sets nothing. `text` is what went to
    /// the game, aliases expanded. `at_ms` is milliseconds since the
    /// epoch.
    ///
    /// Returns true when the send started a pulse, on a server that
    /// sends no Char.Vitals.
    pub fn note_send(&mut self, text: &str, at_ms: i64) -> bool {
        self.observer.sent_at = Some(at_ms);
        self.observer.sent_off = text.lines().any(observer::turns_prompts_off);
        self.observer.off_line = false;
        if !self.forsaken() {
            // Elsewhere a miss is a send whose reply brought text but no
            // prompt Vosh read before your next send.
            if self.stage.has_recognizer() && self.misses.text && !self.misses.matched {
                self.misses.count += 1;
            }
            self.misses.text = false;
            self.misses.matched = false;
        }
        let pulse = self.vars.on_send();
        if pulse {
            self.pulse_started();
        }
        pulse
    }

    /// A pulse started. Under the Forsaken Lands rules the one before it
    /// is a miss when it brought no prompt Vosh read, unless you turned
    /// prompts off (D28).
    fn pulse_started(&mut self) {
        if !self.forsaken() {
            return;
        }
        let missed = self.misses.in_pulse && !self.misses.matched;
        if missed && self.stage.has_recognizer() && !self.prompts_off {
            self.misses.count += 1;
        }
        self.misses.in_pulse = true;
        self.misses.matched = false;
    }

    /// The game sent a line or a partial.
    pub fn note_text(&mut self) {
        self.misses.text = true;
    }

    /// Whether Vosh reads your prompt now.
    pub fn status(&self) -> Status {
        if self.prompts_off {
            Status::PromptsOff
        } else if !self.stage.has_recognizer() {
            Status::NoCapture
        } else if self.misses.count >= MISSES {
            Status::NotMatching
        } else {
            Status::Matching
        }
    }

    /// When Vosh last read your prompt this session.
    pub fn last_match_at(&self) -> Option<DateTime<FixedOffset>> {
        self.misses.last_match_at
    }

    /// The status when it changed since the last call, for one
    /// `session://prompt-status` per socket read.
    pub fn take_status_change(&mut self) -> Option<StatusReport> {
        let report = StatusReport {
            status: self.status(),
            last_match_at: self.misses.last_match_at.map(stamp),
        };
        if self.misses.reported.as_ref() == Some(&report) {
            return None;
        }
        self.misses.reported = Some(report.clone());
        Some(report)
    }

    /// The observer reads lines now: the Forsaken Lands rules hold, no
    /// Char.Prompt came this session, and you sent a line within
    /// [`OBSERVE_MS`].
    pub fn observing(&self, at_ms: i64) -> bool {
        self.forsaken()
            && !self.vars.gmcp().prompt_seen()
            && self
                .observer
                .sent_at
                .is_some_and(|sent| (0..=OBSERVE_MS).contains(&(at_ms - sent)))
    }

    /// Read a complete line the game printed as a reply to `prompt` or
    /// `fprompt`, while [`PromptEngine::observing`]. A setting it shows
    /// goes to an aabahran capture that follows the game. The reply to
    /// `prompt off` never does, and raises prompts off instead.
    pub fn observe_line(&mut self, raw: &[u8], plain: &str, at: DateTime<FixedOffset>) {
        if !self.observing(at.timestamp_millis()) {
            return;
        }
        let Some(reply) = observer::reply(plain) else {
            return;
        };
        let text = observer::setting(reply, raw);
        let (kind, applied) = match reply.kind {
            ReplyKind::Off => {
                self.observer.off_line = true;
                self.prompts_off = true;
                (SeenKind::Off, false)
            }
            ReplyKind::Prompt => {
                if self.observer.sent_off || self.observer.off_line {
                    self.prompts_off = true;
                    return;
                }
                // Any `prompt` but `prompt off` turns prompts on.
                self.prompts_off = false;
                self.note_session_setting(Some(&text), None, at);
                let applied = self.take_settings(Some(&text), None, CaptureSource::Session, at);
                (SeenKind::Prompt, applied)
            }
            ReplyKind::Channels => {
                // `channels` turns nothing on. While prompts are off it
                // shows what an older build's `prompt off` left, which
                // is no setting of yours.
                if self.prompts_off {
                    return;
                }
                self.note_session_setting(Some(&text), None, at);
                let applied = self.take_settings(Some(&text), None, CaptureSource::Session, at);
                (SeenKind::Prompt, applied)
            }
            ReplyKind::Fight | ReplyKind::NoFight => {
                self.note_session_setting(None, Some(&text), at);
                let applied = self.take_settings(None, Some(&text), CaptureSource::Session, at);
                (SeenKind::Fprompt, applied)
            }
        };
        self.seen.push(GamePromptSeen {
            kind,
            text,
            applied,
        });
    }

    fn note_session_setting(
        &mut self,
        prompt: Option<&str>,
        fprompt: Option<&str>,
        at: DateTime<FixedOffset>,
    ) {
        let seen = self.observer.seen.get_or_insert(SessionSetting {
            prompt: None,
            fprompt: None,
            at,
        });
        if let Some(prompt) = prompt {
            seen.prompt = Some(prompt.to_string());
        }
        if let Some(fprompt) = fprompt {
            seen.fprompt = Some(fprompt.to_string());
        }
        seen.at = at;
    }

    /// The settings the game showed after your own sends this session.
    pub fn session_setting(&self) -> Option<&SessionSetting> {
        self.observer.seen.as_ref()
    }

    /// Vosh read your prompt at `at`, so your text prompt is on and no
    /// prompt is missed.
    pub fn note_prompt(&mut self, at: DateTime<FixedOffset>) {
        self.prompts_off = false;
        self.misses.count = 0;
        self.misses.matched = true;
        self.misses.last_match_at = Some(at);
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
        self.observer = Observer::default();
        self.misses = Misses {
            reported: self.misses.reported.take(),
            ..Misses::default()
        };
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
        self.observer = Observer::default();
        self.misses = Misses {
            reported: self.misses.reported.take(),
            ..Misses::default()
        };
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

    /// A Forsaken Lands connection that follows the game with `prompt`,
    /// without Char.Prompt.
    fn older_build(prompt: &str) -> PromptEngine {
        let mut engine = PromptEngine::default();
        engine.connect(true);
        engine.set_config(following(prompt));
        engine
    }

    fn at_ms(ms: i64) -> chrono::DateTime<chrono::FixedOffset> {
        chrono::DateTime::from_timestamp_millis(ms)
            .unwrap()
            .fixed_offset()
    }

    const SENT: i64 = 1_759_000_000_000;

    fn line(engine: &mut PromptEngine, text: &str, after_ms: i64) {
        engine.observe_line(text.as_bytes(), text, at_ms(SENT + after_ms));
    }

    #[test]
    fn a_reply_after_your_send_updates_the_capture() {
        // Any send opens the window, prompt abbreviated or another
        // command such as chan.
        for sent in ["prompt %h %m ", "prom %h %m ", "p %h %m ", "chan"] {
            let mut engine = older_build("<%hhp> ");
            engine.note_send(&format!("{sent}\r\n"), SENT);
            line(&mut engine, "Prompt set to %h %m ", 40);
            let got = codes(&engine);
            assert_eq!(got.prompt, "%h %m ", "{sent}");
            assert_eq!(got.source, Some(CaptureSource::Session));
            assert!(got.seen_at.is_some());
            assert_eq!(
                engine.take_seen(),
                [GamePromptSeen {
                    kind: SeenKind::Prompt,
                    text: "%h %m ".into(),
                    applied: true,
                }]
            );
            let seen = engine.session_setting().expect("noted for the card");
            assert_eq!(seen.prompt.as_deref(), Some("%h %m "));
        }
        // An alias that sends it counts the same, since the session hands
        // over what went to the game.
        let mut engine = older_build("<%hhp> ");
        engine.note_send("say hi\r\nfprompt %h>\r\n", SENT);
        line(&mut engine, "Fight prompt set to %h> ", 10);
        assert_eq!(codes(&engine).fprompt, "%h> ");
        assert_eq!(engine.take_seen()[0].kind, SeenKind::Fprompt);
        line(&mut engine, "Fight prompt cleared.", 20);
        assert_eq!(codes(&engine).fprompt, "");
    }

    #[test]
    fn a_reply_after_two_seconds_or_with_no_send_is_ignored() {
        let mut engine = older_build("<%hhp> ");
        line(&mut engine, "Prompt set to %h ", 0);
        assert_eq!(codes(&engine).prompt, "<%hhp> ", "no send yet");
        engine.note_send("prompt %h\r\n", SENT);
        line(&mut engine, "Prompt set to %h ", OBSERVE_MS + 1);
        assert_eq!(codes(&engine).prompt, "<%hhp> ");
        assert!(engine.take_seen().is_empty());
        line(&mut engine, "Prompt set to %h ", OBSERVE_MS);
        assert_eq!(codes(&engine).prompt, "%h ");
    }

    #[test]
    fn prompt_off_saves_nothing_and_raises_prompts_off_in_either_order() {
        // Your own send said prompt off.
        let mut engine = older_build("<%hhp> ");
        engine.note_send("prompt off\r\n", SENT);
        line(&mut engine, "Prompt set to garbage", 5);
        assert_eq!(codes(&engine).prompt, "<%hhp> ");
        assert!(engine.prompts_off());

        // An alias sent it under another name, and the reply comes after
        // the line the older builds print first.
        let mut engine = older_build("<%hhp> ");
        engine.note_send("quiet\r\n", SENT);
        line(&mut engine, "You will no longer see prompts.", 5);
        assert!(engine.prompts_off());
        line(&mut engine, "Prompt set to garbage", 6);
        assert_eq!(codes(&engine).prompt, "<%hhp> ");
        let seen = engine.take_seen();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].kind, SeenKind::Off);

        // prompt turns them on again with Current prompt, and a
        // recognized prompt clears them too.
        engine.note_send("prompt\r\n", SENT + 3_000);
        line(&mut engine, "Current prompt: <%hhp> ", 3_010);
        assert!(!engine.prompts_off());
        engine.note_send("prompt off\r\n", SENT + 4_000);
        line(&mut engine, "You will no longer see prompts.", 4_010);
        assert!(engine.prompts_off());
        engine.note_prompt(at());
        assert!(!engine.prompts_off());
    }

    #[test]
    fn channels_shows_your_prompt_and_turns_nothing_on() {
        // prompt off on an older build stores a buffer it never filled.
        let mut engine = older_build("<%hhp> ");
        engine.note_send("prompt off\r\n", SENT);
        line(&mut engine, "You will no longer see prompts.", 5);
        line(&mut engine, "Prompt set to \u{1}\u{2}", 6);
        let _ = engine.take_seen();
        // channels shows that buffer and leaves prompts off.
        engine.note_send("channels\r\n", SENT + 3_000);
        line(&mut engine, "Your current prompt is: \u{1}\u{2}", 3_010);
        assert!(engine.prompts_off());
        assert_eq!(engine.status(), Status::PromptsOff);
        assert_eq!(codes(&engine).prompt, "<%hhp> ");
        assert!(engine.take_seen().is_empty());
        // With prompts on, the setting it shows is yours.
        engine.note_send("prompt %h\r\n", SENT + 4_000);
        line(&mut engine, "Prompt set to %h ", 4_010);
        assert!(!engine.prompts_off());
        engine.note_send("channels\r\n", SENT + 5_000);
        line(&mut engine, "Your current prompt is: %h %m ", 5_010);
        assert_eq!(codes(&engine).prompt, "%h %m ");
        assert!(!engine.prompts_off());
    }

    #[test]
    fn with_char_prompt_this_session_the_observer_does_nothing() {
        let mut engine = older_build("<%hhp> ");
        engine.observe("Char.Prompt", char_prompt(true, "%h ", ""), at());
        let _ = engine.take_seen();
        engine.note_send("prompt %m\r\n", SENT);
        assert!(!engine.observing(SENT + 10));
        line(&mut engine, "Prompt set to %m ", 10);
        assert_eq!(codes(&engine).prompt, "%h ");
        assert!(engine.take_seen().is_empty());
        assert!(engine.session_setting().is_none());
    }

    #[test]
    fn the_observer_keeps_to_the_forsaken_lands() {
        let mut engine = PromptEngine::default();
        engine.connect(false);
        engine.note_send("prompt %h\r\n", SENT);
        assert!(!engine.observing(SENT));
        // A profile without a capture still notes the setting for the
        // card on The Forsaken Lands.
        let mut engine = PromptEngine::default();
        engine.connect(true);
        engine.note_send("prompt %h\r\n", SENT);
        line(&mut engine, "Prompt set to %h ", 1);
        assert!(engine.config().capture.is_none());
        assert_eq!(
            engine.session_setting().and_then(|s| s.prompt.as_deref()),
            Some("%h ")
        );
        assert!(!engine.take_seen()[0].applied);
    }

    fn vitals(engine: &mut PromptEngine) {
        engine.observe("Char.Vitals", json!({"hp": 10, "maxhp": 20}), at());
    }

    #[test]
    fn three_pulses_without_a_prompt_are_not_matching_and_one_match_clears_it() {
        let mut engine = older_build("<%hhp> ");
        assert_eq!(engine.status(), Status::Matching);
        let first = engine.take_status_change().expect("the first report");
        assert_eq!(first.status, Status::Matching);
        assert_eq!(first.last_match_at, None);
        assert_eq!(engine.take_status_change(), None, "no change");
        // The pulse that starts the session is no miss.
        vitals(&mut engine);
        vitals(&mut engine);
        vitals(&mut engine);
        assert_eq!(engine.status(), Status::Matching, "two misses");
        vitals(&mut engine);
        assert_eq!(engine.status(), Status::NotMatching);
        assert_eq!(
            engine.take_status_change().map(|r| r.status),
            Some(Status::NotMatching)
        );
        engine.note_prompt(at());
        let report = engine.take_status_change().expect("matching again");
        assert_eq!(report.status, Status::Matching);
        assert_eq!(
            report.last_match_at.as_deref(),
            Some("2026-09-29T12:58:02-05:00")
        );
        // A pulse with a prompt in it is no miss.
        for _ in 0..5 {
            vitals(&mut engine);
            engine.note_prompt(at());
        }
        assert_eq!(engine.status(), Status::Matching);
    }

    #[test]
    fn pulses_with_prompts_off_count_no_miss() {
        let mut engine = older_build("%n%P%C<%hhp %mm %vmv> ");
        engine.observe(
            "Char.Prompt",
            char_prompt(false, "%n%P%C<%hhp %mm %vmv> ", ""),
            at(),
        );
        // The new build keeps sending the packages each pulse.
        for _ in 0..5 {
            vitals(&mut engine);
        }
        assert_eq!(engine.status(), Status::PromptsOff);
        engine.observe(
            "Char.Prompt",
            char_prompt(true, "%n%P%C<%hhp %mm %vmv> ", ""),
            at(),
        );
        assert_eq!(engine.status(), Status::Matching, "no misses were kept");
    }

    #[test]
    fn a_profile_without_a_capture_misses_nothing() {
        let mut engine = PromptEngine::default();
        engine.connect(true);
        for _ in 0..5 {
            vitals(&mut engine);
        }
        assert_eq!(engine.status(), Status::NoCapture);
        engine.set_config(following("<%hhp> "));
        assert_eq!(engine.status(), Status::Matching);
    }

    #[test]
    fn other_games_count_misses_by_send() {
        let mut engine = PromptEngine::default();
        engine.connect(false);
        engine.set_config(PromptConfig {
            capture: CaptureConfig::Regex(RegexCapture {
                lines: vec![r"^<(?<hp>\d+)hp> $".into()],
                ..RegexCapture::default()
            }),
            ..PromptConfig::default()
        });
        // Char.Vitals starts no miss here, and a send with no reply is
        // none either.
        for _ in 0..4 {
            vitals(&mut engine);
            engine.note_send("look\r\n", 0);
        }
        assert_eq!(engine.status(), Status::Matching);
        for _ in 0..3 {
            engine.note_text();
            engine.note_send("look\r\n", 0);
        }
        assert_eq!(engine.status(), Status::NotMatching);
        engine.note_text();
        engine.note_prompt(at());
        assert_eq!(engine.status(), Status::Matching);
    }

    #[test]
    fn a_new_table_keeps_the_values() {
        let mut engine = playing();
        engine.set_config(PromptConfig::from_legacy(false, "%mana"));
        assert_eq!(engine.vars.prompt_vars().len(), 2);
        assert!(engine.vars.new_build());
    }
}
