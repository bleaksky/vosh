//! A fake Aabahran that plays one connection.
//!
//! [`Mud`] answers what a client sends with the bytes the game would
//! write, in the game's wire order. The game writes each GMCP packet
//! straight to the socket and holds text in the output buffer until the
//! pulse ends, so every packet of a pulse comes first: the ones a command
//! sent (Char.Prompt from `prompt`, Room.Info from `look`, Char.Affects
//! when a song lands), then the prompt time packages. The text follows,
//! with the game's `\n\r` line ends: the reply, the battle line in a
//! fight, a blank line unless you play compact, the prompt that
//! [`game::prompt`] prints for your PROMPT, and IAC GA, or IAC EOR once
//! the client asked for it from a game that plays [`Options::eor`].
//! [`Options::order`] moves the packets after the reply and before the
//! prompt, as a game that writes GMCP into its output buffer sends them.
//!
//! Three server builds are played, as [`Build`] names them. Output that
//! comes without a command, such as someone arriving, starts on a new line
//! as `write_to_buffer` starts it.
//!
//! What it answers, besides the game's own `prompt`, `fprompt`, `look`,
//! `afk`, `compact`, `telnetga` and `quit`:
//!
//! - `quit` takes your affects off one at a time, each removal sending
//!   Char.Affects, then closes the link. `quit menu`, `quit switch` and
//!   `quit character` do the same and go back to the account menu, where
//!   the next line plays you again with the affects your pfile kept.
//! - `fight` starts or ends a fight in which a Blackwatch guard hits you,
//!   so you tank.
//! - `lament` puts lamented tears on you or takes it off.
//! - `blind` makes you blind or lets you see again, so the game withholds
//!   your opponent's health.
//! - `split` cuts the next prompt into two writes [`SPLIT_MS`] apart.
//! - `cast N name` puts an affect on you for N hours, or recasts it, and
//!   `tick` runs one hour of `affect_update`: each timed affect loses an
//!   hour and one at 0 wears off. Each sends Char.Affects at once.
//! - `spam N` sends N lines and a prompt, and `pulses N` sends N pulses
//!   [`PULSE_MS`] apart, as combat rounds come.
//! - `bash Tolliver`, `bash Maren` or `bash Orla` slams into them as
//!   `do_bash` prints it and lags you [`Options::bash_ms`], without the
//!   fight and the damage a real bash brings. The game holds each line
//!   you send while you are lagged and runs the first once the lag ends,
//!   then one each [`GAME_PULSE_MS`], as comm.c reads its buffer. The
//!   fake hands that back as [`Write::after_ms`].
//!
//! Anything else gets `Huh?` and a prompt.

use std::fmt::Write as _;

use super::game::{self, State, Tank};
use super::gmcp;

/// The telnet bytes the fake reads and writes.
pub mod telnet {
    pub const IAC: u8 = 255;
    pub const DONT: u8 = 254;
    pub const DO: u8 = 253;
    pub const WONT: u8 = 252;
    pub const WILL: u8 = 251;
    pub const SB: u8 = 250;
    pub const GA: u8 = 249;
    pub const SE: u8 = 240;
    pub const EOR: u8 = 239;
    pub const GMCP: u8 = 201;
    /// The option a client asks for with IAC DO, which the game marks
    /// prompts with once it agreed.
    pub const TELOPT_EOR: u8 = 25;
}

use telnet::{DO, EOR, GA, GMCP, IAC, SB, SE, TELOPT_EOR, WILL};

/// James's PROMPT, which the fake starts with, as `do_prompt` stores
/// it: one line out of a fight, and a tank line above it while someone
/// in your group tanks.
pub const PROMPT: &str = "%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c";

/// `prompt all`, as `do_prompt` stores it.
pub const PROMPT_ALL: &str = "%n%P%C<%hhp %mm %vmv> ";

/// Your opponent in a fight.
pub const OPPONENT: &str = "a Blackwatch guard";

/// Your opponent's health in a fight, in percent.
pub const OPPONENT_PCT: i64 = 54;

/// Your health while the guard hits you.
pub const FIGHT_HIT: i64 = 765;

/// What the older builds store for `prompt off`. They fall through to
/// `Prompt set to` with a buffer `do_prompt` never filled.
pub const UNFILLED: &str = "\u{1}\u{2}";

/// How long the second half of a split prompt waits, in milliseconds.
pub const SPLIT_MS: u64 = 400;

/// How far apart `pulses N` sends its pulses, in milliseconds.
pub const PULSE_MS: u64 = 50;

/// Aabahran's pulse, in milliseconds, `1000 / PULSE_PER_SECOND`.
pub const GAME_PULSE_MS: u64 = 250;

/// How long a bash that lands lags you, its 24 beats (const.c:1881).
pub const BASH_MS: u64 = 24 * GAME_PULSE_MS;

/// The longest setting `do_prompt` keeps.
pub const KEEP: usize = 255;

/// Which server build the fake plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Build {
    /// 54f14ef6 and later. Char.Prompt at login and on every change,
    /// Char.State and Room.Weather each pulse, the `tank` object, the
    /// `hidden` flag on every packet lamented tears empties, the prompt
    /// time packages with the text prompt off or while you are away, and
    /// a `prompt off` that returns.
    New,
    /// 243cac5c. Lamented tears empties and zeroes the packets with no
    /// flag, and drops the battle line.
    Unflagged,
    /// Before 243cac5c. Lamented tears leaves every packet true, and only
    /// Char.Affects names the song.
    Older,
}

impl Build {
    /// A build by the name [`Build::name`] gives it.
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "new" | "54f14ef6" => Some(Self::New),
            "243cac5c" | "unflagged" => Some(Self::Unflagged),
            "older" | "old" => Some(Self::Older),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Unflagged => "243cac5c",
            Self::Older => "older",
        }
    }
}

/// Where the prompt time packages come beside the text of their pulse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TickOrder {
    /// Before the text, as Aabahran sends them: `gmcp_send` writes
    /// straight to the socket (gmcp.c:21) and the text waits in the output
    /// buffer until the pulse ends (comm.c:1629).
    #[default]
    First,
    /// After the reply and before the prompt, as a game that writes GMCP
    /// into its output buffer as it prints the prompt sends them. The
    /// packets a command wrote come with them.
    Middle,
}

/// How a connection starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub build: Build,
    /// Your character's name.
    pub name: String,
    /// Your PROMPT and fight prompt as the game stores them.
    pub prompt: String,
    pub fprompt: String,
    /// The connection takes over a link dead character, so the game
    /// sends no Char.Status and no Char.Prompt.
    pub reconnect: bool,
    /// The characters your account lists. With any, the game shows the
    /// account menu first and you pick one by its number, as
    /// `chargen_acct_menu` (comm.c) reads it, and you play the one you
    /// picked.
    pub account: Vec<String>,
    /// `telnetga` is on, so each prompt ends in IAC GA.
    pub ga: bool,
    /// The game plays server proposal S3 with no state kept: it answers
    /// each IAC DO EOR it reads with IAC WILL EOR, and once it has, ends
    /// each prompt with IAC EOR in place of GA. A client that answered
    /// each WILL EOR with DO EOR would go back and forth with it forever.
    pub eor: bool,
    /// `compact` is on, so no blank line comes before a prompt.
    pub compact: bool,
    /// Your wizi and incog levels, 0 for none. Either one makes you an
    /// immortal of level [`IMMORTAL_LEVEL`], and above 1 the game prints
    /// `(Wizi N) ` and `(Incog N) ` before each prompt.
    pub wizi: i64,
    pub incog: i64,
    /// The affects on you when you log in, as your pfile holds them.
    pub affects: Vec<Affect>,
    /// How long `bash` lags you, in milliseconds.
    pub bash_ms: u64,
    /// Where the prompt time packages come.
    pub order: TickOrder,
}

/// An affect on you, one Char.Affects row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Affect {
    pub kind: String,
    pub name: String,
    /// Hours left, -1 permanent.
    pub duration: i64,
    pub level: i64,
    pub location: String,
    pub modifier: i64,
}

impl Affect {
    /// A spell of level 50 that modifies nothing.
    pub fn spell(name: &str, duration: i64) -> Self {
        Self {
            kind: "spell".into(),
            name: name.into(),
            duration,
            level: MORTAL_LEVEL,
            location: "none".into(),
            modifier: 0,
        }
    }

    /// The row as `gmcp_send_affects` writes it.
    fn json(&self) -> String {
        format!(
            r#"{{"kind":{},"name":{},"duration":{},"level":{},"location":{},"modifier":{}}}"#,
            quote(&self.kind),
            quote(&self.name),
            self.duration,
            self.level,
            quote(&self.location),
            self.modifier
        )
    }
}

/// What Tester logs in with: bless for 6 hours and armor for 44.
pub fn default_affects() -> Vec<Affect> {
    vec![
        Affect {
            location: "hitroll".into(),
            modifier: 4,
            ..Affect::spell("bless", 6)
        },
        Affect {
            location: "ac".into(),
            modifier: -20,
            ..Affect::spell("armor", 44)
        },
    ]
}

impl Options {
    /// Tester logging in fresh with [`PROMPT`], GA on and compact off.
    pub fn new(build: Build) -> Self {
        Self {
            build,
            name: "Tester".into(),
            prompt: PROMPT.into(),
            fprompt: String::new(),
            reconnect: false,
            account: Vec::new(),
            ga: true,
            eor: false,
            compact: false,
            wizi: 0,
            incog: 0,
            affects: default_affects(),
            bash_ms: BASH_MS,
            order: TickOrder::First,
        }
    }
}

/// The level Char.Status names for a mortal.
pub const MORTAL_LEVEL: i64 = 50;

/// The level Char.Status names for an immortal with wizi or incog.
pub const IMMORTAL_LEVEL: i64 = 60;

/// Bytes to write, after a wait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Write {
    /// How long to wait before writing, in milliseconds.
    pub after_ms: u64,
    pub bytes: Vec<u8>,
    /// The game closes the connection after these bytes.
    pub close: bool,
}

impl Write {
    fn now(bytes: Vec<u8>) -> Self {
        Self {
            after_ms: 0,
            bytes,
            close: false,
        }
    }
}

/// One pulse of output, and where its prompt starts.
struct Pulse {
    bytes: Vec<u8>,
    prompt_at: Option<usize>,
}

/// One connection to the fake game.
#[derive(Debug, Clone)]
pub struct Mud {
    build: Build,
    name: String,
    reconnect: bool,
    /// The characters the account menu lists, see [`Options::account`].
    account: Vec<String>,
    /// You picked a character at the account menu.
    picked: bool,
    /// What the game holds for you when it prints the prompt.
    pub state: State,
    /// Your PROMPT and fight prompt as the game stores them.
    pub prompt: String,
    pub fprompt: String,
    /// `COMM_PROMPT`: the game prints your prompt.
    pub prompt_on: bool,
    pub ga: bool,
    /// The game answers each IAC DO EOR, see [`Options::eor`].
    eor: bool,
    /// The game answered IAC DO EOR, so each prompt ends in IAC EOR.
    pub eor_on: bool,
    pub compact: bool,
    /// You cannot see, so the game withholds your opponent's health.
    pub blind: bool,
    /// The affects on you, in the order Char.Affects lists them.
    pub affects: Vec<Affect>,
    /// The client answered IAC WILL GMCP.
    gmcp: bool,
    logged_in: bool,
    split_next: bool,
    bash_ms: u64,
    /// Where the prompt time packages come, see [`Options::order`].
    order: TickOrder,
    /// The lag a skill put on you, which holds your next line.
    wait_ms: Option<u64>,
    /// Client bytes not read yet, and the line they build.
    input: Vec<u8>,
    line: Vec<u8>,
}

impl Mud {
    pub fn new(options: Options) -> Self {
        let state = State {
            exits: "[Exits: S]".into(),
            sky: "indoors".into(),
            temp: 68,
            region: "Temperate".into(),
            lang: "common".into(),
            fallback_hides: options.build == Build::New,
            invis: options.wizi,
            incog: options.incog,
            immortal: options.wizi > 0 || options.incog > 0,
            ..State::default()
        };
        Self {
            build: options.build,
            name: options.name,
            reconnect: options.reconnect,
            account: options.account,
            picked: false,
            state,
            prompt: options.prompt,
            fprompt: options.fprompt,
            prompt_on: true,
            ga: options.ga,
            eor: options.eor,
            eor_on: false,
            compact: options.compact,
            blind: false,
            affects: options.affects,
            gmcp: false,
            logged_in: false,
            split_next: false,
            bash_ms: options.bash_ms,
            order: options.order,
            wait_ms: None,
            input: Vec::new(),
            line: Vec::new(),
        }
    }

    /// A connection already logged in, with GMCP on, for a test that
    /// starts from a pulse.
    pub fn playing(options: Options) -> Self {
        let mut mud = Self::new(options);
        mud.gmcp = true;
        mud.logged_in = true;
        mud
    }

    pub fn build(&self) -> Build {
        self.build
    }

    /// What the game writes when you connect: IAC WILL GMCP and a line
    /// that names the build.
    pub fn greeting(&self) -> Vec<u8> {
        let mut out = vec![IAC, WILL, GMCP];
        out.extend_from_slice(
            format!(
                "The fake Aabahran, {} build. Press Enter to log in if your client has no GMCP.\n\r",
                self.build.name()
            )
            .as_bytes(),
        );
        out
    }

    /// Read what the client sent. IAC DO GMCP logs you in with GMCP on,
    /// and without GMCP the first line logs you in. After that each line
    /// is a command. A line that comes while a skill lags you waits for
    /// the lag to end, and the lines after it a pulse each. Returns what
    /// to write, in order.
    pub fn receive(&mut self, bytes: &[u8]) -> Vec<Write> {
        let mut writes = Vec::new();
        let mut held = false;
        self.input.extend_from_slice(bytes);
        let mut i = 0;
        while i < self.input.len() {
            let byte = self.input[i];
            if byte == IAC {
                let Some(&next) = self.input.get(i + 1) else {
                    break;
                };
                match next {
                    SB => {
                        let Some(end) = self.input[i..].windows(2).position(|w| w == [IAC, SE])
                        else {
                            break;
                        };
                        i += end + 2;
                    }
                    WILL..=telnet::DONT => {
                        let Some(&option) = self.input.get(i + 2) else {
                            break;
                        };
                        if next == DO && option == GMCP && !self.gmcp {
                            self.gmcp = true;
                            if !self.logged_in {
                                writes.push(Write::now(self.login()));
                            }
                        }
                        if next == DO && option == TELOPT_EOR && self.eor {
                            self.eor_on = true;
                            writes.push(Write::now(vec![IAC, WILL, TELOPT_EOR]));
                        }
                        i += 3;
                    }
                    IAC => {
                        self.line.push(IAC);
                        i += 2;
                    }
                    _ => i += 2,
                }
                continue;
            }
            i += 1;
            if byte != b'\n' {
                self.line.push(byte);
                continue;
            }
            let line = String::from_utf8_lossy(&self.line)
                .trim_end_matches('\r')
                .to_string();
            self.line.clear();
            if self.logged_in {
                let wait = match self.wait_ms.take() {
                    Some(ms) => {
                        held = true;
                        ms
                    }
                    None if held => GAME_PULSE_MS,
                    None => 0,
                };
                let mut answer = self.command(&line);
                if let Some(first) = answer.first_mut() {
                    first.after_ms += wait;
                }
                writes.extend(answer);
            } else if self.at_menu() {
                writes.push(Write::now(self.choose(&line)));
            } else {
                writes.push(Write::now(self.login()));
            }
        }
        self.input.drain(..i);
        writes
    }

    /// You log in: `connect_char` sends Char.Status, Char.Prompt on the
    /// new build, Char.Affects, Char.Worth and the time packages, then
    /// `do_look` sends Room.Info, and the first pulse follows. A reconnect
    /// sends none of those packets.
    pub fn login(&mut self) -> Vec<u8> {
        if self.at_menu() {
            return self.account_menu();
        }
        self.logged_in = true;
        if self.reconnect {
            return self
                .pulse(
                    Vec::new(),
                    "Reconnecting. Type replay to see missed tells.\n\r",
                    false,
                )
                .bytes;
        }
        let mut early = Vec::new();
        if self.gmcp {
            early.extend(self.status());
            if self.build == Build::New {
                early.extend(self.char_prompt());
            }
            early.extend(self.affects());
            early.extend(self.worth());
            early.extend(world_time());
            early.extend(world_moons());
            early.extend(room_info());
        }
        let welcome = format!(
            "Welcome to the fake Aabahran, {}.\n\r{}",
            self.name, ROOM_TEXT
        );
        self.pulse(early, &welcome, false).bytes
    }

    /// The game waits for your pick at the account menu.
    fn at_menu(&self) -> bool {
        !self.account.is_empty() && !self.picked
    }

    /// The account menu as `show_acct_menu` and `acct_menu_row` (comm.c)
    /// print it with 256 colours off, then the prompt of `acct-menu`
    /// (tables.c).
    fn account_menu(&self) -> Vec<u8> {
        use std::fmt::Write as _;
        const RST: &str = "\x1B[0m";
        let mut out = format!(
            "\n\r \x1B[37mAccount:{RST} \x1B[1;37mwanderer{RST}\n\r \x1B[1;30m{}{RST}\n\r",
            "\u{2500}".repeat(50)
        );
        let (race, class, seen) = ("Human", "Warrior", "2026-10-07");
        for (i, name) in self.account.iter().enumerate() {
            let number = i + 1;
            let _ = write!(
                out,
                "  \x1B[1;37m{number:2}{RST}. \x1B[37m{name:<15}{RST} \x1B[36mLv{MORTAL_LEVEL:<3}{RST} \x1B[1;30m{race:<8} {class:<12}{RST}  \x1B[1;30m{seen}{RST}\n\r",
            );
        }
        out.push_str(
            "\n\r [#] Play   [N]ew character   [L]ink character   [P]assword   [D]isconnect\n\r",
        );
        out.push_str("\n\rYour choice> ");
        out.into_bytes()
    }

    /// Your answer at the account menu: a number in the list logs you in
    /// as that character, and anything else shows the menu again.
    fn choose(&mut self, line: &str) -> Vec<u8> {
        let pick = line.trim().parse::<usize>().ok();
        match pick
            .and_then(|n| n.checked_sub(1))
            .and_then(|i| self.account.get(i))
        {
            Some(name) => {
                self.name = name.clone();
                self.picked = true;
                self.login()
            }
            None => {
                let mut out = b"Invalid selection.\n\r".to_vec();
                out.extend(self.account_menu());
                out
            }
        }
    }

    /// Run one line you typed. Returns what to write, in order.
    pub fn command(&mut self, line: &str) -> Vec<Write> {
        let trimmed = line.trim_start();
        let (word, rest) = trimmed
            .split_once(char::is_whitespace)
            .map_or((trimmed, ""), |(w, r)| (w, r.trim_start()));
        let lower = word.to_ascii_lowercase();
        let pulse = match lower.as_str() {
            "" => self.pulse(Vec::new(), "", false),
            w if w.len() >= 3 && "prompt".starts_with(w) => self.do_prompt(rest),
            w if w.len() >= 2 && "fprompt".starts_with(w) => self.do_fprompt(rest),
            "l" | "lo" | "loo" | "look" => self.look(),
            "fight" | "kill" => self.fight(),
            "lament" => self.lament(),
            "cast" => self.cast(rest),
            "tick" => self.tick(),
            "blind" => self.blind(),
            "bash" => self.bash(rest),
            "afk" => {
                self.state.afk = !self.state.afk;
                let reply = if self.state.afk {
                    "You are now in AFK mode.\n\r"
                } else {
                    "AFK mode removed. Type 'replay' to see tells.\n\r"
                };
                self.pulse(Vec::new(), reply, false)
            }
            "compact" => {
                self.compact = !self.compact;
                let reply = if self.compact {
                    "Compact mode set.\n\r"
                } else {
                    "Compact mode removed.\n\r"
                };
                self.pulse(Vec::new(), reply, false)
            }
            "telnetga" => {
                self.ga = !self.ga;
                let reply = if self.ga {
                    "Telnet GA enabled.\n\r"
                } else {
                    "Telnet GA removed.\n\r"
                };
                self.pulse(Vec::new(), reply, false)
            }
            "split" => {
                let pulse = self.pulse(
                    Vec::new(),
                    "The next prompt comes in two writes.\n\r",
                    false,
                );
                self.split_next = true;
                return vec![Write::now(pulse.bytes)];
            }
            "spam" => {
                let count = count(rest, 1000);
                let mut lines = String::new();
                for i in 1..=count {
                    let _ = write!(lines, "Line {i} of {count} of the spam.\n\r");
                }
                self.pulse(Vec::new(), &lines, false)
            }
            "pulses" => {
                let count = count(rest, 20);
                return (1..=count)
                    .map(|i| Write {
                        after_ms: PULSE_MS,
                        bytes: self.pulse_later(&format!("Pulse {i} of {count}.")),
                        close: false,
                    })
                    .collect();
            }
            "quit" => return self.quit(rest),
            _ => self.pulse(Vec::new(), "Huh?\n\r", false),
        };
        self.deliver(pulse)
    }

    /// Output that comes without a command, such as a combat round or
    /// someone arriving: `reply` on a new line, then the prompt.
    pub fn pulse_later(&mut self, reply: &str) -> Vec<u8> {
        self.pulse(Vec::new(), &format!("{reply}\n\r"), true).bytes
    }

    /// The combat round that ends your fight, such as the one your
    /// opponent dies in: `reply` on a new line, then the prompt.
    /// `stop_fighting` (fight.c:10278) writes Char.Combat `{}` straight to
    /// the socket in the middle of the round, so it comes before the
    /// prompt time packages, and the round's text waits in the output
    /// buffer until the pulse ends.
    pub fn fight_ends_later(&mut self, reply: &str) -> Vec<u8> {
        self.stop_fighting();
        let early = self.combat();
        self.pulse(early, &format!("{reply}\n\r"), true).bytes
    }

    /// The round that starts a fight someone else begins, such as an
    /// aggressive guard: the guard attacks, `reply` follows on a new line,
    /// then the prompt with the fight's first Char.Combat.
    pub fn fight_starts_later(&mut self, reply: &str) -> Vec<u8> {
        self.state.fighting = true;
        self.state.hit = FIGHT_HIT;
        self.state.position = 8;
        self.state.tank = Some(Tank {
            name: self.name.clone(),
            hit: FIGHT_HIT,
            max_hit: self.state.max_hit,
        });
        self.pulse(Vec::new(), &format!("{reply}\n\r"), true).bytes
    }

    /// Hand over a pulse, cut in two when `split` asked for it.
    fn deliver(&mut self, pulse: Pulse) -> Vec<Write> {
        let Some(at) = pulse.prompt_at.filter(|_| self.split_next) else {
            return vec![Write::now(pulse.bytes)];
        };
        self.split_next = false;
        let cut = (at + 8).min(pulse.bytes.len().saturating_sub(1)).max(at);
        vec![
            Write::now(pulse.bytes[..cut].to_vec()),
            Write {
                after_ms: SPLIT_MS,
                bytes: pulse.bytes[cut..].to_vec(),
                close: false,
            },
        ]
    }

    /// One pulse: the packets a command wrote (`early`), the prompt time
    /// packages, then the text. `later` starts the text on a new line, as
    /// output that no command asked for does.
    fn pulse(&mut self, early: Vec<u8>, reply: &str, later: bool) -> Pulse {
        let mut packets = early;
        let ticks = self.build == Build::New || (self.prompt_on && !self.state.afk);
        if self.gmcp && ticks {
            packets.extend(self.vitals());
            packets.extend(self.worth());
            packets.extend(self.combat());
            packets.extend(self.group());
            if self.build == Build::New {
                packets.extend(self.char_state());
                packets.extend(self.weather());
            }
        }
        let mut out = Vec::new();
        if self.order == TickOrder::First {
            out.append(&mut packets);
        }
        let mut text = String::new();
        if later {
            text.push_str("\n\r");
        }
        text.push_str(reply);
        if let Some(line) = self.battle_line() {
            text.push_str(&line);
        }
        if !self.compact {
            text.push_str("\n\r");
        }
        game::send_to_char(&mut out, &text, &self.state);
        out.append(&mut packets);
        let mut prompt_at = None;
        if self.prompt_on {
            prompt_at = Some(out.len());
            out.extend(game::prompt(&self.prompt, &self.fprompt, &self.state));
        }
        if self.eor_on {
            out.extend_from_slice(&[IAC, EOR]);
        } else if self.ga {
            out.extend_from_slice(&[IAC, GA]);
        }
        Pulse {
            bytes: out,
            prompt_at,
        }
    }

    /// `do_prompt`, `act_info.c:2083`.
    fn do_prompt(&mut self, argument: &str) -> Pulse {
        self.prompt_on = true;
        if argument.is_empty() {
            let reply = format!("Current prompt: {}\n\r", self.prompt);
            let early = self.char_prompt();
            return self.pulse(early, &reply, false);
        }
        if argument.eq_ignore_ascii_case("off") {
            self.prompt_on = false;
            let mut reply = String::from("You will no longer see prompts.\n\r");
            if self.build == Build::New {
                let early = self.char_prompt();
                return self.pulse(early, &reply, false);
            }
            // The older builds fall through and store a buffer they never
            // filled.
            self.prompt = UNFILLED.into();
            let _ = write!(reply, "Prompt set to {}\n\r", self.prompt);
            return self.pulse(Vec::new(), &reply, false);
        }
        self.prompt = if argument == "all" {
            PROMPT_ALL.into()
        } else {
            stored(argument)
        };
        let reply = format!("Prompt set to {}\n\r", self.prompt);
        let early = self.char_prompt();
        self.pulse(early, &reply, false)
    }

    /// `do_fprompt`, `act_info.c:2118`.
    fn do_fprompt(&mut self, argument: &str) -> Pulse {
        if argument.is_empty() {
            let reply = if self.fprompt.is_empty() {
                "No fight prompt set. Use 'fprompt <string>' to set one.\n\r".to_string()
            } else {
                format!("Current fight prompt: {}\n\r", self.fprompt)
            };
            return self.pulse(Vec::new(), &reply, false);
        }
        let reply = if argument.eq_ignore_ascii_case("off") {
            self.fprompt.clear();
            "Fight prompt cleared.\n\r".to_string()
        } else {
            self.fprompt = stored(argument);
            format!("Fight prompt set to {}\n\r", self.fprompt)
        };
        let early = self.char_prompt();
        self.pulse(early, &reply, false)
    }

    fn look(&mut self) -> Pulse {
        if self.blind {
            return self.pulse(Vec::new(), "You can't see a thing!\n\r", false);
        }
        let early = if self.gmcp { room_info() } else { Vec::new() };
        self.pulse(early, ROOM_TEXT, false)
    }

    fn fight(&mut self) -> Pulse {
        if self.state.fighting {
            self.stop_fighting();
            return self.pulse(Vec::new(), "A Blackwatch guard flees south.\n\r", false);
        }
        self.state.fighting = true;
        self.state.hit = FIGHT_HIT;
        self.state.position = 8;
        self.state.tank = Some(Tank {
            name: self.name.clone(),
            hit: FIGHT_HIT,
            max_hit: self.state.max_hit,
        });
        self.pulse(Vec::new(), "A Blackwatch guard attacks you!\n\r", false)
    }

    /// `do_bash`, skills.c:2090. A bash that lands prints the line
    /// skills.c:2271 sends you and lags you, so the game holds what you
    /// send next.
    fn bash(&mut self, argument: &str) -> Pulse {
        let target = argument.split_whitespace().next().unwrap_or("");
        if target.is_empty() {
            return self.pulse(Vec::new(), "But you aren't fighting anyone!\n\r", false);
        }
        let lower = target.to_ascii_lowercase();
        let Some((name, them)) = BASHED
            .iter()
            .find(|(name, _)| name.to_ascii_lowercase().starts_with(&lower))
        else {
            return self.pulse(Vec::new(), "They aren't here.\n\r", false);
        };
        self.wait_ms = Some(self.bash_ms);
        let reply = format!("You slam into {name}, and send {them} flying!\n\r");
        self.pulse(Vec::new(), &reply, false)
    }

    /// The fight is over: your health comes back, you stand, and nobody
    /// tanks.
    fn stop_fighting(&mut self) {
        self.state.fighting = false;
        self.state.hit = self.state.max_hit;
        self.state.position = 9;
        self.state.tank = None;
    }

    /// `cast N name`: the affect lands for N hours, or its hours start
    /// over when it is on you already.
    fn cast(&mut self, argument: &str) -> Pulse {
        let (hours, name) = argument
            .split_once(char::is_whitespace)
            .map_or(("", argument), |(h, n)| (h, n.trim()));
        let Ok(hours) = hours.parse::<i64>() else {
            return self.pulse(Vec::new(), "Cast what, for how many hours?\n\r", false);
        };
        match self.affects.iter_mut().find(|a| a.name == name) {
            Some(affect) => affect.duration = hours,
            None => self.affects.push(Affect::spell(name, hours)),
        }
        let reply = format!("You cast {name}.\n\r");
        let early = self.affects();
        self.pulse(early, &reply, false)
    }

    /// One hour of `affect_update`: a timed affect loses an hour, and one
    /// at 0 wears off. Permanent ones stay.
    fn tick(&mut self) -> Pulse {
        self.affects.retain(|a| a.duration != 0);
        for affect in &mut self.affects {
            if affect.duration > 0 {
                affect.duration -= 1;
            }
        }
        let early = self.affects();
        self.pulse(early, "The hour passes.\n\r", false)
    }

    /// `do_quit`, `act_comm.c:2952`. The game saves you, then
    /// `extract_char` frees you, and `free_char` takes each affect off in
    /// turn with `affect_remove` (recycle.c 846), which writes the list
    /// that is left straight to the socket (handler.c 3495). So the lists
    /// shrink to nothing before the goodbye, which waits in the output
    /// buffer until the socket closes. `quit menu`, `quit switch` and
    /// `quit character` go back to the account menu on the same link, and
    /// the next line plays you again with the affects your pfile kept.
    fn quit(&mut self, argument: &str) -> Vec<Write> {
        let arg = argument
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let to_menu = !arg.is_empty()
            && (["menu", "character", "switch"]
                .iter()
                .any(|w| w.starts_with(&arg))
                || arg == "char");
        let mut bytes = self.take_affects_off();
        if to_menu {
            bytes.extend_from_slice(
                b"You step away from the Forsaken Lands and return to your account menu.\n\r",
            );
            bytes.extend_from_slice(
                format!("Press Enter to play {} again.\n\r", self.name).as_bytes(),
            );
            self.logged_in = false;
            return vec![Write::now(bytes)];
        }
        bytes.extend_from_slice(b"Alas, all good things must come to an end.\n\r");
        vec![Write {
            after_ms: 0,
            bytes,
            close: true,
        }]
    }

    /// `free_char` takes your affects off one at a time, first to last,
    /// and each removal sends Char.Affects. Your pfile keeps them.
    fn take_affects_off(&mut self) -> Vec<u8> {
        let pfile = self.affects.clone();
        let mut out = Vec::new();
        while !self.affects.is_empty() {
            self.affects.remove(0);
            if self.gmcp {
                out.extend(self.affects());
            }
        }
        self.affects = pfile;
        out
    }

    fn lament(&mut self) -> Pulse {
        self.state.lament = !self.state.lament;
        let reply = if self.state.lament {
            "Tears fall as the lament takes you.\n\r"
        } else {
            "The lament fades from your mind.\n\r"
        };
        // The song lands or ends, and Char.Affects goes out at once.
        let early = self.affects();
        self.pulse(early, reply, false)
    }

    fn blind(&mut self) -> Pulse {
        self.blind = !self.blind;
        self.state.exits = if self.blind {
            "[Exits: --- ]".into()
        } else {
            "[Exits: S]".into()
        };
        let reply = if self.blind {
            "You are blinded!\n\r"
        } else {
            "You can see again.\n\r"
        };
        self.pulse(Vec::new(), reply, false)
    }

    /// The line `process_output` prints about your opponent before the
    /// prompt, when you can see it. Lamented tears drops it on 243cac5c
    /// and the new build.
    fn battle_line(&self) -> Option<String> {
        if !self.state.fighting || self.blind {
            return None;
        }
        if self.state.lament && self.build != Build::Older {
            return None;
        }
        let wound = wound(OPPONENT_PCT);
        let mut line = format!("{OPPONENT} {wound} \n\r");
        line.replace_range(..1, &line[..1].to_ascii_uppercase());
        Some(line)
    }

    fn packet(&self, package: &str, json: &str) -> Vec<u8> {
        if self.gmcp {
            gmcp(package, json)
        } else {
            Vec::new()
        }
    }

    fn status(&self) -> Vec<u8> {
        let level = if self.state.immortal {
            IMMORTAL_LEVEL
        } else {
            MORTAL_LEVEL
        };
        self.packet(
            "Char.Status",
            &format!(
                r#"{{"name":{},"level":{level},"race":"human","class":"dark-knight"}}"#,
                quote(&self.name)
            ),
        )
    }

    /// Char.Prompt, sent by the new build alone.
    fn char_prompt(&self) -> Vec<u8> {
        if self.build != Build::New {
            return Vec::new();
        }
        self.packet(
            "Char.Prompt",
            &format!(
                r#"{{"enabled":{},"prompt":{},"fprompt":{}}}"#,
                self.prompt_on,
                raw_quote(&self.prompt),
                raw_quote(&self.fprompt)
            ),
        )
    }

    fn vitals(&self) -> Vec<u8> {
        let st = &self.state;
        let json = match (self.state.lament, self.build) {
            (true, Build::New) => {
                r#"{"hp":0,"maxhp":0,"mana":0,"maxmana":0,"move":0,"maxmove":0,"hidden":true}"#
                    .to_string()
            }
            (true, Build::Unflagged) => {
                r#"{"hp":0,"maxhp":0,"mana":0,"maxmana":0,"move":0,"maxmove":0}"#.to_string()
            }
            _ => format!(
                r#"{{"hp":{},"maxhp":{},"mana":{},"maxmana":{},"move":{},"maxmove":{}}}"#,
                st.hit, st.max_hit, st.mana, st.max_mana, st.moves, st.max_move
            ),
        };
        self.packet("Char.Vitals", &json)
    }

    fn worth(&self) -> Vec<u8> {
        let st = &self.state;
        self.packet(
            "Char.Worth",
            &format!(
                r#"{{"gold":{},"bank":5000,"exp":{},"tnl":{},"trains":3,"practices":12,"cps":{},"rps":{},"cabal":"none"}}"#,
                st.gold, st.exp, st.tnl, st.cp, st.rp
            ),
        )
    }

    fn combat(&self) -> Vec<u8> {
        if !self.state.fighting {
            return self.packet("Char.Combat", "{}");
        }
        let target = quote(OPPONENT);
        let withheld = self.state.lament || self.blind;
        let json = match self.build {
            Build::New => {
                let mut json = if withheld {
                    format!(r#"{{"target":{target},"hidden":true"#)
                } else {
                    format!(
                        r#"{{"target":{target},"condition":"{}","hp_pct":{OPPONENT_PCT}"#,
                        condition(OPPONENT_PCT)
                    )
                };
                if let Some(tank) = &self.state.tank {
                    if self.state.lament {
                        let _ = write!(json, r#","tank":{{"name":{}}}"#, quote(&tank.name));
                    } else {
                        let _ = write!(
                            json,
                            r#","tank":{{"name":{},"hp_pct":{}}}"#,
                            quote(&tank.name),
                            100 * tank.hit / tank.max_hit.max(1)
                        );
                    }
                }
                json.push('}');
                json
            }
            Build::Unflagged if withheld => format!(r#"{{"target":{target}}}"#),
            _ => format!(
                r#"{{"target":{target},"condition":"{}","hp_pct":{OPPONENT_PCT}}}"#,
                condition(OPPONENT_PCT)
            ),
        };
        self.packet("Char.Combat", &json)
    }

    /// You and a loyal wolf.
    fn group(&self) -> Vec<u8> {
        let json = match (self.state.lament, self.build) {
            (true, Build::New) => r#"{"hidden":true}"#.to_string(),
            (true, Build::Unflagged) => "{}".to_string(),
            _ => {
                let st = &self.state;
                let pct = |v: i64, max: i64| if max > 0 { v * 100 / max } else { 0 };
                format!(
                    r#"{{"leader":{name},"members":[{{"id":1769388810,"name":{name},"level":50,"class":"dark-knight","hp_pct":{},"mana_pct":{},"move_pct":{},"tnl":{}}},{{"id":1769401002,"name":"a loyal wolf","level":32,"class":"mob","hp_pct":91,"mana_pct":100,"move_pct":88,"tnl":0}}]}}"#,
                    pct(st.hit, st.max_hit),
                    pct(st.mana, st.max_mana),
                    pct(st.moves, st.max_move),
                    st.tnl,
                    name = quote(&self.name),
                )
            }
        };
        self.packet("Group.Info", &json)
    }

    /// Char.State, sent by the new build alone.
    fn char_state(&self) -> Vec<u8> {
        let position = POSITIONS
            .get(self.state.position)
            .copied()
            .unwrap_or("unknown");
        self.packet(
            "Char.State",
            &format!(
                r#"{{"position":"{position}","language":{}}}"#,
                quote(&self.state.lang)
            ),
        )
    }

    /// Room.Weather, sent by the new build alone.
    fn weather(&self) -> Vec<u8> {
        let st = &self.state;
        self.packet(
            "Room.Weather",
            &format!(
                r#"{{"sky":{},"temp":{},"unit":"F","region":{}}}"#,
                quote(&st.sky),
                st.temp,
                quote(&st.region)
            ),
        )
    }

    fn affects(&self) -> Vec<u8> {
        let json = match (self.state.lament, self.build) {
            (true, Build::New) => r#"{"affects":[],"hidden":true}"#.to_string(),
            (true, Build::Unflagged) => r#"{"affects":[]}"#.to_string(),
            (true, Build::Older) => format!(
                r#"{{"affects":[{BLESS},{{"kind":"song","name":"lamented tears","duration":5,"level":42,"location":"none","modifier":0}}]}}"#
            ),
            (false, _) => {
                let rows: Vec<String> = self.affects.iter().map(Affect::json).collect();
                format!(r#"{{"affects":[{}]}}"#, rows.join(","))
            }
        };
        self.packet("Char.Affects", &json)
    }
}

/// The room you stand in, as `do_look` prints it.
/// Who stands in the room for `bash`, and the pronoun `$M` gives them.
const BASHED: [(&str, &str); 3] = [("Tolliver", "him"), ("Maren", "her"), ("Orla", "her")];

const ROOM_TEXT: &str = "The Bank of Aabahran\n\r  Marble counters line the hall, and a clerk nods at you.\n\r[Exits: south]\n\r";

const BLESS: &str =
    r#"{"kind":"spell","name":"bless","duration":6,"level":50,"location":"hitroll","modifier":4}"#;

/// `position_table`, from dead to standing.
const POSITIONS: [&str; 10] = [
    "dead",
    "mortally wounded",
    "incapacitated",
    "stunned",
    "meditate",
    "sleeping",
    "resting",
    "sitting",
    "fighting",
    "standing",
];

fn room_info() -> Vec<u8> {
    gmcp(
        "Room.Info",
        r#"{"num":5279,"name":"The Bank of Aabahran","area":"Fort Blackwatch","terrain":"inside","sector":0,"region":0,"climate":"Temperate","exits":{"south":5233}}"#,
    )
}

fn world_time() -> Vec<u8> {
    gmcp(
        "World.Time",
        r#"{"hour":14,"day":3,"month":5,"year":1203,"sunlight":"light","sky":"cloudy"}"#,
    )
}

fn world_moons() -> Vec<u8> {
    gmcp(
        "World.Moons",
        r#"{"moons":[{"name":"Lysenties","active":true,"phase":4,"phase_name":"full"},{"name":"Nercuros","active":true,"phase":0,"phase_name":"new"},{"name":"Dyphrities","active":false,"phase":7,"phase_name":"waning crescent"}],"eclipse":false,"triad":false,"near_alignment":true}"#,
    )
}

/// What `do_prompt` and `do_fprompt` store for a setting you typed: the
/// first [`KEEP`] characters, `~` smashed to `-`, trailing spaces gone,
/// and a space after it unless it ends in `%c` in any case.
pub fn stored(typed: &str) -> String {
    let mut text: String = typed.trim_end_matches(' ').chars().take(KEEP).collect();
    text = text.replace('~', "-");
    let lower = text.to_ascii_lowercase();
    if !lower.ends_with("%c") {
        text.push(' ');
    }
    text
}

/// The battle line's words for an opponent's health.
fn wound(pct: i64) -> &'static str {
    match pct {
        100.. => "is in excellent condition.",
        90..=99 => "has a few scratches.",
        75..=89 => "has some small wounds and bruises.",
        50..=74 => "has quite a few wounds.",
        30..=49 => "has some big nasty wounds and scratches.",
        15..=29 => "looks pretty hurt.",
        0..=14 => "is in awful condition.",
        _ => "is bleeding to death.",
    }
}

/// Char.Combat's word for an opponent's health.
fn condition(pct: i64) -> &'static str {
    match pct {
        100.. => "excellent",
        90..=99 => "a few scratches",
        75..=89 => "small wounds",
        50..=74 => "quite a few wounds",
        30..=49 => "big nasty wounds",
        15..=29 => "pretty hurt",
        0..=14 => "awful",
        _ => "bleeding to death",
    }
}

/// A JSON string.
fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A JSON string as `json_escape_raw` writes it: quotes and backslashes
/// escaped, control characters as `\u00XX`, colour codes kept.
fn raw_quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The count after `spam` or `pulses`, or `default`.
fn count(text: &str, default: usize) -> usize {
    text.split_whitespace()
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(default)
        .max(1)
}
