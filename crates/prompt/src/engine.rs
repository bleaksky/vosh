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
use crate::aabahran::{and_list, CompileError, Origin, Which, Who};
use crate::capture;
use crate::config::{AabahranCapture, CaptureConfig, CaptureSource, PromptConfig};
use crate::gmcp::{CharPrompt, Observed, CHAR_STATE, CHAR_STATUS};
use crate::overrides::PromptPreview;
use crate::stage::Stage;
use crate::state::{OpenRowState, PromptState};
use crate::template::Template;
use crate::vars::{self, forsaken_lands, Vars, Vosh};

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
    /// The catalog names of the parts of your design the capture fed
    /// before it took the settings and nothing feeds now: no code in the
    /// new settings, and no package that sends it this session. Vosh says
    /// so once, and the card and Settings ring those parts (P14). Left
    /// out of the event when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub lost: Vec<String>,
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

/// The clock pieces a design reads (decision 6). While it reads one, the
/// session repaints your idle prompt as what the piece shows changes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Clock {
    /// The tick, which counts down a second at a time.
    pub tick: bool,
    /// The local time or the date.
    pub wall: bool,
}

impl Clock {
    /// The clock pieces among the fields a design reads.
    fn of(reads: &std::collections::BTreeSet<crate::template::FieldRef>) -> Self {
        let reads = |name: &str| reads.iter().any(|field| field.name == name);
        Self {
            tick: reads("tick"),
            wall: reads("time") || reads("date"),
        }
    }
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

/// Why the migrated capture kept its pattern when the game showed your
/// PROMPT.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Kept {
    /// A color code runs into a code in the settings the game sent, so
    /// they do not compile.
    Compile(CompileError),
    /// The pattern fills values under names of its own, in the order it
    /// reads them. No code fills them, so the design and any script that
    /// reads them would go blank.
    Unknown(Vec<String>),
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
    /// The open card chose Aabahran's code reader on a host Vosh does not
    /// know, with More > Use Forsaken Lands prompt codes…, so the Forsaken
    /// Lands rules hold until the card lets it go or another profile takes
    /// over (D17). A connection keeps it, since the card stays open across
    /// one.
    reader: bool,
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
    /// Why the migrated capture kept its pattern when the game last
    /// showed your PROMPT this session.
    kept_pattern: Option<Kept>,
    /// What the open card shows on your prompt in place of the live
    /// render.
    preview: Option<PromptPreview>,
    /// The newest entry of the candidates ring and the pulse it came in,
    /// so a capture you choose reads the prompt already on screen.
    newest: Option<(u64, u64)>,
    /// The clock pieces the design reads.
    clock: Clock,
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
        let recaptured = self.config.capture != config.capture;
        if recaptured {
            // A new capture starts with no misses, and a reason the old
            // one kept its pattern no longer holds.
            self.misses.count = 0;
            self.kept_pattern = None;
        }
        self.config = config;
        self.compile();
        self.apply_rules();
        if recaptured {
            self.read_newest();
        }
    }

    /// Read the newest prompt in the candidates ring with the capture just
    /// taken, when it came in this pulse. On first use the game's prompt
    /// came before the profile read any, and the values only it shows,
    /// such as Wizi, then read at once rather than at the next prompt.
    /// They last the pulse, as any capture does.
    fn read_newest(&mut self) {
        let Some((id, pulse)) = self.newest else {
            return;
        };
        if pulse != self.vars.gmcp().pulse() {
            return;
        }
        let block = self
            .stage
            .ring()
            .last()
            .filter(|entry| entry.id == id)
            .and_then(|entry| {
                self.stage
                    .recognize(&entry.raw, &entry.plain, crate::stage::End::Line)
            });
        if let Some(block) = block {
            let raw = block.raw_text();
            self.vars.capture(crate::vars::Capture {
                values: block.values,
                raw: Some(raw),
            });
        }
    }

    /// Compile the table for the stage: the capture for who you are, and
    /// the fields the design reads.
    fn compile(&mut self) {
        self.stage.set_show(self.config.show);
        self.stage.set_capture_for(&self.config.capture, self.who);
        let reads = Template::parse(&self.config.template).reads();
        self.stage.set_reads(&reads);
        self.clock = Clock::of(&reads);
    }

    /// Keep a GMCP packet. Char.Status and Char.State say who the prompt
    /// is for, and a change compiles the capture again. Char.Prompt is
    /// the game's own word on your prompt settings, which an aabahran
    /// capture follows and a migrated capture switches to (D10).
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
    /// differ from the ones it holds, with the source and the time, and a
    /// migrated capture switches to them, prompts off or on.
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
        let (applied, lost) = self.take_settings(
            Some(&prompt.prompt),
            Some(&prompt.fprompt),
            CaptureSource::Gmcp,
            at,
        );
        GamePromptSeen {
            kind: SeenKind::Gmcp,
            text,
            applied,
            lost,
        }
    }

    /// Hand settings the game showed to the capture. An aabahran capture
    /// that follows the game takes them when they differ from the ones it
    /// holds. A migrated capture switches to them (see
    /// [`PromptEngine::switch_migrated`]). Any other capture takes
    /// nothing. Returns whether the capture took them, and the parts of
    /// your design nothing feeds since (see [`GamePromptSeen::lost`]).
    fn take_settings(
        &mut self,
        prompt: Option<&str>,
        fprompt: Option<&str>,
        source: CaptureSource,
        at: DateTime<FixedOffset>,
    ) -> (bool, Vec<String>) {
        let before = self.fed();
        let applied = self.take_settings_from(prompt, fprompt, source, at);
        let lost = if applied {
            self.lost_since(&before)
        } else {
            Vec::new()
        };
        (applied, lost)
    }

    /// The catalog names the capture fills now.
    fn fed(&self) -> Vec<String> {
        self.stage
            .recognizer()
            .map(capture::Recognizer::reads)
            .unwrap_or_default()
            .iter()
            .map(|name| vars::feeds(name).to_string())
            .collect()
    }

    /// The parts of your design that `before`, what the capture filled
    /// then, fed and nothing feeds now: no code of the capture, and no
    /// package that sends it this session. A package only the new build
    /// sends feeds it only on the new build.
    fn lost_since(&self, before: &[String]) -> Vec<String> {
        let now = self.fed();
        let mut lost: Vec<String> = Vec::new();
        for field in Template::parse(&self.config.template).reads() {
            let Some(entry) = vars::entry_for(&field).filter(|e| !e.param) else {
                continue;
            };
            let name = entry.name;
            let sent = entry.package.is_some_and(|package| {
                self.vars.gmcp().has(package) && (!entry.new_build || self.vars.new_build())
            });
            if before.iter().any(|n| n == name)
                && !now.iter().any(|n| n == name)
                && !sent
                && !lost.iter().any(|n| n == name)
            {
                lost.push(name.to_string());
            }
        }
        lost
    }

    /// The body of [`PromptEngine::take_settings`].
    fn take_settings_from(
        &mut self,
        prompt: Option<&str>,
        fprompt: Option<&str>,
        source: CaptureSource,
        at: DateTime<FixedOffset>,
    ) -> bool {
        if self.config.capture.is_migrated() {
            return self.switch_migrated(prompt, fprompt, source, at);
        }
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

    /// Switch the pattern the move from a capture trigger wrote to
    /// Aabahran's codes, which follow the game from then on (D10, James
    /// on 2026-09-30). The first PROMPT the game shows under the
    /// Forsaken Lands rules does it, from Char.Prompt or from the reply
    /// the observer reads. A fight prompt alone says too little. Without
    /// one from the game, the fight prompt is the one the game showed
    /// this session, or none. The design and the drawing switch stay as
    /// they are. The pattern stays, and [`PromptEngine::kept_pattern`]
    /// says why, when it fills a value under a name Vosh does not know,
    /// which no code fills, or when the settings do not compile. A value
    /// under a name Vosh knows that the codes leave out is one your
    /// PROMPT no longer shows, so it does not hold the switch back.
    /// Returns whether it switched.
    fn switch_migrated(
        &mut self,
        prompt: Option<&str>,
        fprompt: Option<&str>,
        source: CaptureSource,
        at: DateTime<FixedOffset>,
    ) -> bool {
        let Some(prompt) = prompt else {
            return false;
        };
        if !self.forsaken() {
            return false;
        }
        let unknown = self.unknown_values();
        if !unknown.is_empty() {
            self.kept_pattern = Some(Kept::Unknown(unknown));
            return false;
        }
        let fprompt = fprompt
            .map(str::to_string)
            .or_else(|| {
                self.observer
                    .seen
                    .as_ref()
                    .and_then(|seen| seen.fprompt.clone())
            })
            .unwrap_or_default();
        match codes_from_game(prompt, &fprompt, source, at, self.who) {
            Ok(codes) => {
                let mut config = self.config.clone();
                config.capture = CaptureConfig::Aabahran(codes);
                self.set_config(config);
                true
            }
            Err(error) => {
                self.kept_pattern = Some(Kept::Compile(error));
                false
            }
        }
    }

    /// The values the capture's pattern fills under names Vosh does not
    /// know, in the order it reads them. None for any other capture.
    fn unknown_values(&self) -> Vec<String> {
        let CaptureConfig::Regex(pattern) = &self.config.capture else {
            return Vec::new();
        };
        capture::fills(pattern)
            .into_iter()
            .filter(|name| !vars::known(name))
            .collect()
    }

    /// Why the migrated capture kept its pattern when the game last
    /// showed your PROMPT this session, as one sentence for `#prompt`.
    pub fn kept_pattern(&self) -> Option<String> {
        let because = match self.kept_pattern.as_ref()? {
            Kept::Compile(error) => {
                let setting = match error.which {
                    Which::Prompt => "prompt",
                    Which::Fight => "fight prompt",
                };
                format!(
                    "a color code runs into {} in the {setting} the game sent",
                    error.code
                )
            }
            Kept::Unknown(names) => match names.as_slice() {
                [name] => {
                    format!("it fills a value named {name}, and no prompt code fills that name")
                }
                _ => format!(
                    "it fills values named {}, and no prompt code fills those names",
                    and_list(names)
                ),
            },
        };
        Some(format!(
            "Vosh kept the pattern from your old capture trigger because {because}."
        ))
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

    /// The status and when Vosh last read your prompt, as
    /// `session://prompt-status` carries them.
    pub fn status_report(&self) -> StatusReport {
        StatusReport {
            status: self.status(),
            last_match_at: self.misses.last_match_at.map(stamp),
        }
    }

    /// Everything the card reads about your prompt now, for
    /// `prompt_state_get` and `session://prompt-state`: each field with its
    /// state and source, the status, the new build sign and the open row
    /// with where each piece of the design landed in it.
    pub fn state(&self, vosh: &Vosh) -> PromptState {
        let reads = self
            .stage
            .recognizer()
            .map(crate::capture::Recognizer::reads)
            .unwrap_or_default();
        PromptState {
            catalog: crate::state::catalog(&self.vars, vosh, &reads),
            status: self.status_report(),
            new_build: self.vars.new_build(),
            forsaken: self.forsaken(),
            open_row: self.stage.open_row().map(|open| {
                let block = self.stage.last_raw();
                let replaced = block.map(|b| b.replaced.clone()).unwrap_or_default();
                OpenRowState {
                    gen: open.gen,
                    spans: open.spans.clone(),
                    plain: open.plain.clone(),
                    raw_lines: replaced
                        .iter()
                        .filter_map(|i| block.and_then(|b| b.lines.get(*i)))
                        .map(|line| line.plain.clone())
                        .collect(),
                    raw_from: replaced.first().copied().unwrap_or(0),
                }
            }),
            packages: self.vars.gmcp().packages().map(str::to_string).collect(),
        }
    }

    /// The status when it changed since the last call, for one
    /// `session://prompt-status` per socket read.
    pub fn take_status_change(&mut self) -> Option<StatusReport> {
        let report = self.status_report();
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
    /// goes to an aabahran capture that follows the game, and a PROMPT it
    /// shows switches a migrated capture. The reply to `prompt off` never
    /// does either, and raises prompts off instead.
    pub fn observe_line(&mut self, raw: &[u8], plain: &str, at: DateTime<FixedOffset>) {
        if !self.observing(at.timestamp_millis()) {
            return;
        }
        let Some(reply) = observer::reply(plain) else {
            return;
        };
        let text = observer::setting(reply, raw);
        let (kind, (applied, lost)) = match reply.kind {
            ReplyKind::Off => {
                self.observer.off_line = true;
                self.prompts_off = true;
                (SeenKind::Off, (false, Vec::new()))
            }
            ReplyKind::Prompt => {
                if self.observer.sent_off || self.observer.off_line {
                    self.prompts_off = true;
                    return;
                }
                // Any `prompt` but `prompt off` turns prompts on.
                self.prompts_off = false;
                // After `prompt off` an older build shows the buffer it
                // never filled, control bytes and all. That is no
                // setting of yours, so the codes you have stay.
                if leftover_buffer(&text) {
                    return;
                }
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
            lost,
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

    /// Show what the open card shows on your prompt in place of the live
    /// render, or the live render again with None. A preview that draws
    /// the live prompt as it is counts as None. It lasts until the card
    /// clears it or the connection goes, and the next repaint shows it.
    pub fn set_preview(&mut self, preview: Option<PromptPreview>) {
        self.preview = preview.filter(|p| !p.is_live());
        // The card shows a preview for as long as it is open, so the row
        // that draws your design borrows the band while one is set.
        self.stage.set_card(self.preview.is_some());
    }

    /// What the open card shows on your prompt, while it shows anything
    /// but the live render.
    pub fn preview(&self) -> Option<&PromptPreview> {
        self.preview.as_ref()
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

    /// The clock pieces your design draws: None while it reads none, or
    /// while Vosh draws no design.
    pub fn clock(&self) -> Option<Clock> {
        let clock = self.clock;
        (self.draws() && (clock.tick || clock.wall)).then_some(clock)
    }

    /// Where your prompt shows, `[prompt] show`.
    pub fn show(&self) -> crate::config::PromptShow {
        self.config.show
    }

    /// The rows the band above the command line keeps while your prompt
    /// shows pinned. See [`Stage::zone`].
    pub fn zone(&self) -> usize {
        self.stage
            .zone(self.draws(), &Template::parse(&self.config.template))
    }

    /// Record a candidate in the ring, on a send or a GA or EOR, with
    /// whether drawing is on and whether the profile has a capture. See
    /// [`Stage::record`].
    pub fn record(&mut self, partial: Option<(&[u8], &str)>, at_ms: i64) {
        let draw = self.draws();
        let capture = !self.config.capture.is_none();
        self.stage.record(partial, at_ms, draw, capture);
        if let Some(entry) = self.stage.ring().last() {
            if self.newest.map(|(id, _)| id) != Some(entry.id) {
                self.newest = Some((entry.id, self.vars.gmcp().pulse()));
            }
        }
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
    /// Forsaken Lands, the capture reads Aabahran's codes, or the open
    /// card chose the code reader.
    pub fn forsaken(&self) -> bool {
        self.vars.forsaken()
    }

    /// The open card chose Aabahran's code reader, or let it go (D17).
    /// While it holds, the Forsaken Lands rules hold, so the observer
    /// reads the game's replies to `prompt` for the card's fields.
    pub fn set_reader(&mut self, on: bool) {
        self.reader = on;
        self.apply_rules();
    }

    /// A connection opened. It starts with no packets and no values.
    /// `known_host` is whether the host is The Forsaken Lands. A preview
    /// the open card set stays, since the card can be open as you
    /// connect (D9), and the first prompt draws what it shows.
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
        self.kept_pattern = None;
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
    /// go with it, and so do the open row, the candidates ring and the
    /// card's preview. The webview clears its copy of the prompt vars on
    /// the disconnect.
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
        self.kept_pattern = None;
        self.preview = None;
        self.stage.set_card(false);
        self.apply_rules();
    }

    /// Another profile is taking over the connection. The GMCP packets
    /// and the new build sign stay, since the connection did not change,
    /// and the values the last profile's prompt read go. The next
    /// [`PromptEngine::set_config`] hands over the new profile's table.
    pub fn switch_profile(&mut self) {
        // The card opens again for the new profile, from its first step.
        self.reader = false;
        // The new profile starts with no values read, so its capture
        // reads nothing from the prompt already on screen.
        self.newest = None;
        let forsaken = self.rules();
        self.vars.switch_profile(forsaken);
        self.kept_pattern = None;
        // The triggers the last profile's prompt lost name nothing of
        // this one's.
        self.stage.forget_gags_without_reader();
    }

    fn rules(&self) -> bool {
        self.reader || forsaken_lands(self.known_host, self.config.capture.is_aabahran())
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

/// The capture a migrated one becomes when the game shows your settings:
/// Aabahran's codes as the game stores them, following the game, learned
/// from `source` at `at`. An error when they do not compile for `who`.
pub fn codes_from_game(
    prompt: &str,
    fprompt: &str,
    source: CaptureSource,
    at: DateTime<FixedOffset>,
    who: Who,
) -> Result<AabahranCapture, CompileError> {
    crate::aabahran::compile(prompt, fprompt, Origin::Stored, who)?;
    Ok(AabahranCapture {
        prompt: prompt.to_string(),
        fprompt: fprompt.to_string(),
        follow_game: true,
        seen_at: Some(stamp(at)),
        source: Some(source),
    })
}

/// A prompt setting the game showed that holds a control character.
/// No PROMPT you type can, so it is the buffer an older build's
/// `prompt off` left unfilled.
fn leftover_buffer(setting: &str) -> bool {
    setting.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AabahranCapture, CaptureConfig, RegexCapture};
    use crate::stage::End;
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
    fn the_state_reports_the_fields_the_status_and_the_packages() {
        let mut engine = playing();
        engine.set_config(PromptConfig {
            capture: CaptureConfig::Aabahran(AabahranCapture {
                prompt: "<%hhp %mm> ".into(),
                ..AabahranCapture::default()
            }),
            ..PromptConfig::from_legacy(true, "%hp")
        });
        let state = engine.state(&Vosh::default());
        assert!(state.new_build);
        assert_eq!(state.status, engine.status_report());
        assert_eq!(state.packages, ["Char.Prompt", "Char.Vitals"]);
        assert_eq!(state.open_row, None);
        let hp = state.catalog.iter().find(|f| f.name == "hp").expect("hp");
        assert!(hp.in_prompt);
        let mood = state.catalog.iter().find(|f| f.name == "mood");
        assert!(mood.is_some(), "a script name lists too");
        let gold = state
            .catalog
            .iter()
            .find(|f| f.name == "gold")
            .expect("gold");
        assert!(!gold.in_prompt);
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
    fn the_card_lends_the_band_while_its_preview_lasts() {
        use crate::overrides::PromptPreview;
        let mut engine = PromptEngine::default();
        engine.connect(true);
        assert!(!engine.stage.card_open());
        let labels = PromptPreview {
            placeholders: true,
            ..PromptPreview::default()
        };
        engine.set_preview(Some(labels.clone()));
        assert!(engine.stage.card_open());
        // A preview that draws the live prompt as it is counts as none.
        engine.set_preview(Some(PromptPreview::default()));
        assert!(!engine.stage.card_open());
        engine.set_preview(Some(labels));
        engine.disconnect();
        assert!(!engine.stage.card_open());
    }

    #[test]
    fn the_preview_the_card_set_offline_lasts_through_the_connect() {
        use crate::overrides::{Preview, PromptPreview};
        let mut engine = PromptEngine::default();
        let placeholders = PromptPreview {
            placeholders: true,
            ..PromptPreview::default()
        };
        engine.set_preview(Some(placeholders.clone()));
        // The card stays open as you connect, so the first prompt draws
        // what it shows.
        engine.connect(true);
        assert_eq!(engine.preview(), Some(&placeholders));
        let low = PromptPreview {
            preview: Some(Preview::LowHealth),
            ..PromptPreview::default()
        };
        engine.set_preview(Some(low));
        // The connection going clears it.
        engine.disconnect();
        assert_eq!(engine.preview(), None);
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
    fn a_first_capture_reads_the_prompt_already_on_screen() {
        // First use on a profile that read nothing: the game's prompt came
        // and went into the ring before you chose its codes. The values
        // only the prompt shows, such as Wizi, read from it at once, so
        // the start list draws them without waiting for the next prompt.
        let line = "(Wizi 60) [1020/1020hp] ";
        let first = || {
            let mut engine = PromptEngine::default();
            engine.connect(true);
            engine.observe("Char.Vitals", json!({"hp": 1020, "maxhp": 1020}), at());
            // The prompt shows as sent, and its GA puts it in the ring.
            let mut out = crate::stage::Output::new(false);
            let _ = engine.stage.paint_partial(&mut out, line.as_bytes(), None);
            engine.record(Some((line.as_bytes(), line)), 5);
            engine
        };
        let mut engine = first();
        engine.set_config(following("[%h/%Hhp] "));
        let vars = engine.vars.prompt_vars();
        assert_eq!(vars.get("wizi").map(String::as_str), Some("60"));
        assert_eq!(vars.get("hp").map(String::as_str), Some("1020"));

        // A pulse after it means the line is older than what the game
        // sent since, so it reads nothing.
        let mut engine = first();
        engine.observe("Char.Vitals", json!({"hp": 900, "maxhp": 1020}), at());
        engine.set_config(following("[%h/%Hhp] "));
        assert_eq!(engine.vars.prompt_vars().get("wizi"), None);

        // Codes that do not read the line read nothing from it either.
        let mut engine = first();
        engine.set_config(following("<%hhp> "));
        assert_eq!(engine.vars.prompt_vars().get("wizi"), None);

        // Another profile taking over starts with no values read, even
        // when its codes read the line (section 5).
        let mut engine = first();
        engine.switch_profile();
        engine.set_config(following("[%h/%Hhp] "));
        assert_eq!(engine.vars.prompt_vars().get("wizi"), None);
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
                lost: Vec::new(),
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
                lost: Vec::new(),
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
                    lost: Vec::new(),
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
    fn a_new_prompt_names_the_parts_of_your_design_nothing_feeds_any_more() {
        // Same as the game reads your tank's health. On an older build
        // only %P sends it, so dropping %P in the game leaves that part
        // blank, and Vosh says so once (P14).
        let mut engine = older_build("%n%P%C[%h/%Hhp]%c");
        let mut config = engine.config().clone();
        config.template = "%{if:tank}%tank: %{tank_hp:game}%nl%{end}[%hp/%{maxhp}hp]".into();
        engine.set_config(config);
        engine.observe(
            "Char.Vitals",
            json!({"hp": 1020, "maxhp": 1020, "mana": 800, "maxmana": 800, "move": 930, "maxmove": 930}),
            at_ms(SENT),
        );
        engine.note_send("prompt %n%C[%h/%Hhp]%c\r\n", SENT);
        line(&mut engine, "Prompt set to %n%C[%h/%Hhp]%c", 40);
        let seen = engine.take_seen();
        assert_eq!(seen.len(), 1);
        assert!(seen[0].applied);
        // Health still comes from Char.Vitals, so only the tank's health
        // is lost.
        assert_eq!(seen[0].lost, ["tank_hp"]);
        // A change that keeps every part fed names none.
        engine.note_send("prompt %n%P%C[%h/%Hhp]%c\r\n", SENT + 100);
        line(&mut engine, "Prompt set to %n%P%C[%h/%Hhp]%c", 140);
        assert!(engine.take_seen()[0].lost.is_empty());
    }

    #[test]
    fn on_the_new_build_char_combat_keeps_your_tanks_health() {
        let mut engine = PromptEngine::default();
        engine.connect(true);
        let mut config = following("%n%P%C[%h/%Hhp]%c");
        config.template = "%{if:tank}%tank: %{tank_hp:game}%nl%{end}[%hp]".into();
        engine.set_config(config);
        engine.observe(
            "Char.Prompt",
            char_prompt(true, "%n%P%C[%h/%Hhp]%c", ""),
            at(),
        );
        engine.observe(
            "Char.Combat",
            json!({"target": "a rat", "hp_pct": 80, "condition": "fine", "tank": {"name": "Tester", "hp_pct": 90}}),
            at(),
        );
        let _ = engine.take_seen();
        engine.observe(
            "Char.Prompt",
            char_prompt(true, "%n%C[%h/%Hhp]%c", ""),
            at(),
        );
        let seen = engine.take_seen();
        assert!(seen[0].applied);
        assert!(seen[0].lost.is_empty(), "{:?}", seen[0].lost);
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
    fn prompt_after_prompt_off_keeps_your_codes_over_the_leftover_buffer() {
        // An older build's prompt off stores a buffer it never filled,
        // and prompt with no argument then shows it, control bytes and
        // all. That is no setting of yours: prompts come back on and the
        // codes you have stay.
        for reply in ["Current prompt: \u{1}\u{2}", "Prompt set to \u{1}\u{2}"] {
            let mut engine = older_build("<%hhp> ");
            engine.note_send("prompt off\r\n", SENT);
            line(&mut engine, "You will no longer see prompts.", 5);
            line(&mut engine, "Prompt set to \u{1}\u{2}", 6);
            assert!(engine.prompts_off());
            engine.take_seen();
            engine.note_send("prompt\r\n", SENT + 3_000);
            line(&mut engine, reply, 3_010);
            assert_eq!(codes(&engine).prompt, "<%hhp> ", "{reply:?}");
            assert!(!engine.prompts_off(), "{reply:?} turns prompts on");
            assert!(engine.take_seen().is_empty(), "{reply:?}");
        }
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

    #[test]
    fn the_code_reader_the_card_chose_reads_the_replies_on_another_host() {
        // A local server of The Forsaken Lands, with no capture yet. More
        // > Use Forsaken Lands prompt codes… gives it the rules (D17), so
        // the reply to prompt fills the card's fields.
        let mut engine = PromptEngine::default();
        engine.connect(false);
        engine.set_reader(true);
        assert!(engine.forsaken());
        engine.note_send("prompt\r\n", SENT);
        line(
            &mut engine,
            "Current prompt: %n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c",
            40,
        );
        let seen = engine.take_seen();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].kind, SeenKind::Prompt);
        assert_eq!(seen[0].text, "%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c");
        assert!(!seen[0].applied, "no capture takes it before you save");

        // The card stays open across a connection, and the rules with it.
        engine.disconnect();
        engine.connect(false);
        assert!(engine.forsaken());

        // Once the card lets it go, the host plays by its own rules.
        engine.set_reader(false);
        assert!(!engine.forsaken());
        engine.note_send("prompt\r\n", SENT + 3_000);
        line(&mut engine, "Current prompt: %h ", 3_040);
        assert!(engine.take_seen().is_empty());

        // Another profile taking over lets it go too.
        engine.set_reader(true);
        engine.switch_profile();
        assert!(!engine.forsaken());
    }

    fn vitals(engine: &mut PromptEngine) {
        engine.observe("Char.Vitals", json!({"hp": 10, "maxhp": 20}), at());
    }

    #[test]
    fn a_design_reads_a_clock_only_with_a_clock_piece_and_drawing_on() {
        let clock = |template: &str, draw: bool| {
            let mut engine = PromptEngine::default();
            engine.set_config(PromptConfig {
                draw,
                template: template.into(),
                ..following("<%hhp> ")
            });
            engine.clock()
        };
        let tick = Clock {
            tick: true,
            wall: false,
        };
        let wall = Clock {
            tick: false,
            wall: true,
        };
        assert_eq!(clock("<%hp> %tick", true), Some(tick));
        assert_eq!(clock("%{tick:bar:10} ", true), Some(tick));
        // A color that follows the tick changes with it too.
        assert_eq!(clock("%bg_tick%hp ", true), Some(tick));
        assert_eq!(clock("%{time:hms} ", true), Some(wall));
        assert_eq!(clock("%{date:md} ", true), Some(wall));
        assert_eq!(
            clock("%{if:tick}%tick%{end} %time", true),
            Some(Clock {
                tick: true,
                wall: true
            })
        );
        assert_eq!(clock("<%hp %mana %move> ", true), None);
        assert_eq!(clock(crate::DEFAULT_DESIGN, true), None);
        assert_eq!(clock("<%hp> %tick", false), None);
        assert_eq!(clock("", true), None);
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

    /// The pattern the old capture trigger held.
    const OLD_PATTERN: &str = r"\[(?<hp>\d+)/(?<maxhp>\d+)hp (?<mana>\d+)/(?<maxmana>\d+)mn (?<move>\d+)/(?<maxmove>\d+)mv\]";
    /// The PROMPT that pattern was written for, as the game stores it.
    const OLD: &str = "%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c";
    /// The PROMPT James set after the move, as the game stores it.
    const NEW: &str = "%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv (%K hp) %s [%S]> ";
    /// A design that draws in place of the prompt.
    const DESIGN: &str = "%{c:100,100,100}[%c_reset%s_italic%hp]";

    /// A table as the move from the capture trigger wrote it.
    fn migrated() -> PromptConfig {
        PromptConfig {
            draw: true,
            template: DESIGN.into(),
            previous_templates: vec!["%hp".into()],
            capture: CaptureConfig::Regex(RegexCapture {
                lines: vec![OLD_PATTERN.into()],
                settle: false,
                source: Some(CaptureSource::Migrated),
                ..RegexCapture::default()
            }),
            ..PromptConfig::default()
        }
    }

    /// A Forsaken Lands connection whose profile holds the migrated
    /// capture.
    fn moved() -> PromptEngine {
        let mut engine = PromptEngine::default();
        engine.connect(true);
        engine.set_config(migrated());
        engine
    }

    /// The table stays as it was apart from the capture.
    fn keeps_the_rest(engine: &PromptEngine) {
        let config = engine.config();
        assert!(config.draw);
        assert_eq!(config.template, DESIGN);
        assert_eq!(config.previous_templates, ["%hp"]);
    }

    #[test]
    fn the_first_char_prompt_switches_a_migrated_capture_to_the_codes() {
        let mut engine = moved();
        let revision = engine.revision();
        engine.observe("Char.Prompt", char_prompt(true, OLD, "`1%h``> "), at());
        assert_eq!(
            codes(&engine),
            AabahranCapture {
                prompt: OLD.into(),
                fprompt: "`1%h``> ".into(),
                follow_game: true,
                seen_at: Some("2026-09-29T12:58:02-05:00".into()),
                source: Some(CaptureSource::Gmcp),
            }
        );
        keeps_the_rest(&engine);
        assert!(engine.revision() > revision, "Settings reads it again");
        assert_eq!(
            engine.take_seen(),
            [GamePromptSeen {
                kind: SeenKind::Gmcp,
                text: OLD.into(),
                applied: true,
                lost: Vec::new(),
            }],
            "the toast follows"
        );
        assert!(engine.forsaken());
        assert_eq!(engine.kept_pattern(), None);
        // The codes compile at once, so the next prompt draws.
        let block = engine
            .stage
            .recognize(b"", "[1020/1020hp 800/800mn 930/930mv]", End::Line)
            .expect("the prompt");
        assert_eq!(block.values.get("maxmove").map(String::as_str), Some("930"));

        // From here it follows the game as any aabahran capture does.
        engine.observe("Char.Prompt", char_prompt(true, NEW, ""), at());
        assert_eq!(codes(&engine).prompt, NEW);
        assert!(engine.take_seen()[0].applied);
        let line = "(Wizi 60) (Incog 60) [1020/1020hp 800/800mn 930/930mv (100 hp) common [std]> ";
        let block = engine
            .stage
            .recognize(b"", line, End::Settled)
            .expect("the new prompt");
        assert_eq!(block.values.get("wizi").map(String::as_str), Some("60"));
        assert_eq!(block.values.get("hp_pct").map(String::as_str), Some("100"));
        keeps_the_rest(&engine);
    }

    #[test]
    fn the_codes_from_the_game_keep_the_settings_as_sent_and_follow_the_game() {
        let got = codes_from_game(
            NEW,
            "`1%h``hp> ",
            CaptureSource::Session,
            at(),
            Who::default(),
        )
        .expect("they compile");
        assert_eq!(
            got,
            AabahranCapture {
                prompt: NEW.into(),
                fprompt: "`1%h``hp> ".into(),
                follow_game: true,
                seen_at: Some("2026-09-29T12:58:02-05:00".into()),
                source: Some(CaptureSource::Session),
            }
        );
        let error = codes_from_game("<`%h> ", "", CaptureSource::Gmcp, at(), Who::default())
            .expect_err("a color runs into %h");
        assert_eq!(error.code, "%h");
        assert_eq!(error.which, Which::Prompt);
    }

    #[test]
    fn prompts_off_still_switches_a_migrated_capture() {
        let mut engine = moved();
        engine.observe("Char.Prompt", char_prompt(false, OLD, ""), at());
        assert_eq!(codes(&engine).prompt, OLD);
        assert!(engine.prompts_off());
        assert_eq!(engine.status(), Status::PromptsOff);
        assert!(engine.take_seen()[0].applied);
    }

    #[test]
    fn only_the_migrated_capture_switches() {
        let typed = PromptConfig {
            capture: CaptureConfig::Regex(RegexCapture {
                lines: vec![OLD_PATTERN.into()],
                source: Some(CaptureSource::Typed),
                ..RegexCapture::default()
            }),
            ..migrated()
        };
        let unsourced = PromptConfig {
            capture: CaptureConfig::Regex(RegexCapture {
                lines: vec![OLD_PATTERN.into()],
                ..RegexCapture::default()
            }),
            ..migrated()
        };
        let none = PromptConfig {
            capture: CaptureConfig::None,
            ..migrated()
        };
        for config in [typed, unsourced, none] {
            let mut engine = PromptEngine::default();
            engine.connect(true);
            engine.set_config(config.clone());
            engine.observe("Char.Prompt", char_prompt(true, NEW, ""), at());
            assert_eq!(*engine.config(), config);
            assert!(!engine.take_seen()[0].applied);
            engine.note_send("prompt x\r\n", SENT);
            line(&mut engine, "Prompt set to x ", 5);
            assert_eq!(*engine.config(), config);
        }
    }

    #[test]
    fn a_migrated_capture_switches_only_under_the_forsaken_lands_rules() {
        let mut engine = PromptEngine::default();
        engine.connect(false);
        engine.set_config(migrated());
        assert!(!engine.forsaken());
        engine.observe("Char.Prompt", char_prompt(true, OLD, ""), at());
        assert_eq!(*engine.config(), migrated());
        assert!(!engine.take_seen()[0].applied);
    }

    #[test]
    fn a_prompt_that_does_not_compile_keeps_the_pattern_and_says_why() {
        let mut engine = moved();
        engine.observe("Char.Prompt", char_prompt(true, "<`%h> ", ""), at());
        assert_eq!(*engine.config(), migrated());
        assert!(engine.stage.has_recognizer(), "the pattern still reads");
        assert!(!engine.take_seen()[0].applied, "no toast");
        assert_eq!(
            engine.kept_pattern().as_deref(),
            Some("Vosh kept the pattern from your old capture trigger because a color code runs into %h in the prompt the game sent.")
        );
        // A fight prompt that does not compile says so.
        engine.observe("Char.Prompt", char_prompt(true, OLD, "`(2%f1 "), at());
        assert_eq!(*engine.config(), migrated());
        assert_eq!(
            engine.kept_pattern().as_deref(),
            Some("Vosh kept the pattern from your old capture trigger because a color code runs into %f1 in the fight prompt the game sent.")
        );
        // The next prompt that compiles switches it, and the reason goes.
        engine.observe("Char.Prompt", char_prompt(true, OLD, ""), at());
        assert_eq!(codes(&engine).prompt, OLD);
        assert_eq!(engine.kept_pattern(), None);

        // A new connection starts with no reason.
        let mut engine = moved();
        engine.observe("Char.Prompt", char_prompt(true, "<`%h> ", ""), at());
        assert!(engine.kept_pattern().is_some());
        engine.disconnect();
        assert_eq!(engine.kept_pattern(), None);
        assert_eq!(*engine.config(), migrated());
    }

    #[test]
    fn the_reply_to_your_prompt_switches_a_migrated_capture_without_char_prompt() {
        let mut engine = moved();
        // A fight prompt alone says nothing of your PROMPT.
        engine.note_send("fprompt\r\n", SENT);
        line(&mut engine, "Current fight prompt: <%hhp fight> ", 5);
        assert_eq!(*engine.config(), migrated());
        assert!(!engine.take_seen()[0].applied);

        engine.note_send("prom x\r\n", SENT + 3_000);
        line(&mut engine, "Prompt set to <%hhp %mm> ", 3_010);
        let got = codes(&engine);
        assert_eq!(got.prompt, "<%hhp %mm> ");
        assert_eq!(got.fprompt, "<%hhp fight> ", "the fight prompt it showed");
        assert!(got.follow_game);
        assert_eq!(got.source, Some(CaptureSource::Session));
        assert!(got.seen_at.is_some());
        keeps_the_rest(&engine);
        assert_eq!(
            engine.take_seen(),
            [GamePromptSeen {
                kind: SeenKind::Prompt,
                text: "<%hhp %mm> ".into(),
                applied: true,
                lost: Vec::new(),
            }]
        );

        // What channels shows switches it too.
        let mut engine = moved();
        engine.note_send("channels\r\n", SENT);
        line(&mut engine, "Your current prompt is: <%hhp> ", 5);
        assert_eq!(codes(&engine).prompt, "<%hhp> ");
        assert_eq!(codes(&engine).fprompt, "");
    }

    #[test]
    fn the_reply_to_prompt_off_switches_nothing() {
        // Your own send said prompt off.
        let mut engine = moved();
        engine.note_send("prompt off\r\n", SENT);
        line(&mut engine, "Prompt set to \u{1}\u{2}", 5);
        assert_eq!(*engine.config(), migrated());
        assert!(engine.prompts_off());
        // An alias sent it, and the reply follows the line it prints first.
        let mut engine = moved();
        engine.note_send("quiet\r\n", SENT);
        line(&mut engine, "You will no longer see prompts.", 5);
        line(&mut engine, "Prompt set to \u{1}\u{2}", 6);
        assert_eq!(*engine.config(), migrated());
        // channels while prompts are off shows no setting of yours.
        engine.note_send("channels\r\n", SENT + 3_000);
        line(&mut engine, "Your current prompt is: \u{1}\u{2}", 3_010);
        assert_eq!(*engine.config(), migrated());
        // A reply after two seconds counts for nothing either.
        engine.note_send("prompt x\r\n", SENT + 4_000);
        line(&mut engine, "Prompt set to <%hhp> ", 4_000 + OBSERVE_MS + 1);
        assert_eq!(*engine.config(), migrated());
        assert!(engine.take_seen().iter().all(|s| !s.applied));
    }

    #[test]
    fn a_switch_applies_the_latest_char_prompt_to_a_migrated_capture() {
        let mut engine = PromptEngine::default();
        engine.connect(true);
        engine.set_config(PromptConfig::from_legacy(true, "%hp"));
        engine.observe("Char.Prompt", char_prompt(true, NEW, ""), at());
        assert!(!engine.take_seen()[0].applied, "default reads nothing");

        engine.switch_profile();
        engine.set_config(migrated());
        engine.follow_latest(at());
        assert_eq!(codes(&engine).prompt, NEW);
        assert_eq!(codes(&engine).source, Some(CaptureSource::Gmcp));
        keeps_the_rest(&engine);
        assert_eq!(engine.take_seen().len(), 1);

        // The reason a switch kept the pattern is the new profile's.
        let mut engine = PromptEngine::default();
        engine.connect(true);
        engine.set_config(migrated());
        engine.observe("Char.Prompt", char_prompt(true, "<`%h> ", ""), at());
        assert!(engine.kept_pattern().is_some());
        assert!(!engine.take_seen()[0].applied);
        engine.switch_profile();
        engine.set_config(PromptConfig::from_legacy(true, "%hp"));
        engine.follow_latest(at());
        assert_eq!(engine.kept_pattern(), None);
        assert!(engine.config().capture.is_none());
        engine.switch_profile();
        engine.set_config(migrated());
        engine.follow_latest(at());
        assert_eq!(*engine.config(), migrated());
        assert!(engine.kept_pattern().is_some());
        assert!(engine.take_seen().is_empty());
    }

    /// A migrated table whose pattern reads `pattern`, with `names` for
    /// its groups, and a design that reads `design`.
    fn migrated_with(pattern: &str, names: &[(&str, &str)], design: &str) -> PromptConfig {
        PromptConfig {
            draw: true,
            template: design.into(),
            previous_templates: Vec::new(),
            capture: CaptureConfig::Regex(RegexCapture {
                lines: vec![pattern.into()],
                names: names
                    .iter()
                    .map(|(group, var)| ((*group).to_string(), (*var).to_string()))
                    .collect(),
                source: Some(CaptureSource::Migrated),
                ..RegexCapture::default()
            }),
            ..PromptConfig::default()
        }
    }

    #[test]
    fn a_pattern_that_fills_a_name_no_code_fills_keeps_the_pattern_and_says_why() {
        // Groups named for themselves, outside the catalog, as the help
        // invites you to name them.
        let own = migrated_with(r"\[(?<h>\d+)/(?<mh>\d+)hp\]", &[], "[%h/%mh hp]");
        let mut engine = PromptEngine::default();
        engine.connect(true);
        engine.set_config(own.clone());
        engine.observe("Char.Prompt", char_prompt(true, "[%h/%Hhp]%c", ""), at());
        assert_eq!(*engine.config(), own, "the pattern and the design stay");
        assert!(!engine.take_seen()[0].applied, "no toast and no save");
        assert_eq!(
            engine.kept_pattern().as_deref(),
            Some("Vosh kept the pattern from your old capture trigger because it fills values named h and mh, and no prompt code fills those names.")
        );
        // The pattern still reads the values the design shows.
        let block = engine
            .stage
            .recognize(b"", "[100/200hp]", End::Line)
            .expect("the prompt");
        assert_eq!(block.values.get("h").map(String::as_str), Some("100"));
        assert_eq!(block.values.get("mh").map(String::as_str), Some("200"));
        // No PROMPT the game shows later changes that.
        engine.observe("Char.Prompt", char_prompt(true, NEW, ""), at());
        assert_eq!(*engine.config(), own);
        assert!(engine.kept_pattern().is_some());

        // A group the old trigger handed to a name of its own, through the
        // names the move wrote.
        let named = migrated_with(
            r"\[(?<h>\d+)/(?<maxhp>\d+)hp",
            &[("h", "health")],
            "HP=%health",
        );
        let mut engine = PromptEngine::default();
        engine.connect(true);
        engine.set_config(named.clone());
        engine.observe("Char.Prompt", char_prompt(true, OLD, ""), at());
        assert_eq!(*engine.config(), named);
        assert_eq!(
            engine.kept_pattern().as_deref(),
            Some("Vosh kept the pattern from your old capture trigger because it fills a value named health, and no prompt code fills that name.")
        );

        // The observer keeps it too, on a build without Char.Prompt, and
        // a profile switch that hands it the latest packet.
        let mut engine = PromptEngine::default();
        engine.connect(true);
        engine.set_config(named.clone());
        engine.note_send("prompt x\r\n", SENT);
        line(&mut engine, "Prompt set to <%hhp %Hmhp> ", 5);
        assert_eq!(*engine.config(), named);
        assert!(!engine.take_seen()[0].applied);
        assert!(engine.kept_pattern().is_some());
        let mut engine = PromptEngine::default();
        engine.connect(true);
        engine.observe("Char.Prompt", char_prompt(true, OLD, ""), at());
        engine.switch_profile();
        engine.set_config(named.clone());
        engine.follow_latest(at());
        assert_eq!(*engine.config(), named);
        assert!(engine.take_seen().iter().all(|s| !s.applied));
        assert!(engine.kept_pattern().is_some());
    }

    #[test]
    fn a_pattern_that_fills_only_names_vosh_knows_switches() {
        // A group the old trigger never read, another spelling of a max,
        // and a percent the codes fill all leave the switch alone.
        let known = migrated_with(
            r"\[(?<hp>\d+)/(?<mhp>\d+)hp (?<hp_pct>\d+)% (?<extra>\w+)\]",
            &[("extra", "")],
            "%hp/%mhp",
        );
        let mut engine = PromptEngine::default();
        engine.connect(true);
        engine.set_config(known);
        engine.observe("Char.Prompt", char_prompt(true, NEW, ""), at());
        assert_eq!(codes(&engine).prompt, NEW);
        assert_eq!(engine.kept_pattern(), None);

        // A value the game's PROMPT no longer shows goes with it, since
        // no code fills a name the catalog knows but the prompt leaves out.
        let mut engine = moved();
        engine.observe("Char.Prompt", char_prompt(true, "<%hhp %mm> ", ""), at());
        assert_eq!(codes(&engine).prompt, "<%hhp %mm> ");
        assert!(engine.take_seen()[0].applied);
    }

    #[test]
    fn a_reconnect_keeps_the_pattern_until_the_game_shows_your_prompt() {
        let mut engine = moved();
        // A link dead reconnect sends no Char.Prompt, and the pattern
        // reads the prompt as before.
        vitals(&mut engine);
        assert_eq!(*engine.config(), migrated());
        assert!(engine
            .stage
            .recognize(b"", "[1020/1020hp 800/800mn 930/930mv]", End::Line)
            .is_some());
        // prompt in the game sends it.
        engine.observe("Char.Prompt", char_prompt(true, OLD, ""), at());
        assert_eq!(codes(&engine).prompt, OLD);
    }

    /// An engine reading Aabahran's `prompt` and drawing `template` while
    /// `draw` is on.
    fn zoned(prompt: &str, template: &str, draw: bool) -> PromptEngine {
        let mut engine = PromptEngine::default();
        engine.set_config(PromptConfig {
            draw,
            template: template.into(),
            capture: CaptureConfig::Aabahran(AabahranCapture {
                prompt: prompt.into(),
                ..AabahranCapture::default()
            }),
            ..PromptConfig::default()
        });
        engine
    }

    #[test]
    fn the_band_keeps_the_rows_the_tallest_prompt_can_take() {
        // One line, no tank code: one row.
        assert_eq!(zoned("[%h/%Hhp]%c", "<%hp>", true).zone(), 1);
        // James's PROMPT prints the tank line above the vitals in a fight.
        // A design that reads nothing on it leaves it as sent.
        let james = "%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c";
        assert_eq!(zoned(james, "<%hp>", true).zone(), 2);
        // One that reads the tank takes it over.
        assert_eq!(zoned(james, "%tank %{tank_hp:pct}%% <%hp>", true).zone(), 1);
        // Every line break counts, inside a condition too.
        let detailed = "%{if:fight}%opponent%nl%{end}%hp";
        assert_eq!(zoned(james, detailed, true).zone(), 3);
        assert_eq!(
            zoned("[%h/%Hhp]%c", "%hp%{nl}%mana%nl%move", true).zone(),
            3
        );
        // A design that reads the prompt as sent takes its lines.
        assert_eq!(zoned(james, "%{raw}", true).zone(), 2);
        assert_eq!(zoned(james, "%{raw}%nl%hp", true).zone(), 3);
        // Not drawing, the game's own lines.
        assert_eq!(zoned(james, detailed, false).zone(), 2);
        assert_eq!(zoned("[%h/%Hhp]%c", "", true).zone(), 1);
        // No more than six.
        let tall = "%hp%nl%hp%nl%hp%nl%hp%nl%hp%nl%hp%nl%hp%nl%hp";
        assert_eq!(zoned(james, tall, true).zone(), crate::stage::ZONE_MAX);
    }

    #[test]
    fn a_regex_capture_and_no_capture_keep_one_row_and_its_breaks() {
        let mut engine = PromptEngine::default();
        assert_eq!(engine.zone(), 1);
        engine.set_config(PromptConfig {
            draw: true,
            template: "%hp%nl%mana".into(),
            capture: CaptureConfig::Regex(RegexCapture {
                lines: vec![r"\[(?<hp>\d+)hp\]".into()],
                ..RegexCapture::default()
            }),
            ..PromptConfig::default()
        });
        assert_eq!(engine.zone(), 2);
    }
}
