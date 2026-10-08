//! Reading the game's replies to `prompt` and `fprompt` while no
//! Char.Prompt came, and what the game said of your prompt settings for
//! `session://game-prompt-seen`.

use chrono::{DateTime, FixedOffset};
use serde::Serialize;

use super::PromptEngine;
use crate::aabahran::observer::{self, ReplyKind};
use crate::config::CaptureSource;

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
    /// so once, and the card and Settings ring those parts. Left
    /// out of the event when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub lost: Vec<String>,
}

/// How long after one of your own sends the game's reply to `prompt` or
/// `fprompt` counts, in milliseconds.
pub(crate) const OBSERVE_MS: i64 = observer::WINDOW_MS;

/// The settings the game showed after one of your own sends this
/// session, without Char.Prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSetting {
    pub prompt: Option<String>,
    pub fprompt: Option<String>,
    /// When the last of them came.
    pub at: DateTime<FixedOffset>,
}

/// What the observer keeps between lines.
#[derive(Debug, Clone, Default)]
pub(super) struct Observer {
    /// When you last sent a line, in milliseconds since the epoch.
    pub(super) sent_at: Option<i64>,
    /// Your last send was `prompt off`.
    pub(super) sent_off: bool,
    /// "You will no longer see prompts." came since your last send or
    /// the last pulse.
    pub(super) off_line: bool,
    pub(super) seen: Option<SessionSetting>,
}

impl PromptEngine {
    /// The observer reads lines now: the Forsaken Lands rules hold, no
    /// Char.Prompt came this session, and you sent a line within
    /// `OBSERVE_MS`.
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

    /// What the game said of your prompt settings since the last call,
    /// oldest first, for `session://game-prompt-seen`.
    pub fn take_seen(&mut self) -> Vec<GamePromptSeen> {
        std::mem::take(&mut self.seen)
    }
}

/// A prompt setting the game showed that holds a control character.
/// No PROMPT you type can, so it is the buffer an older build's
/// `prompt off` left unfilled.
fn leftover_buffer(setting: &str) -> bool {
    setting.chars().any(char::is_control)
}
