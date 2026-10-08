//! The writer, the session side of the writing card. It follows where
//! the game takes your input, its prompt, a line editor or its pager,
//! and runs the one job the card asks of the game at a time (Description
//! Editor Q5, Note Editor Q13).
//!
//! It also notices when a line you typed opened the game's editor on a
//! text it can name, and offers the card while nothing else went out
//! after that line (Description Editor Q3, Note Editor Q3). Until the
//! game's prompt returns, the page sends what you type there raw and
//! counts it against the text's width.
//!
//! While a job runs, every other send of the session waits: what
//! triggers, timers, Lua and `#walk` send stays in the stream's hold and
//! goes once the job ends. A line you type goes at once, as `./` and
//! your line while the editor is open, which the editor runs as a game
//! command (`olc.c:3617`), and as typed at the game's prompt (Q6).
//!
//! The writer only decides. Each event hands back the lines to send and
//! the session does the IO, as the walker does.
//!
//! - `kinds` holds the texts the card takes and how each one goes.
//! - `openers` reads which text a line you typed opens.
//! - `game_text` holds what the game prints around its editor and what
//!   it does to a line.
//! - `payloads` holds what the page asks of a job and hears of it.
//! - `plan` holds what a job sends, its stages and the mends.
//! - `job` runs one job.

pub(crate) mod game_text;
pub(crate) mod job;
pub(crate) mod kinds;
mod openers;
pub(crate) mod payloads;
mod plan;

use serde::Serialize;
use tokio::time::Instant;

use game_text::{editor_waits, lone_prompts, opened_listing, pager_waits, GameLine, BANNER};
use job::Job;
use kinds::Kind;
use payloads::{JobProgress, JobResult, WriteJob};

pub(crate) use payloads::Action;

/// Where the game takes what you send.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Game {
    /// Vosh has not seen the game's prompt since the link opened.
    #[default]
    Unknown,
    /// The game's own prompt, which the GMCP prompt tick comes with.
    Prompt,
    /// A line editor, which waits behind `> `.
    Editor,
    /// The pager, which waits for Return.
    Pager,
}

/// The card's offer, after you opened the game's editor yourself on a
/// text it can name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Offer {
    pub(crate) id: u64,
    pub(crate) kind: Kind,
}

/// How the last job ended, with the page's number for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Done {
    pub(crate) id: u64,
    pub(crate) result: JobResult,
}

/// What the page hears on [`crate::app::events::WRITING`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub(crate) struct WritingState {
    pub(crate) game: Game,
    /// The text the game's editor holds, while Vosh can name it.
    pub(crate) editor: Option<Kind>,
    pub(crate) offer: Option<Offer>,
    pub(crate) job: Option<JobProgress>,
    /// How many lines of the session's other sends wait for the job.
    pub(crate) held: usize,
    pub(crate) done: Option<Done>,
}

/// What the page asks the writer.
#[derive(Debug)]
pub(crate) enum WriterCommand {
    Start(WriteJob),
    Stop,
    /// Open the card on the offer `id`.
    Take {
        id: u64,
    },
}

/// A line you typed that opens the game's editor on a text Vosh names.
#[derive(Debug)]
struct Opener {
    kind: Kind,
    /// The lines the session had sent once it went.
    out: u64,
    /// The game printed the line that names the text, for a kind that
    /// has one.
    named: bool,
    beast: Option<String>,
}

/// The game's editor, open on a text Vosh names.
#[derive(Debug)]
struct Open {
    kind: Kind,
    out: u64,
    lines: Vec<GameLine>,
    /// The listing ended with the editor's `> `, or its pager waits.
    listed: bool,
    pager: bool,
    beast: Option<String>,
    offer: Option<u64>,
}

/// The writer. See the module notes.
#[derive(Debug, Default)]
pub(crate) struct Writer {
    game: Game,
    opener: Option<Opener>,
    open: Option<Open>,
    job: Option<Job>,
    /// A job that waits for the game's prompt.
    waiting: Option<WriteJob>,
    done: Option<Done>,
    next_offer: u64,
}

impl Writer {
    /// True while the writer reads the game's lines, so the session hands
    /// them over.
    pub(crate) fn watching(&self) -> bool {
        self.job.is_some() || self.opener.is_some() || self.open.as_ref().is_some_and(|o| !o.listed)
    }

    /// True while a job holds the session's other sends.
    pub(crate) fn holds(&self) -> bool {
        self.job.as_ref().is_some_and(Job::holds)
    }

    /// When the job next needs a look.
    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.job.as_ref().and_then(Job::deadline)
    }

    pub(crate) fn state(&self, held: usize) -> WritingState {
        WritingState {
            game: self.game,
            editor: self.open.as_ref().map(|o| o.kind),
            offer: self
                .open
                .as_ref()
                .and_then(|o| o.offer.map(|id| Offer { id, kind: o.kind })),
            job: self.job.as_ref().map(Job::progress),
            held,
            done: self.done.clone(),
        }
    }

    /// `bytes`, lines you typed, as they go to the game, and note what
    /// they open. `out` is the count of lines the session sent once they
    /// go. While a job holds the editor open, each line goes as `./` and
    /// the line. Anything you send takes back the offer.
    pub(crate) fn typed(&mut self, bytes: &[u8], out: u64) -> Vec<u8> {
        if let Some(open) = &mut self.open {
            open.offer = None;
        }
        let text = String::from_utf8_lossy(bytes);
        let lines: Vec<&str> = text
            .split_terminator('\n')
            .map(|l| l.trim_end_matches('\r'))
            .collect();
        self.opener = lines
            .last()
            .and_then(|line| openers::opens(line))
            .map(|kind| Opener {
                kind,
                out,
                named: kind.names_itself().is_none(),
                beast: None,
            });
        if !self.job.as_ref().is_some_and(Job::in_editor) {
            return bytes.to_vec();
        }
        let mut wired = Vec::with_capacity(bytes.len() + 3 * lines.len());
        for line in lines {
            wired.extend_from_slice(b"./ ");
            wired.extend_from_slice(line.as_bytes());
            wired.extend_from_slice(b"\r\n");
        }
        wired
    }

    /// A line the game sent. `out` is the count of lines the session has
    /// sent.
    pub(crate) fn line(&mut self, line: &GameLine, out: u64) {
        if let Some(job) = &mut self.job {
            job.line(line);
        }
        if let Some(opener) = &mut self.opener {
            if let Some(names) = opener.kind.names_itself() {
                if let Some(rest) = line.plain.strip_prefix(names) {
                    opener.named = true;
                    opener.beast = rest
                        .strip_suffix('.')
                        .map(str::to_string)
                        .filter(|b| !b.is_empty());
                }
            }
        }
        if line.plain == BANNER[0] {
            self.game = Game::Editor;
            // An editor the card opens itself is no offer.
            let opener = self.opener.take().filter(|o| o.named && self.job.is_none());
            self.open = opener.map(|o| Open {
                kind: o.kind,
                out: o.out,
                lines: Vec::new(),
                listed: false,
                pager: false,
                beast: o.beast,
                offer: None,
            });
        }
        if let Some(open) = &mut self.open {
            if !open.listed {
                open.lines.push(line.clone());
            }
            if out != open.out {
                open.offer = None;
            }
        }
    }

    /// The end of a read: the text it brought, without colors, and the
    /// partial it ended on. Returns the lines to send.
    pub(crate) fn read_end(
        &mut self,
        text: &str,
        partial: &str,
        out: u64,
        now: Instant,
    ) -> Vec<String> {
        if editor_waits(partial) {
            self.game = Game::Editor;
        } else if pager_waits(partial) {
            self.game = Game::Pager;
        }
        if let Some(open) = &mut self.open {
            let waits = editor_waits(partial);
            let paged = pager_waits(partial);
            if !open.listed && (waits || paged) {
                open.listed = true;
                open.pager = paged;
                if out == open.out && self.job.is_none() {
                    self.next_offer += 1;
                    open.offer = Some(self.next_offer);
                }
            } else if waits {
                open.pager = false;
            }
        }
        let mut send = Vec::new();
        if let Some(job) = &mut self.job {
            job.read_end(lone_prompts(text), partial, now, &mut send);
        }
        send
    }

    /// The game's prompt tick. The editor and the pager closed.
    pub(crate) fn tick(&mut self, now: Instant) -> Vec<String> {
        self.game = Game::Prompt;
        self.opener = None;
        self.open = None;
        let mut send = Vec::new();
        if let Some(job) = &mut self.job {
            job.tick(now, &mut send);
        } else if let Some(spec) = self.waiting.take() {
            self.begin(spec, now, &mut send);
        }
        send
    }

    /// A look at the time.
    pub(crate) fn poll(&mut self, now: Instant) -> Vec<String> {
        let mut send = Vec::new();
        if let Some(job) = &mut self.job {
            job.poll(now, &mut send);
        }
        send
    }

    /// Run what the page asked. `out` is the count of lines the session
    /// has sent.
    pub(crate) fn command(
        &mut self,
        command: WriterCommand,
        out: u64,
        now: Instant,
    ) -> Vec<String> {
        let mut send = Vec::new();
        match command {
            WriterCommand::Start(spec) => {
                if self.job.is_some() || self.waiting.is_some() {
                    return send;
                }
                if spec.action == Action::Paste {
                    if self.game == Game::Editor {
                        self.begin(spec, now, &mut send);
                    }
                    return send;
                }
                match self.game {
                    Game::Prompt => self.begin(spec, now, &mut send),
                    Game::Editor => self.finish(spec.id, JobResult::Busy),
                    // The pager takes an empty line as Return, and the
                    // job starts at the prompt after it.
                    Game::Pager => {
                        send.push(String::new());
                        self.waiting = Some(spec);
                    }
                    Game::Unknown => self.waiting = Some(spec),
                }
            }
            WriterCommand::Stop => {
                if let Some(job) = &mut self.job {
                    job.stop(now, &mut send);
                } else if let Some(spec) = self.waiting.take() {
                    self.finish(spec.id, JobResult::Stopped { sent: 0 });
                }
            }
            WriterCommand::Take { id } => {
                let ours = self
                    .open
                    .as_ref()
                    .is_some_and(|o| o.offer == Some(id) && o.out == out);
                let open = if ours { self.open.take() } else { None };
                match open {
                    Some(open) if self.job.is_none() => {
                        let listed = opened_listing(&open.lines);
                        let mut job = Job::take(id, open.kind, listed, open.beast, open.pager);
                        job.start(now, &mut send);
                        self.job = Some(job);
                    }
                    _ => self.finish(id, JobResult::OfferGone),
                }
            }
        }
        send
    }

    /// The link dropped.
    pub(crate) fn dropped(&mut self) {
        if let Some(job) = &mut self.job {
            job.dropped();
        }
        if let Some(spec) = self.waiting.take() {
            self.finish(
                spec.id,
                JobResult::Dropped {
                    sent: 0,
                    posted: false,
                },
            );
        }
        self.game = Game::Unknown;
        self.opener = None;
        self.open = None;
    }

    /// Take a job that ended out of the way, keeping how it ended.
    pub(crate) fn settle(&mut self) {
        let Some(job) = &self.job else {
            return;
        };
        let Some(result) = job.result().cloned() else {
            return;
        };
        let id = job.id();
        self.job = None;
        self.finish(id, result);
    }

    fn begin(&mut self, spec: WriteJob, now: Instant, send: &mut Vec<String>) {
        let mut job = Job::new(spec);
        job.start(now, send);
        self.job = Some(job);
    }

    fn finish(&mut self, id: u64, result: JobResult) {
        self.done = Some(Done { id, result });
    }
}

#[cfg(test)]
mod tests;
