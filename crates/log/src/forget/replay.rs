//! Replays each session's login row by row to find the lines that answered
//! a password prompt.

use std::collections::HashMap;

use super::aabahran::{
    after_password_prompt, carries_password, is_account_menu, is_password_prompt, is_quit,
    AFTER_IMM_PASSWORD, ALREADY_PLAYING, ASK_AGAIN, IMM_PASSWORD_NEEDED, MAIN_MENU, MISMATCH,
    NULL_PASSWORD, RECONNECT_FAILED, SET_IMM, VERIFY_IMM,
};
use super::PasswordLines;
use crate::HIDDEN_SENT_TEXT;

/// What the login waits for next, as far as the log shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wait {
    /// Play, or anything the replay does not follow.
    Nothing,
    MainMenu,
    AccountName,
    NewAccountName,
    AccountMenu,
    LinkName,
    CharacterName,
    ConfirmName,
    ConnectAnyway,
    EnterName,
    /// The next `left` lines you send answer password prompts.
    Password {
        left: u8,
        then: After,
        refused: Refused,
    },
    /// You picked a character. An immortal one asks `Imm-Password?> `
    /// with nothing printed first.
    MaybeImmPassword,
}

/// Where the login goes after the last password of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum After {
    Nothing,
    MaybeImmPassword,
}

impl After {
    fn wait(self) -> Wait {
        match self {
            After::Nothing => Wait::Nothing,
            After::MaybeImmPassword => Wait::MaybeImmPassword,
        }
    }
}

/// Where the login goes when the game turns down the name that would
/// have led to a password prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Refused {
    Nothing,
    NewAccountName,
    EnterName,
}

impl Refused {
    fn wait(self) -> Wait {
        match self {
            Refused::Nothing => Wait::Nothing,
            Refused::NewAccountName => Wait::NewAccountName,
            Refused::EnterName => Wait::EnterName,
        }
    }
}

/// `left` password answers due, then play.
fn passwords(left: u8) -> Wait {
    Wait::Password {
        left,
        then: After::Nothing,
        refused: Refused::Nothing,
    }
}

/// The last password prompt your lines answered, which the game's reply
/// can take back by asking again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Answered {
    /// The wait that line answered.
    wait: Wait,
    /// Pieces that went out in the same write after it and found no
    /// password due. The game reads each piece as its own answer, so when
    /// it asks again, these answer the prompts it asks from there.
    ahead: u8,
}

/// A line sent right after a pick, blanked once the game shows it was
/// the answer to `Imm-Password?> `.
#[derive(Debug, Clone)]
struct Candidate {
    /// The line, and any pieces of it that went out in the same write.
    ids: Vec<i64>,
    /// The time of that write.
    ts: i64,
    /// No game output came since, so a line with the same time is a piece.
    open: bool,
    /// The game printed something else since.
    heard: bool,
}

/// One session's replay.
#[derive(Debug)]
struct Replay {
    wait: Wait,
    /// The nearest earlier row reads as a password prompt.
    after_prompt: bool,
    /// The password prompt your last lines answered, when they did.
    answered: Option<Answered>,
    candidate: Option<Candidate>,
    /// The time of the write your last line went out in, when that line
    /// was blanked and no game output came since.
    blank_write: Option<i64>,
}

impl Default for Replay {
    fn default() -> Self {
        Self {
            wait: Wait::Nothing,
            after_prompt: false,
            answered: None,
            candidate: None,
            blank_write: None,
        }
    }
}

impl Replay {
    /// A menu line: the login waits at that menu whatever came before.
    fn enter(&mut self, wait: Wait) {
        self.wait = wait;
        self.answered = None;
        self.candidate = None;
    }

    /// Take one line as the answer to a password prompt, when one is due.
    /// True when it was.
    fn answer_password(&mut self) -> bool {
        let Wait::Password { left, then, .. } = self.wait else {
            return false;
        };
        self.answered = Some(Answered {
            wait: self.wait,
            ahead: 0,
        });
        self.wait = if left > 1 {
            Wait::Password {
                left: left - 1,
                then,
                refused: Refused::Nothing,
            }
        } else {
            then.wait()
        };
        true
    }

    /// The game turned an answer down and waits at `wait` again. The
    /// pieces sent ahead of its reply answer the prompts from there.
    fn ask_again(&mut self, wait: Wait, ahead: u8) {
        self.wait = wait;
        self.answered = None;
        for _ in 0..ahead {
            if !self.answer_password() {
                if let Some(answered) = self.answered.as_mut() {
                    answered.ahead = answered.ahead.saturating_add(1);
                }
            }
        }
    }

    /// One row of game output. Returns the candidate lines it confirms.
    fn output(&mut self, text: &str) -> Vec<i64> {
        let row = text.trim_end();
        self.after_prompt = is_password_prompt(row);
        // A prompt row asks for what the replay already waits for, and a
        // prompt joined to the next line leaves that line as the reply.
        let line = after_password_prompt(row);
        self.blank_write = None;
        if let Some(candidate) = self.candidate.as_mut() {
            candidate.open = false;
        }
        if line == MAIN_MENU {
            self.enter(Wait::MainMenu);
            return Vec::new();
        }
        if is_account_menu(line) {
            self.enter(Wait::AccountMenu);
            return Vec::new();
        }
        // The line break in front of a prompt, or the prompt itself.
        if line.trim_start().is_empty() {
            return Vec::new();
        }

        let mut confirmed = Vec::new();
        if AFTER_IMM_PASSWORD.contains(&line) {
            if let Some(candidate) = self.candidate.take() {
                confirmed = candidate.ids;
            }
        } else if let Some(candidate) = self.candidate.as_mut() {
            candidate.heard = true;
        }

        if line == ALREADY_PLAYING {
            self.wait = Wait::ConnectAnyway;
            self.answered = None;
            return confirmed;
        }
        if line == RECONNECT_FAILED {
            self.wait = Wait::EnterName;
            self.answered = None;
            return confirmed;
        }
        if ASK_AGAIN.contains(&line) {
            // The same prompt again. Answers sent ahead of the game's
            // reply each earn one of these lines, so with no answer left
            // to take back, one more password is due.
            match (self.answered.take(), self.wait) {
                (Some(answered), _) => self.ask_again(answered.wait, answered.ahead),
                (
                    None,
                    Wait::Password {
                        left,
                        then,
                        refused,
                    },
                ) => {
                    self.wait = Wait::Password {
                        left: left.saturating_add(1),
                        then,
                        refused,
                    };
                }
                (None, _) => {}
            }
            return confirmed;
        }
        if let Some(answered) = self.answered.take() {
            let again = if MISMATCH.contains(&line) || line == SET_IMM {
                Some(passwords(2))
            } else if line == VERIFY_IMM {
                Some(passwords(1))
            } else {
                None
            };
            if let Some(wait) = again {
                self.ask_again(wait, answered.ahead);
                return confirmed;
            }
        }
        self.wait = match self.wait {
            Wait::MaybeImmPassword if line == IMM_PASSWORD_NEEDED => passwords(2),
            Wait::MaybeImmPassword => Wait::Nothing,
            Wait::Password { .. } if line == NULL_PASSWORD => self.wait,
            Wait::Password { refused, .. } => refused.wait(),
            Wait::ConfirmName => Wait::CharacterName,
            other => other,
        };
        confirmed
    }

    /// One line you sent. Returns its id when it holds a password.
    fn sent(&mut self, id: i64, ts_ms: i64, text: &str) -> Option<i64> {
        // A line that went out in the same write as a blanked one is a
        // piece of it. The input pipeline used to split a password at `;`.
        let same_write = self.blank_write == Some(ts_ms);
        let hidden = text == HIDDEN_SENT_TEXT;
        // The game skips leading spaces before it reads an answer.
        let answer = text.strip_prefix("> ").unwrap_or(text).trim_start();
        let first = answer.chars().next().map(|c| c.to_ascii_uppercase());
        let after_prompt = std::mem::take(&mut self.after_prompt);
        if let Some(candidate) = self.candidate.as_mut() {
            if candidate.open && candidate.ts == ts_ms {
                if !hidden {
                    candidate.ids.push(id);
                }
                return None;
            }
        }
        // A line after a pick that the game never showed to be the
        // immortal password was an ordinary command.
        self.candidate = None;

        if self.answer_password() {
            self.blank_write = Some(ts_ms);
            return (!hidden).then_some(id);
        }
        let blank = after_prompt || same_write || carries_password(answer);
        // A piece of the last answer's write answers whatever the game
        // asks once it turns that answer down.
        match self.answered.as_mut() {
            Some(answered) if same_write => answered.ahead = answered.ahead.saturating_add(1),
            _ => self.answered = None,
        }
        self.wait = match self.wait {
            // Taken above.
            Wait::Password { .. } => self.wait,
            Wait::MaybeImmPassword => {
                if !hidden {
                    self.candidate = Some(Candidate {
                        ids: vec![id],
                        ts: ts_ms,
                        open: true,
                        heard: false,
                    });
                }
                Wait::Nothing
            }
            Wait::MainMenu => match first {
                Some('E' | 'L') => Wait::AccountName,
                Some('C') => Wait::NewAccountName,
                _ => Wait::Nothing,
            },
            Wait::NewAccountName if is_quit(answer) => Wait::Nothing,
            Wait::NewAccountName => Wait::Password {
                left: 2,
                then: After::Nothing,
                refused: Refused::NewAccountName,
            },
            Wait::AccountMenu => match first {
                Some('P') => passwords(3),
                Some('L') => Wait::LinkName,
                Some('N') => Wait::CharacterName,
                Some('D') => Wait::Nothing,
                Some(c) if c.is_ascii_digit() => Wait::MaybeImmPassword,
                _ => Wait::AccountMenu,
            },
            Wait::LinkName | Wait::CharacterName | Wait::EnterName if is_quit(answer) => {
                Wait::Nothing
            }
            // An account name that is too short or long, or a character
            // that cannot be linked, prints an error and the menu before
            // any prompt, which ends the wait.
            Wait::AccountName | Wait::LinkName => passwords(1),
            Wait::CharacterName => Wait::ConfirmName,
            Wait::ConfirmName if first == Some('Y') => passwords(2),
            Wait::ConfirmName => Wait::CharacterName,
            Wait::ConnectAnyway if first == Some('Y') => Wait::Nothing,
            Wait::ConnectAnyway => Wait::EnterName,
            Wait::EnterName => Wait::Password {
                left: 1,
                then: After::MaybeImmPassword,
                refused: Refused::EnterName,
            },
            Wait::Nothing => Wait::Nothing,
        };
        self.blank_write = blank.then_some(ts_ms);
        (blank && !hidden).then_some(id)
    }
}

/// Replays session logs row by row to find the lines where you sent a
/// password. See the module docs for the rule.
#[derive(Debug, Default)]
pub(crate) struct PasswordFinder {
    sessions: HashMap<i64, Replay>,
    found: Vec<(i64, i64)>,
}

impl PasswordFinder {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Feed one log row. Rows arrive in id order, and sessions may
    /// interleave. `sent` marks a line you sent (a `> ` row with no raw
    /// bytes). `ts_ms` is the row's time. The lines of one write to the
    /// game share it.
    pub(crate) fn row(&mut self, id: i64, session_id: i64, ts_ms: i64, text: &str, sent: bool) {
        let replay = self.sessions.entry(session_id).or_default();
        if sent {
            if let Some(line) = replay.sent(id, ts_ms, text) {
                self.found.push((line, session_id));
            }
        } else {
            for line in replay.output(text) {
                self.found.push((line, session_id));
            }
        }
    }

    /// The lines found, oldest first. A session whose log ends right
    /// after the line that followed a pick counts that line, since the
    /// game said nothing to show it was a command.
    pub(crate) fn finish(mut self) -> PasswordLines {
        for (sid, replay) in &self.sessions {
            if let Some(candidate) = &replay.candidate {
                if !candidate.heard {
                    self.found
                        .extend(candidate.ids.iter().map(|id| (*id, *sid)));
                }
            }
        }
        self.found.sort_unstable();
        self.found.dedup();
        PasswordLines {
            lines: self.found,
            wipe_pending: false,
        }
    }
}
