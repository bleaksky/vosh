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
    /// The design pushes part of a row to the right edge, `%{right}`.
    right: bool,
    /// The columns of the terminal your prompt shows in, as the session
    /// last heard them, which a push to the right edge reaches to.
    cols: Option<usize>,
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
        let template = Template::parse(&self.config.template);
        let reads = template.reads();
        self.stage.set_reads(&reads);
        self.clock = Clock::of(&reads);
        self.right = template
            .tokens()
            .iter()
            .any(|t| t.kind == crate::template::TokenKind::Right);
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

    /// The terminal your prompt shows in is `cols` wide now. A connection
    /// and another profile keep it, since the window stays.
    pub fn set_cols(&mut self, cols: usize) {
        self.cols = Some(cols);
    }

    /// The columns of the terminal your prompt shows in, None until the
    /// session heard them.
    pub fn cols(&self) -> Option<usize> {
        self.cols
    }

    /// Vosh draws a design that pushes part of a row to the right edge,
    /// so a new width moves that part and your prompt draws again.
    pub fn pushes_right(&self) -> bool {
        self.draws() && self.right
    }

    /// How your design draws now: the width a push to the right edge
    /// reaches to, with the labels of values that have nothing to show
    /// when `placeholders` asks for them.
    pub fn render_options(&self, placeholders: bool) -> crate::render::RenderOptions {
        crate::render::RenderOptions {
            placeholders,
            cols: self.cols,
        }
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
mod tests;
