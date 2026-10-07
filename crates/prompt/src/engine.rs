//! The custom prompt of the live profile: its saved `[prompt]` table and
//! the session's variables (section 5, live state).
//!
//! The table lasts as long as the profile. The variables last as long as
//! the connection, and a profile switch keeps the GMCP packets while it
//! drops the values the last profile's prompt read.
//!
//! The card adds two methods to [`PromptEngine`], `state` in
//! [`crate::card::state`] and `kept_pattern` in [`crate::card::sentences`],
//! so this module never imports the card.

mod char_prompt;
mod replies;
mod status;

pub use replies::{GamePromptSeen, SeenKind};
pub use status::Status;

use std::collections::BTreeMap;

use chrono::{DateTime, FixedOffset, SecondsFormat};
use serde_json::Value as Json;

use crate::aabahran::Who;
use crate::config::{AabahranCapture, CaptureConfig, PromptConfig};
use crate::design::Template;
use crate::stage::Stage;
use crate::values::gmcp::{Observed, CHAR_STATE, CHAR_STATUS};
use crate::values::overrides::PromptPreview;
use crate::values::{forsaken_lands, Vars};

pub(crate) use char_prompt::Kept;
use replies::Observer;
use status::Misses;
pub(crate) use status::StatusReport;

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
    pub(crate) fn of(reads: &std::collections::BTreeSet<crate::design::FieldRef>) -> Self {
        let reads = |name: &str| reads.iter().any(|field| field.name == name);
        Self {
            tick: reads("tick"),
            wall: reads("time") || reads("date"),
        }
    }
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
    pub(crate) kept_pattern: Option<Kept>,
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
    /// Settings save or an edit hands it over. A design that follows the
    /// game is written from the table's codes for who you are, so a
    /// change of codes writes it again. The session's values stay, the
    /// capture compiles, and the Forsaken Lands rules follow it.
    pub fn set_config(&mut self, mut config: PromptConfig) {
        config.mirror_game(self.who);
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

    /// Take what you chose in `chosen`, the table another engine on the
    /// same profile holds after you changed it there (Q29 of the sessions
    /// review). The switch, where your prompt shows, whether the design
    /// follows the game, a design you wrote, the earlier designs and a
    /// capture you set come from `chosen`. What this engine's own game
    /// supplied stays: its Aabahran codes, with when and how Vosh learned
    /// them, while the chosen codes follow the game too, a capture its
    /// game decides while the chosen one is the game's as well, and the
    /// design written from its codes while the design follows the game.
    pub fn take_choice(&mut self, mut chosen: PromptConfig) {
        chosen.capture = match (chosen.capture, &self.config.capture) {
            (CaptureConfig::Aabahran(theirs), CaptureConfig::Aabahran(mine))
                if theirs.follow_game =>
            {
                CaptureConfig::Aabahran(AabahranCapture {
                    follow_game: true,
                    ..mine.clone()
                })
            }
            (theirs, mine) if theirs.game_decides() && mine.game_decides() => mine.clone(),
            (theirs, _) => theirs,
        };
        self.set_config(chosen);
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
            self.vars.capture(crate::values::Capture {
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
            .any(|t| t.kind == crate::design::TokenKind::Right);
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
        self.take_who(who);
    }

    /// The prompt is for `who` now. The capture compiles for it, and a
    /// design that follows the game is written again for it, since who
    /// you are decides whether Vosh can draw some settings, such as a
    /// color left open before `%u`.
    fn take_who(&mut self, who: Who) {
        if who == self.who {
            return;
        }
        self.who = who;
        let mut config = self.config.clone();
        if config.mirror_game(who) {
            self.set_config(config);
        } else {
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
    /// shows pinned. See `Stage::zone`.
    pub fn zone(&self) -> usize {
        self.stage
            .zone(self.draws(), &Template::parse(&self.config.template))
    }

    /// Record a candidate in the ring, on a send or a GA or EOR, with
    /// whether drawing is on and whether the profile has a capture. See
    /// `Stage::record`.
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
        self.take_who(Who::default());
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

#[cfg(test)]
mod tests;
