//! Following Char.Prompt, the game's own word on your prompt settings,
//! and handing the settings the game shows to your capture. An aabahran
//! capture that follows the game takes settings that differ from its
//! own, and a capture migrated from a trigger switches to Aabahran's
//! codes.

use chrono::{DateTime, FixedOffset};

use super::{stamp, GamePromptSeen, PromptEngine, SeenKind};
use crate::aabahran::{CompileError, Origin, Who};
use crate::capture;
use crate::config::{AabahranCapture, CaptureConfig, CaptureSource};
use crate::design::Template;
use crate::values;
use crate::values::gmcp::CharPrompt;

/// Why the migrated capture kept its pattern when the game showed your
/// PROMPT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Kept {
    /// A color code runs into a code in the settings the game sent, so
    /// they do not compile.
    Compile(CompileError),
    /// The pattern fills values under names of its own, in the order it
    /// reads them. No code fills them, so the design and any script that
    /// reads them would go blank.
    Unknown(Vec<String>),
}

impl PromptEngine {
    /// Follow a Char.Prompt. `enabled` says whether your text prompt is
    /// on. An aabahran capture that follows the game takes settings that
    /// differ from the ones it holds, with the source and the time, and a
    /// migrated capture switches to them, prompts off or on.
    pub(super) fn follow_char_prompt(
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
    pub(super) fn take_settings(
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
            .map(|name| values::feeds(name).to_string())
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
            let Some(entry) = values::entry_for(&field).filter(|e| !e.param) else {
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
    /// Aabahran's codes, which follow the game from then on, as James
    /// asked on 2026-09-30. The first PROMPT the game shows under the
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
            .filter(|name| !values::known(name))
            .collect()
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
}

/// The capture a migrated one becomes when the game shows your settings:
/// Aabahran's codes as the game stores them, following the game, learned
/// from `source` at `at`. An error when they do not compile for `who`.
pub(crate) fn codes_from_game(
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
