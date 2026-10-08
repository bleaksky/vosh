//! P5 of the perf set, searching a week of logs. A heavy week of play
//! goes into a fresh log file through the real `LogStore`, and the test
//! times the searches the Settings log view runs over it.
//!
//! The week comes from a fixed generator, with its seed and its size. Seven
//! days of two four hour sessions, each hour 30 minutes of fighting, 15 of
//! walking and 15 idle with chat, plus a short session to 127.0.0.1 every
//! third day, which the view hides. That is 702,987 lines in an 83.5 MB
//! file, the heavy week the plan reckons with.
//!
//! Skipped by default. Run it with
//! `cargo test -p vosh-log --release --test p5_search -- --ignored --nocapture`.
//! A dev build runs it too, many times slower, since `SQLite` and the
//! regex engine build there without optimization.
//!
//! A fresh search opens the log again first, as the view does at
//! launch, so `SQLite` starts with no pages of its own. Warm is the
//! median of [`REPS`] more on that connection. A run that writes the
//! week leaves the whole file in the system cache, so every number it
//! prints is a cached read.
//!
//! To time cold reads, set `VOSH_P5_DB` to a file path and run once to
//! write the week there. A later run reuses that file and checks it
//! without reading the lines or their index. After `sudo purge`, a whole
//! run times only the session list and the first search cold, since
//! that search reads every line into the cache. To time any one step
//! cold, run `sudo purge` and then run with `VOSH_P5_ONLY` set to its
//! name as the run prints it, such as `rare name` or `common word, page
//! 2`. The test then times that step alone.
//!
//! In release each first page must come in under a second, fresh, so a
//! search never keeps you waiting. `p5_last_7_days_of_eight_weeks` holds
//! the same bar over eight weeks of the same play, searching the last 7
//! days of the world, as the view opens, so a log that grows for years
//! searches as fast as a week.
//!
//! The numbers alone guard nothing, so each search is also held to what
//! a plain scan of the week finds. The newest 500 matches, oldest first,
//! with their line ids, the count in scope, the local sessions left
//! out, and the page before the first.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use regex::RegexBuilder;
use rusqlite::{Connection, OpenFlags};
use vosh_log::{LogEntry, LogStore, Scope, SearchOptions, SearchPage};

/// Searches of each query on the connection after the fresh one.
const REPS: usize = 5;

/// What the view asks for in a page, `LOG_PAGE_SIZE` in `logView.ts`.
const PAGE: usize = 500;

/// The step that times the session list.
const SESSION_LIST: &str = "session list";

/// The step that times the page before the first `common word` page.
const PAGE_2: &str = "common word, page 2";

/// The generator's seed and week.
const SEED: u64 = 0xA5A5_1234;
const DAYS: u64 = 7;
const SESSIONS_PER_DAY: u64 = 2;
const HOURS_PER_SESSION: u64 = 4;

const GAME_HOST: &str = "play.theforsakenlands.com";
const GAME_PORT: u16 = 9009;

// ---------- the week's generator ----------

/// splitmix64, so the week is the same on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[self.below(xs.len() as u64) as usize]
    }
    fn chance(&mut self, per_million: u64) -> bool {
        self.below(1_000_000) < per_million
    }
}

const MOBS: &[&str] = &[
    "a Blackwatch guard",
    "the city cityguard",
    "a scruffy mercenary",
    "a hungry wolf",
    "a cave troll",
    "an orc warrior",
    "the temple acolyte",
    "a giant spider",
    "a skeletal knight",
    "a wandering merchant",
    "a frost giant",
    "a black bear",
    "a goblin shaman",
    "an elven ranger",
    "the harbor master",
    "a sewer rat",
    "a dark priestess",
    "a drunken sailor",
    "a sand serpent",
    "a ghostly figure",
];
const VERBS: &[&str] = &[
    "scratches",
    "grazes",
    "hits",
    "injures",
    "wounds",
    "mauls",
    "decimates",
    "devastates",
    "maims",
    "MUTILATES",
    "DISEMBOWELS",
    "DISMEMBERS",
    "MASSACRES",
    "misses",
    "=== OBLITERATES ===",
];
const ATTACKS: &[&str] = &[
    "slash", "pierce", "bite", "punch", "claw", "crush", "stab", "blast",
];
const CONDS: &[&str] = &[
    "is in excellent condition.",
    "has a few scratches.",
    "has some small wounds and bruises.",
    "has quite a few wounds.",
    "has some big nasty wounds and scratches.",
    "looks pretty hurt.",
    "is in awful condition.",
];
const ROOMS: &[&str] = &[
    "The Bank of Aabahran",
    "Market Square",
    "A Narrow Alley",
    "The Temple Steps",
    "Before the City Gates",
    "A Dusty Road",
    "Deep in the Forest",
    "A Dark Cave",
    "The Harbor Docks",
    "A Mountain Pass",
    "The Guild Hall",
    "Inside the Tavern",
    "A Muddy Trail",
    "The Old Bridge",
    "A Sandy Beach",
    "Ruins of a Watchtower",
];
const WORDS: &[&str] = &[
    "the",
    "a",
    "of",
    "and",
    "stone",
    "walls",
    "rise",
    "above",
    "you",
    "while",
    "torches",
    "flicker",
    "in",
    "iron",
    "sconces",
    "worn",
    "path",
    "leads",
    "north",
    "toward",
    "distant",
    "hills",
    "smell",
    "smoke",
    "drifts",
    "from",
    "nearby",
    "hearth",
    "cobbles",
    "underfoot",
    "are",
    "slick",
    "with",
    "rain",
    "merchants",
    "call",
    "their",
    "wares",
    "ancient",
    "trees",
    "loom",
    "overhead",
    "branches",
    "creak",
    "wind",
    "moss",
    "covers",
    "roots",
    "faint",
    "light",
    "filters",
    "through",
    "canopy",
    "heavy",
    "wooden",
    "door",
    "stands",
    "open",
    "east",
    "west",
    "south",
    "marble",
    "counters",
    "line",
    "hall",
    "clerk",
    "nods",
    "at",
    "waves",
    "crash",
    "against",
    "rocks",
    "below",
    "gulls",
    "circle",
    "dark",
    "water",
];
const EXITS: &[&str] = &[
    "[Exits: north south]",
    "[Exits: east west]",
    "[Exits: north east south west]",
    "[Exits: south]",
    "[Exits: north up]",
    "[Exits: down]",
    "[Exits: east south west]",
];
const PLAYERS: &[&str] = &[
    "Tester", "Healer", "Grisvald", "Brennan", "Corwyn", "Ottile", "Elsbeth", "Orla",
];
const CHANNELS: &[&str] = &["gossips", "says", "tells you", "tells the group", "cabal"];
const TICKS: &[&str] = &[
    "You are hungry.",
    "You are thirsty.",
    "The sun rises in the east.",
    "The day has begun.",
    "The sun slowly disappears in the west.",
    "The night has begun.",
    "It starts to rain.",
    "The rain stopped.",
    "You feel less tired.",
];
const COMMANDS: &[&str] = &[
    "> n",
    "> s",
    "> e",
    "> w",
    "> u",
    "> d",
    "> look",
    "> kick",
    "> bash",
    "> c 'cure light'",
    "> c 'armor'",
    "> score",
    "> inv",
    "> eq",
    "> rest",
    "> stand",
    "> gt heading back",
    "> get all corpse",
    "> sac corpse",
    "> aff",
];

fn desc_line(r: &mut Rng) -> String {
    let mut s = String::from("  ");
    let target = 58 + r.below(18) as usize;
    while s.len() < target {
        if s.len() > 2 {
            s.push(' ');
        }
        s.push_str(r.pick(WORDS));
    }
    s.push('.');
    s
}

fn chat(r: &mut Rng, rare_name: bool) -> (String, Vec<u8>) {
    let who = if rare_name {
        "Morwenna"
    } else {
        r.pick(PLAYERS)
    };
    let ch = r.pick(CHANNELS);
    let mut msg = String::new();
    let n = 4 + r.below(10);
    for i in 0..n {
        if i > 0 {
            msg.push(' ');
        }
        msg.push_str(r.pick(WORDS));
    }
    if r.chance(40_000) {
        msg.push_str(if r.chance(500_000) { " sell" } else { " buy" });
        msg.push_str(" sword");
    }
    let plain = format!("{who} {ch} '{msg}'");
    let raw = format!("\x1b[0;35m{who} {ch} \x1b[1;37m'{msg}'\x1b[0m");
    (plain, raw.into_bytes())
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// One line of the play, kept to check what each search finds. Its line
/// id is its place in the kept rows plus [`Week::first`].
struct Row {
    session: i64,
    ts: i64,
    text: String,
}

/// The play as it went into the log, or would have: a week, or the
/// last week of eight.
struct Week {
    /// The rows at or after the time the writer kept from.
    rows: Vec<Row>,
    /// The line id of the first kept row.
    first: i64,
    /// Every row written, kept or not.
    total: i64,
    /// Each session's id and whether it went to this machine.
    sessions: Vec<(i64, bool)>,
}

impl Week {
    /// The kept row with line id `id`.
    fn row(&self, id: i64) -> &Row {
        &self.rows[(id - self.first) as usize]
    }
}

/// Where the week's rows go. With no store it only keeps them, for a
/// log a run before wrote.
struct Writer {
    store: Option<LogStore>,
    /// Rows before this time go in the log but are not kept.
    keep_from: i64,
    batch: Vec<LogEntry>,
    week: Week,
}

impl Writer {
    fn start_session(&mut self, host: &str, port: u16, at: i64) -> i64 {
        let id = self.week.sessions.len() as i64 + 1;
        if let Some(store) = self.store.as_mut() {
            let made = store.start_session(host, port, at).expect("a session");
            assert_eq!(made, id, "the log was not empty");
        }
        self.week.sessions.push((id, host == "127.0.0.1"));
        id
    }

    fn set_character(&mut self, id: i64, name: &str) {
        if let Some(store) = self.store.as_mut() {
            store.set_session_character(id, name).expect("a character");
        }
    }

    fn end_session(&mut self, id: i64, at: i64) {
        self.flush(1);
        if let Some(store) = self.store.as_mut() {
            store.end_session(id, at).expect("the end");
        }
    }

    fn push(&mut self, entry: LogEntry) {
        self.week.total += 1;
        if entry.ts_ms >= self.keep_from {
            if self.week.rows.is_empty() {
                self.week.first = self.week.total;
            }
            self.week.rows.push(Row {
                session: entry.session_id,
                ts: entry.ts_ms,
                text: entry.text.clone(),
            });
        }
        if self.store.is_some() {
            self.batch.push(entry);
        }
    }

    /// Write the rows waiting once there are `at_least`.
    fn flush(&mut self, at_least: usize) {
        if self.batch.len() < at_least || self.batch.is_empty() {
            return;
        }
        if let Some(store) = self.store.as_mut() {
            store.append_batch(&self.batch).expect("the rows");
        }
        self.batch.clear();
    }
}

/// One session of play.
struct Play<'a> {
    w: &'a mut Writer,
    r: Rng,
    sid: i64,
    ts: i64,
    hp: i64,
}

impl Play<'_> {
    fn out(&mut self, plain: String, raw: Vec<u8>) {
        self.w.push(LogEntry {
            session_id: self.sid,
            ts_ms: self.ts,
            text: plain,
            raw: Some(raw),
            kind: vosh_log::LineKind::Text,
        });
    }
    fn plain(&mut self, s: &str) {
        self.out(s.to_string(), s.as_bytes().to_vec());
    }
    fn colored(&mut self, s: &str, code: &str) {
        let raw = format!("\x1b[{code}m{s}\x1b[0m");
        self.out(s.to_string(), raw.into_bytes());
    }
    fn sent(&mut self, s: &str) {
        self.w.push(LogEntry {
            session_id: self.sid,
            ts_ms: self.ts,
            text: s.to_string(),
            raw: None,
            kind: vosh_log::LineKind::Text,
        });
    }
    fn prompt(&mut self) {
        // Real prompt lines run about 55 raw bytes for 29 plain
        // (fixtures/prompt/aabahran/lines.json).
        let plain = format!("[{}/1020hp 800/800mn 930/930mv]", self.hp);
        let raw = format!(
            "\x1b[0;37m[\x1b[1;32m{}\x1b[0;37m/1020hp \x1b[1;36m800\x1b[0;37m/800mn \x1b[1;33m930\x1b[0;37m/930mv]\x1b[0m",
            self.hp
        );
        self.out(plain, raw.into_bytes());
    }
    fn round(&mut self, mob: &str) {
        if self.r.chance(500_000) {
            let c = self.r.pick(&COMMANDS[7..11]);
            self.sent(c);
        }
        // fight-tank.bin: attack, condition, blank, tank line, vitals.
        // Heavy play adds your extra attacks and a groupmate's, so 3 to 5
        // damage lines a round.
        let n = 3 + self.r.below(3);
        for _ in 0..n {
            let att = self.r.pick(ATTACKS);
            let verb = self.r.pick(VERBS);
            if self.r.chance(500_000) {
                self.colored(&format!("Your {att} {verb} {mob}!"), "1;33");
            } else {
                let cap = capitalize(mob);
                self.colored(&format!("{cap}'s {att} {verb} you."), "1;31");
            }
        }
        let cond = self.r.pick(CONDS);
        self.plain(&format!("{} {}", capitalize(mob), cond));
        self.plain("");
        self.colored("Tester: [===|===|===|---]", "1;32");
        self.hp = 300 + self.r.below(720) as i64;
        self.prompt();
        self.ts += 3000; // PULSE_VIOLENCE, 12 pulses of 250 ms
        self.w.flush(1000);
    }
    fn room(&mut self) {
        let c = self.r.pick(&COMMANDS[0..6]);
        self.sent(c);
        let title = self.r.pick(ROOMS);
        self.colored(title, "1;36");
        let n = 3 + self.r.below(3);
        for _ in 0..n {
            let d = desc_line(&mut self.r);
            self.plain(&d);
        }
        let ex = self.r.pick(EXITS);
        self.colored(ex, "0;32");
        for _ in 0..self.r.below(3) {
            let mob = capitalize(self.r.pick(MOBS));
            self.colored(&format!("{mob} is here."), "0;33");
        }
        self.plain("");
        self.prompt();
        self.ts += 1500;
        self.w.flush(1000);
    }
    fn idle(&mut self) {
        let k = self.r.below(10);
        if k < 5 {
            let rare = self.r.chance(2_000);
            let (p, raw) = chat(&mut self.r, rare);
            self.out(p, raw);
        } else if k < 7 {
            let t = self.r.pick(TICKS);
            self.plain(t);
        } else if k < 9 {
            let mob = capitalize(self.r.pick(MOBS));
            let dir = self.r.pick(&["north", "south", "east", "west"]);
            self.plain(&format!("{mob} arrives from the {dir}."));
        } else {
            let c = self.r.pick(&COMMANDS[6..]);
            self.sent(c);
        }
        self.plain("");
        self.prompt();
        self.ts += 2000;
        self.w.flush(1000);
    }

    /// One heavy hour. 30 minutes fighting (600 rounds of 3 s), 15
    /// walking (600 rooms at 1.5 s) and 15 idle (450 events at 2 s),
    /// interleaved.
    fn hour(&mut self) {
        let (fights, rooms, idles) = (600, 600, 450);
        let (mut f, mut ro, mut id) = (0, 0, 0);
        while f < fights || ro < rooms || id < idles {
            // A block of walking, then a fight, then some idle.
            for _ in 0..20 {
                if ro < rooms {
                    self.room();
                    ro += 1;
                }
            }
            let mob = self.r.pick(MOBS);
            for _ in 0..20 {
                if f < fights {
                    self.round(mob);
                    f += 1;
                }
            }
            for _ in 0..15 {
                if id < idles {
                    self.idle();
                    id += 1;
                }
            }
        }
    }
}

/// When the play starts.
const START_MS: i64 = 1_790_000_000_000;

const DAY_MS: i64 = 86_400_000;

/// Play `days` days into `store`, or only keep them when `store` is
/// None, keeping the rows from `keep_from` on to check against.
fn generate(store: Option<LogStore>, days: u64, keep_from: i64) -> Week {
    let mut w = Writer {
        store,
        keep_from,
        batch: Vec::new(),
        week: Week {
            rows: Vec::new(),
            first: 1,
            total: 0,
            sessions: Vec::new(),
        },
    };
    let start_ms = START_MS;
    for day in 0..days {
        for s in 0..SESSIONS_PER_DAY {
            let begin = start_ms + (day as i64) * 86_400_000 + (s as i64) * 6 * 3_600_000;
            // A short local test session now and then, which the log
            // view hides.
            if day % 3 == 0 && s == 0 {
                let at = begin - 600_000;
                let sid = w.start_session("127.0.0.1", 4000, at);
                let mut play = Play {
                    w: &mut w,
                    r: Rng(SEED ^ (day * 977 + 13)),
                    sid,
                    ts: at,
                    hp: 1020,
                };
                for _ in 0..200 {
                    play.room();
                }
                w.end_session(sid, begin - 1);
            }
            let sid = w.start_session(GAME_HOST, GAME_PORT, begin);
            w.set_character(sid, "Tester");
            let mut play = Play {
                w: &mut w,
                r: Rng(SEED ^ (day * 31 + s * 7 + 1)),
                sid,
                ts: begin,
                hp: 1020,
            };
            for _ in 0..HOURS_PER_SESSION {
                play.hour();
            }
            let end = play.ts;
            w.end_session(sid, end);
        }
    }
    w.week
}

// ---------- the searches ----------

/// One search the view runs. `scoped` searches the newest session only.
struct Query {
    name: &'static str,
    pattern: &'static str,
    case_sensitive: bool,
    scoped: bool,
}

/// The searches the test times. Each one is a first page with its count,
/// as the view asks when you type.
const SUITE: [Query; 9] = [
    Query {
        name: "rare name",
        pattern: "Morwenna",
        case_sensitive: false,
        scoped: false,
    },
    Query {
        name: "common word",
        pattern: "guard",
        case_sensitive: false,
        scoped: false,
    },
    Query {
        name: "regex",
        pattern: "tells you '.*(sell|buy) sword",
        case_sensitive: false,
        scoped: false,
    },
    Query {
        name: "every prompt",
        pattern: r"^\[\d+/\d+hp",
        case_sensitive: false,
        scoped: false,
    },
    Query {
        name: "no match",
        pattern: "xyzzyplugh",
        case_sensitive: false,
        scoped: false,
    },
    Query {
        name: "case sensitive",
        pattern: "DISEMBOWELS",
        case_sensitive: true,
        scoped: false,
    },
    Query {
        name: "every line",
        pattern: "",
        case_sensitive: false,
        scoped: false,
    },
    Query {
        name: "rare name, newest session",
        pattern: "Morwenna",
        case_sensitive: false,
        scoped: true,
    },
    Query {
        name: "no match, newest session",
        pattern: "xyzzyplugh",
        case_sensitive: false,
        scoped: true,
    },
];

/// What the view sends for a page.
fn options(case_sensitive: bool, session_id: Option<i64>, before: Option<i64>) -> SearchOptions {
    SearchOptions {
        case_sensitive,
        max_results: PAGE,
        scope: Scope {
            logs: session_id.map(|id| vec![id]),
            hide_local: true,
            ..Scope::default()
        },
        before_line_id: before,
    }
}

/// What a plain scan of the week finds for one page: the newest matches
/// in scope as (line id, text), oldest first, and with `with_total` the
/// count of every match in scope.
fn scan(
    week: &Week,
    pattern: &str,
    o: &SearchOptions,
    with_total: bool,
) -> (Vec<(i64, String)>, Option<u64>) {
    let regex = RegexBuilder::new(pattern)
        .case_insensitive(!o.case_sensitive)
        .build()
        .expect("the pattern");
    let local = |session: i64| week.sessions[(session - 1) as usize].1;
    let mut hits = Vec::new();
    let mut matched = 0u64;
    for (at, row) in week.rows.iter().enumerate().rev() {
        let id = at as i64 + week.first;
        if o.scope
            .logs
            .as_ref()
            .is_some_and(|l| !l.contains(&row.session))
            || o.before_line_id.is_some_and(|b| id >= b)
            || o.scope.since_ms.is_some_and(|since| row.ts < since)
            || ((o.scope.hide_local || o.scope.world.is_some()) && local(row.session))
            || !regex.is_match(&row.text)
        {
            continue;
        }
        matched += 1;
        if hits.len() < o.max_results {
            hits.push((id, row.text.clone()));
        }
    }
    hits.reverse();
    (hits, with_total.then_some(matched))
}

/// Hold `page` to what the scan finds.
fn check(
    week: &Week,
    what: &str,
    pattern: &str,
    o: &SearchOptions,
    with_total: bool,
    page: &SearchPage,
) {
    let (want, total) = scan(week, pattern, o, with_total);
    let got: Vec<(i64, String)> = page
        .hits
        .iter()
        .map(|h| (h.line_id, h.text.clone()))
        .collect();
    assert_eq!(got.len(), want.len(), "{what}: how many hits");
    if let Some(at) = got.iter().zip(&want).position(|(g, w)| g != w) {
        panic!(
            "{what}: hit {at} is {:?}, the scan finds {:?}",
            got[at], want[at]
        );
    }
    assert_eq!(page.total, total, "{what}: the count");
    for hit in &page.hits {
        let row = week.row(hit.line_id);
        assert_eq!(hit.session_id, row.session, "{what}: a hit's session");
        assert_ne!(hit.host, "127.0.0.1", "{what}: a local hit");
    }
}

/// How long `f` took, and what it returned.
fn timed<T>(f: impl FnOnce() -> T) -> (Duration, T) {
    let t = Instant::now();
    let out = f();
    (t.elapsed(), out)
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

/// The fresh search and the warm ones, in ms.
struct Times {
    fresh: Duration,
    warm: Vec<Duration>,
}

impl Times {
    fn line(&self, what: &str, page: &SearchPage) -> String {
        let mut warm = self.warm.clone();
        warm.sort_unstable();
        format!(
            "P5 {what:<28} fresh {:>7.1} ms  warm {:>7.1} ms (min {:.1}, max {:.1})  hits {:>3}  count {}",
            ms(self.fresh),
            ms(warm[warm.len() / 2]),
            ms(warm[0]),
            ms(warm[warm.len() - 1]),
            page.hits.len(),
            page.total.map_or_else(|| "-".to_string(), |t| t.to_string()),
        )
    }
}

/// Time one page: once on a fresh connection, then [`REPS`] more on it.
fn time_page(
    path: &Path,
    pattern: &str,
    o: &SearchOptions,
    with_total: bool,
) -> (Times, SearchPage) {
    let store = LogStore::open(path).expect("the log");
    let (fresh, page) = timed(|| store.search_page(pattern, o, with_total).expect("a page"));
    let warm = (0..REPS)
        .map(|_| timed(|| store.search_page(pattern, o, with_total).expect("a page")).0)
        .collect();
    (Times { fresh, warm }, page)
}

/// True when the log at `path` holds `week`. It reads the session count
/// and the newest line id only, so the lines and their index stay out of
/// the system cache for the timed reads.
fn holds(path: &Path, week: &Week) -> bool {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("the log, read only");
    let (sessions, last): (i64, Option<i64>) = conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM sessions), (SELECT MAX(id) FROM log_lines)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("the sizes");
    sessions == week.sessions.len() as i64 && last == Some(week.total)
}

/// Removes the temporary folder when the test ends, passed or not.
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[allow(clippy::cast_precision_loss)]
#[ignore = "P5 benchmark, run with --ignored"]
#[test]
fn p5_search_a_heavy_week() {
    let kept = std::env::var_os("VOSH_P5_DB").map(PathBuf::from);
    let _temp;
    let path = if let Some(path) = kept.clone() {
        path
    } else {
        let dir = std::env::temp_dir().join(format!("vosh-p5-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temporary folder");
        _temp = TempDir(dir.clone());
        dir.join("logs.sqlite")
    };

    // With VOSH_P5_ONLY, the one step it names.
    let only = std::env::var("VOSH_P5_ONLY").ok();
    let steps: Vec<&str> = std::iter::once(SESSION_LIST)
        .chain(SUITE.iter().map(|q| q.name))
        .chain(std::iter::once(PAGE_2))
        .collect();
    if let Some(name) = only.as_deref() {
        assert!(
            steps.contains(&name),
            "VOSH_P5_ONLY names no step. The steps are {steps:?}."
        );
    }
    let runs = |step: &str| only.is_none() || only.as_deref() == Some(step);

    let week = if path.exists() {
        let week = generate(None, DAYS, i64::MIN);
        assert!(
            holds(&path, &week),
            "{} holds another log. Remove it and run again.",
            path.display()
        );
        println!("P5 reused {}", path.display());
        week
    } else {
        let store = LogStore::open(&path).expect("the log");
        let (took, week) = timed(|| generate(Some(store), DAYS, i64::MIN));
        println!(
            "P5 wrote the week in {:.1} s, {:.0} rows/s",
            took.as_secs_f64(),
            week.rows.len() as f64 / took.as_secs_f64()
        );
        week
    };
    let size = std::fs::metadata(&path).map_or(0, |m| m.len());
    let local = week.sessions.iter().filter(|s| s.1).count();
    println!(
        "P5 the week: {} rows, {} sessions ({local} local), {:.1} MB",
        week.rows.len(),
        week.sessions.len(),
        size as f64 / 1e6
    );

    // The newest session the view lists, the last game session of the
    // week, since each local one starts before the game session of its
    // day. The scoped searches take it from the week, so a step timed
    // alone reads nothing of the log before it.
    let newest = week
        .sessions
        .iter()
        .rev()
        .find(|s| !s.1)
        .expect("a game session")
        .0;

    // The session list the view loads when it opens, newest first, with
    // each session's line count.
    if runs(SESSION_LIST) {
        let store = LogStore::open(&path).expect("the log");
        let (took, rows) = timed(|| {
            store
                .list_sessions(
                    0,
                    &Scope {
                        hide_local: true,
                        ..Scope::default()
                    },
                )
                .expect("the sessions")
        });
        let got: Vec<(i64, i64)> = rows.iter().map(|r| (r.id, r.line_count)).collect();
        let want: Vec<(i64, i64)> = week
            .sessions
            .iter()
            .rev()
            .filter(|s| !s.1)
            .map(|&(id, _)| {
                let lines = week.rows.iter().filter(|r| r.session == id).count();
                (id, lines as i64)
            })
            .collect();
        assert_eq!(got, want, "the sessions shown");
        assert_eq!(rows[0].id, newest, "the newest session");
        println!("P5 {SESSION_LIST:<28} fresh {:>7.1} ms", ms(took));
    }

    for q in SUITE.iter().filter(|q| runs(q.name)) {
        let o = options(q.case_sensitive, q.scoped.then_some(newest), None);
        let (times, page) = time_page(&path, q.pattern, &o, true);
        check(&week, q.name, q.pattern, &o, true, &page);
        println!("{}", times.line(q.name, &page));
        demo(q.name, &times);
    }

    // The page before, as the view loads it when you scroll up, with no
    // count. The first page comes from the week, which the `common word`
    // step holds the log to, so the file stays cold for this one.
    if runs(PAGE_2) {
        let first = options(false, None, None);
        let before = scan(&week, "guard", &first, false).0.first().map(|h| h.0);
        let o = options(false, None, before);
        let (times, page) = time_page(&path, "guard", &o, false);
        check(&week, PAGE_2, "guard", &o, false, &page);
        assert_eq!(page.hits.len(), PAGE, "a full page before the first");
        println!("{}", times.line(PAGE_2, &page));
        demo(PAGE_2, &times);
    }
}

/// The one second bar: a release build finds the first page of a search,
/// with its count, in under a second, fresh. A dev build only prints
/// its times.
fn demo(what: &str, times: &Times) {
    if cfg!(debug_assertions) {
        return;
    }
    assert!(
        times.fresh < Duration::from_secs(1),
        "{what} took {:.0} ms, over the one second the Phase 10 demo allows",
        ms(times.fresh)
    );
}

/// How many weeks the long log holds.
const WEEKS: u64 = 8;

/// The one second bar over a long log: eight weeks of the same heavy play,
/// about 5.6 million lines, and the searches the view runs when it opens on
/// Last 7 days of the world you play, each first page with its count under
/// a second in release. Skipped by default, since writing the log takes a
/// while. Run it with
/// `cargo test -p vosh-log --release --test p5_search p5_last_7 -- --ignored --nocapture`.
#[allow(clippy::cast_precision_loss)]
#[ignore = "P5 benchmark, run with --ignored"]
#[test]
fn p5_last_7_days_of_eight_weeks() {
    let dir = std::env::temp_dir().join(format!("vosh-p5-weeks-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temporary folder");
    let _temp = TempDir(dir.clone());
    let path = dir.join("logs.sqlite");
    let days = WEEKS * DAYS;
    // The view opens on the 7 days before now, and now is the end of
    // the play, so the span starts an hour before the last week's first
    // session.
    let since = START_MS + (days - DAYS) as i64 * DAY_MS - 3_600_000;
    let store = LogStore::open(&path).expect("the log");
    let (took, weeks) = timed(|| generate(Some(store), days, since));
    let size = std::fs::metadata(&path).map_or(0, |m| m.len());
    println!(
        "P5 wrote {WEEKS} weeks in {:.1} s: {} rows, {} in the last 7 days, {:.1} MB",
        took.as_secs_f64(),
        weeks.total,
        weeks.rows.len(),
        size as f64 / 1e6
    );
    for q in SUITE.iter().filter(|q| !q.scoped) {
        let o = SearchOptions {
            case_sensitive: q.case_sensitive,
            max_results: PAGE,
            scope: Scope {
                world: Some((GAME_HOST.to_string(), GAME_PORT)),
                since_ms: Some(since),
                ..Scope::default()
            },
            before_line_id: None,
        };
        let (times, page) = time_page(&path, q.pattern, &o, true);
        let what = format!("{}, last 7 days", q.name);
        check(&weeks, &what, q.pattern, &o, true, &page);
        println!("{}", times.line(&what, &page));
        demo(&what, &times);
    }
}
