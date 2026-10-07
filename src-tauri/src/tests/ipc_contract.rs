//! The page and the app reach each other by name. The page runs a Rust
//! command by the name of its function and hears an event by the name
//! its sender gave it. Neither build checks those names, so a rename on
//! one side alone breaks the app only once it runs.
//!
//! These tests read the page sources in `src` and the app sources here
//! and hold the names together, the way the preset tests read
//! presets.ts. Every command the page invokes is registered in
//! `generate_handler!` in ipc.rs, and every invoke passes the keys its
//! `#[tauri::command]` fn reads, in camel case. Every event the page
//! listens for has a sender, the app or the page itself. Every name
//! either side sends has a page listener, or sits on [`UNHEARD`] with its
//! reason, so a rename at one of several senders fails too.
//!
//! Each side names its events once, as constants, the page in
//! `src/ipc/events.ts` and the app in `src-tauri/src/app/events.rs`.
//! Every constant there is named by its event's path in upper snake case,
//! `session://prompt-vars` as `PROMPT_VARS`, so an event goes by one
//! identifier on both sides. Every page listen, emit and emitChanged call
//! names its event by a constant from events.ts, unless the name is built
//! at run time, and events.ts holds no constant that no call names.
//!
//! Every page invoke, listen and emit sits in `src/ipc`, one file to a
//! topic, so the rest of the page reaches the app's commands and events
//! only through the wrappers there. The Tauri window API is not an app
//! command and stays with the windows that call it.
//!
//! An event counts as sent by the app when its name is a string in the
//! app code outside tests. Names reach `emit` through constants, helpers
//! and lists, so the scan reads the strings and not the calls. That holds
//! while the app hears no page event, which the scan checks. A string
//! with a `format!` placeholder builds a family of names. Each family
//! sits on [`APP_FAMILIES`] with its reason, so a template the app adds
//! never passes a listen without review.
//!
//! `fixtures/ipc/names.txt` lists every shared name, so a name that
//! changes on both sides in one commit still shows as a diff there.
//! `VOSH_WRITE_IPC_NAMES=1` writes it again.
//!
//! A page call whose name is built at run time cannot be read from the
//! source. Each one sits on [`BUILT_AT_RUN_TIME`] with its reason, and a
//! test fails when an entry there no longer matches a call. An entry
//! names the page function that makes the call, not its file, so moving
//! that function leaves the list as it is.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// What a page call does with the name it passes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Call {
    /// Runs a Rust command.
    Invoke,
    /// Hears an event.
    Listen,
    /// Sends an event.
    Emit,
}

const CORE: &str = "@tauri-apps/api/core";
const EVENT: &str = "@tauri-apps/api/event";

/// The Tauri functions the page passes a name to, with the module each
/// comes from, what the call does, and which argument holds the name.
const CALLEES: &[(&str, &str, Call, usize)] = &[
    ("invoke", CORE, Call::Invoke, 0),
    ("listen", EVENT, Call::Listen, 0),
    ("once", EVENT, Call::Listen, 0),
    ("emit", EVENT, Call::Emit, 0),
    ("emitTo", EVENT, Call::Emit, 1),
];

/// A page call whose name is built at run time.
struct BuiltAtRunTime {
    /// The page function that makes the call, wherever under `src` it is
    /// defined, so a move to another file leaves the entry as it is.
    function: &'static str,
    /// The Tauri function it calls.
    callee: &'static str,
    /// Where its names come from.
    names: Names,
    /// Why the name is built at run time.
    why: &'static str,
}

/// Where the names of a call on [`BUILT_AT_RUN_TIME`] come from.
enum Names {
    /// The function passes on its parameter at this index as the name.
    /// The argument there of each call to the function, in every page file
    /// that defines or imports it, is checked like any other name.
    Param(usize),
    /// A template that starts with this text, which the sender builds the
    /// same way.
    Family(&'static str),
}

/// Every page call whose name is built at run time.
const BUILT_AT_RUN_TIME: &[BuiltAtRunTime] = &[
    BuiltAtRunTime {
        function: "onGmcpPackage",
        callee: "listen",
        names: Names::Family("session://gmcp/"),
        why: "onGmcpPackage hears one GMCP package. The app sends each package \
              on session://gmcp/ and the package name with its dots turned to \
              dashes, in session/gmcp.rs.",
    },
    BuiltAtRunTime {
        function: "emitChanged",
        callee: "emit",
        names: Names::Param(0),
        why: "emitChanged tells the other windows about a Settings value only \
              when it changed. Its callers name the events.",
    },
];

/// A family of event names the app builds at run time.
struct AppFamily {
    /// The text every name in the family starts with, up to the first
    /// placeholder of the app's `format!`.
    prefix: &'static str,
    /// Why the app builds the names and who hears them.
    why: &'static str,
}

/// Every family of event names the app builds at run time. A name the
/// page listens for counts as sent by the app when it starts with one of
/// these, so an entry has to name a scheme and more.
const APP_FAMILIES: &[AppFamily] = &[AppFamily {
    prefix: "session://gmcp/",
    why: "The session sends each GMCP package on session://gmcp/ and the \
          package name with its dots turned to dashes. onGmcpPackage on the \
          page builds the same names, and fixtures/ipc/gmcp-events.json holds \
          both sides to that encoding.",
}];

/// A name the app or the page sends that no page listen hears.
struct Unheard {
    /// The name, or the prefix of a family.
    name: &'static str,
    /// Why nothing listens for it.
    why: &'static str,
}

/// Every name sent with no page listener. Everything else the app or the
/// page sends must have one, so a rename at one sender alone fails even
/// while another sender keeps the old name.
const UNHEARD: &[Unheard] = &[
    Unheard {
        name: r"https?://[^\s<>()\[\]]+",
        why: "The terminal grid's pattern for a web address in the game output. \
              It is no event.",
    },
    Unheard {
        name: "session://alerts-ended",
        why: "A plugin's alerts ended. The page half drops its notices, after \
              R18 (Alerts Q19).",
    },
    Unheard {
        name: "vosh://daylight-changed",
        why: "The game turned to day or night. Switch themes With the game \
              reads it in the page half, after R18 (Alerts Q16).",
    },
    Unheard {
        name: "vosh://preset-edits-changed",
        why: "Your edits to a preset were saved. The Presets page and the \
              trigger cards follow it once they edit presets (Presets Q10).",
    },
];

/// A name argument, as far as the source tells it.
#[derive(Clone, Debug, PartialEq)]
enum Name {
    /// Spelled out in the source, directly or through a constant.
    Fixed(String),
    /// A template that starts with this text.
    Family(String),
    /// Anything else.
    Unknown,
}

/// The arguments object an invoke passes, as far as the source tells it.
#[derive(Clone, Debug, PartialEq)]
enum Args {
    /// No arguments object.
    Absent,
    /// Object literals, one for each branch of a `?:`, each by its keys.
    Keys(Vec<BTreeSet<String>>),
    /// Anything else, such as a variable or a spread.
    Unknown,
}

/// One call the page makes with a name.
struct PageCall {
    /// The file, from the repo root.
    file: String,
    line: usize,
    /// The Tauri function it calls.
    callee: &'static str,
    call: Call,
    /// The name argument as the source writes it.
    arg: String,
    name: Name,
    /// The page function on [`BUILT_AT_RUN_TIME`] that passes the name
    /// on to the Tauri one, for a name read from a call to that function.
    via: Option<&'static str>,
    /// The innermost named page function the call sits in.
    within: Option<String>,
    /// Which parameter of that function the name argument is, when it is
    /// no more than one.
    param: Option<usize>,
    /// The arguments object of an invoke. Absent for any other call.
    args: Args,
}

impl PageCall {
    fn at(&self) -> String {
        format!("{} line {}", self.file, self.line)
    }
}

/// The names both sides use, read from the sources.
struct Contract {
    /// The commands registered in `generate_handler!`, each with the keys
    /// its function reads, or None when the scan cannot read them.
    commands: BTreeMap<String, Option<Vec<Param>>>,
    /// The strings in the app code outside tests that hold `://`, each
    /// with the files that hold it.
    app_names: BTreeMap<String, BTreeSet<String>>,
    /// Every call the page makes with a name.
    calls: Vec<PageCall>,
    /// The event constants of [`PAGE_EVENTS`] and [`APP_EVENTS`].
    event_constants: Vec<EventConstant>,
    /// Code the scan cannot follow.
    problems: Vec<String>,
}

/// The page file that names every event the page hears or sends, and the
/// app file that names every event the app sends.
const PAGE_EVENTS: &str = "src/ipc/events.ts";
const APP_EVENTS: &str = "src-tauri/src/app/events.rs";

/// The page folder that holds every call into the app and every event.
const PAGE_IPC: &str = "src/ipc/";

/// An event name a file holds as a constant.
struct EventConstant {
    /// [`PAGE_EVENTS`] or [`APP_EVENTS`].
    file: &'static str,
    ident: String,
    name: String,
}

impl Contract {
    /// Whether the app builds names that start `prefix` with `format!`.
    fn builds(&self, prefix: &str) -> bool {
        self.app_names
            .keys()
            .any(|n| template_head(n) == Some(prefix))
    }

    /// The prefixes of the families on `families` that the app builds.
    /// A family off the list never counts, so a template such as
    /// `{scheme}://{host}` cannot pass every name the page listens for.
    fn app_families<'a>(&'a self, families: &'a [AppFamily]) -> impl Iterator<Item = &'a str> {
        families.iter().map(|f| f.prefix).filter(|p| self.builds(p))
    }

    /// Whether a page listen hears `name`.
    fn heard(&self, name: &Name) -> bool {
        self.calls
            .iter()
            .filter(|c| c.call == Call::Listen)
            .any(|c| match (&c.name, name) {
                (Name::Fixed(heard), Name::Fixed(sent)) => heard == sent,
                (Name::Fixed(heard), Name::Family(sent)) => heard.starts_with(sent.as_str()),
                (Name::Family(heard), Name::Fixed(sent) | Name::Family(sent)) => {
                    sent.starts_with(heard.as_str())
                }
                _ => false,
            })
    }

    /// Every name the app or the page sends, by its text or the prefix of
    /// its family, with who sends it. A family the app builds off
    /// `families` is left to the family check.
    fn sends(&self, families: &[AppFamily]) -> BTreeMap<String, (Name, BTreeSet<String>)> {
        let mut sends: BTreeMap<String, (Name, BTreeSet<String>)> = BTreeMap::new();
        let mut add = |name: Name, who: String| {
            let key = match &name {
                Name::Fixed(text) | Name::Family(text) => text.clone(),
                Name::Unknown => return,
            };
            sends
                .entry(key)
                .or_insert_with(|| (name, BTreeSet::new()))
                .1
                .insert(who);
        };
        for (name, files) in self.app_sends(families) {
            for file in files {
                add(name.clone(), file.clone());
            }
        }
        for call in self.calls.iter().filter(|c| c.call == Call::Emit) {
            add(call.name.clone(), call.at());
        }
        sends
    }

    /// The names the app sends, each with the files that hold it. A
    /// family the app builds off `families` is left to the family check.
    fn app_sends<'a>(
        &'a self,
        families: &'a [AppFamily],
    ) -> impl Iterator<Item = (Name, &'a BTreeSet<String>)> + 'a {
        self.app_names.iter().filter_map(|(text, files)| {
            let name = match template_head(text) {
                None => Name::Fixed(text.clone()),
                Some(head) if families.iter().any(|f| f.prefix == head) => {
                    Name::Family(head.into())
                }
                Some(_) => return None,
            };
            Some((name, files))
        })
    }

    /// Whether the app or the page sends `name`.
    fn sent(&self, name: &Name, families: &[AppFamily]) -> bool {
        let page = || self.calls.iter().filter(|c| c.call == Call::Emit);
        match name {
            Name::Fixed(event) => {
                self.app_names.contains_key(event)
                    || self.app_families(families).any(|p| event.starts_with(p))
                    || page().any(|c| match &c.name {
                        Name::Fixed(sent) => sent == event,
                        Name::Family(p) => event.starts_with(p.as_str()),
                        Name::Unknown => false,
                    })
            }
            Name::Family(prefix) => {
                self.app_families(families).any(|p| p == prefix) || page().any(|c| c.name == *name)
            }
            Name::Unknown => false,
        }
    }
}

/// The text of a `format!` string before its first placeholder, or None
/// when it has none.
fn template_head(text: &str) -> Option<&str> {
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c == '{' {
            if chars.peek().map(|&(_, n)| n) == Some('{') {
                chars.next();
            } else {
                return Some(&text[..i]);
            }
        }
    }
    None
}

fn contract() -> &'static Contract {
    static CONTRACT: OnceLock<Contract> = OnceLock::new();
    CONTRACT.get_or_init(|| {
        let app = read_app();
        let page = read_page();
        Contract {
            commands: app.commands,
            app_names: app.names,
            calls: page.calls,
            event_constants: page.events.into_iter().chain(app.events).collect(),
            problems: [app.problems, page.problems].concat(),
        }
    })
}

fn fail_with(failures: Vec<String>) {
    assert!(failures.is_empty(), "\n{}\n", failures.join("\n"));
}

// The checks. Each one returns what it finds wrong, so a test can run it
// on a contract built by hand and see that it rejects its case.

/// Every command the page invokes is registered.
fn unregistered_invokes(contract: &Contract) -> Vec<String> {
    let mut failures = Vec::new();
    for call in contract.calls.iter().filter(|c| c.call == Call::Invoke) {
        match &call.name {
            Name::Fixed(command) if !contract.commands.contains_key(command) => {
                failures.push(format!(
                    "{} invokes {command}, and generate_handler! in src-tauri/src/ipc.rs \
                     registers no command by that name.",
                    call.at()
                ));
            }
            Name::Family(prefix) if !contract.commands.keys().any(|c| c.starts_with(prefix)) => {
                failures.push(format!(
                    "{} invokes commands that start {prefix}, and generate_handler! \
                     registers none.",
                    call.at()
                ));
            }
            _ => {}
        }
    }
    failures
}

/// Every invoke passes the keys its command's function reads and no
/// other, so a renamed parameter on either side fails.
fn invoke_arguments(contract: &Contract) -> Vec<String> {
    let mut failures = Vec::new();
    for call in contract.calls.iter().filter(|c| c.call == Call::Invoke) {
        let Name::Fixed(command) = &call.name else {
            continue;
        };
        let Some(Some(params)) = contract.commands.get(command) else {
            continue;
        };
        let objects = match &call.args {
            Args::Absent => vec![BTreeSet::new()],
            Args::Keys(objects) => objects.clone(),
            Args::Unknown => {
                failures.push(format!(
                    "{} passes {command} arguments the contract test cannot read. Pass \
                     an object literal with every key spelled out.",
                    call.at()
                ));
                continue;
            }
        };
        for keys in objects {
            for key in &keys {
                if !params.iter().any(|p| p.key == *key) {
                    failures.push(format!(
                        "{} passes {command} the key {key}, and its #[tauri::command] fn \
                         has no parameter by that name.",
                        call.at()
                    ));
                }
            }
            for param in params.iter().filter(|p| !p.optional) {
                if !keys.contains(&param.key) {
                    failures.push(format!(
                        "{} invokes {command} without the key {}, which its \
                         #[tauri::command] fn needs.",
                        call.at(),
                        param.key
                    ));
                }
            }
        }
    }
    failures
}

/// Every event the page listens for has a sender.
fn unsent_listens(contract: &Contract, families: &[AppFamily]) -> Vec<String> {
    let mut failures = Vec::new();
    for call in contract.calls.iter().filter(|c| c.call == Call::Listen) {
        match &call.name {
            Name::Fixed(event) if !contract.sent(&call.name, families) => failures.push(format!(
                "{} listens for {event}, and neither the app nor the page sends it.",
                call.at()
            )),
            Name::Family(prefix) if !contract.sent(&call.name, families) => failures.push(format!(
                "{} listens for names that start {prefix}, and neither the app nor the \
                 page builds one.",
                call.at()
            )),
            _ => {}
        }
    }
    failures
}

/// Every family of names the app builds is on `families`, each entry
/// there names a scheme and more, and each matches a family the app
/// builds.
fn unlisted_app_families(contract: &Contract, families: &[AppFamily]) -> Vec<String> {
    let mut failures = Vec::new();
    for (name, files) in &contract.app_names {
        let Some(head) = template_head(name) else {
            continue;
        };
        if !families.iter().any(|f| f.prefix == head) {
            failures.push(format!(
                "{} builds the name {name:?} at run time, and APP_FAMILIES lists no \
                 family that starts {head:?}. Add it with who hears those names, or \
                 spell the names out.",
                files.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
    }
    for family in families {
        let named = family
            .prefix
            .split_once("://")
            .is_some_and(|(scheme, rest)| !scheme.is_empty() && !rest.is_empty());
        if !named {
            failures.push(format!(
                "APP_FAMILIES lists {:?}, which ends at or before its ://, so it would \
                 pass every name in the scheme. List the family by more of its names.",
                family.prefix
            ));
        }
        if family.why.trim().is_empty() {
            failures.push(format!(
                "APP_FAMILIES lists {:?} with no reason.",
                family.prefix
            ));
        }
        if !contract.builds(family.prefix) {
            failures.push(format!(
                "APP_FAMILIES lists {:?}, and the app builds no name that starts with \
                 it. Remove the entry.",
                family.prefix
            ));
        }
    }
    failures
}

/// Every name the app or the page sends has a page listener, or sits on
/// `unheard` with its reason, and every entry there is sent and unheard.
fn unheard_sends(contract: &Contract, families: &[AppFamily], unheard: &[Unheard]) -> Vec<String> {
    let mut failures = Vec::new();
    let sends = contract.sends(families);
    for (key, (name, who)) in &sends {
        if contract.heard(name) || unheard.iter().any(|u| u.name == key) {
            continue;
        }
        let who = who.iter().cloned().collect::<Vec<_>>().join(", ");
        let what = match name {
            Name::Family(prefix) => format!("names that start {prefix}"),
            _ => key.clone(),
        };
        failures.push(format!(
            "{who} sends {what}, and no page listen hears it. Fix the name, or add \
             it to UNHEARD with why nothing listens."
        ));
    }
    for entry in unheard {
        if entry.why.trim().is_empty() {
            failures.push(format!("UNHEARD lists {} with no reason.", entry.name));
        }
        match sends.get(entry.name) {
            None => failures.push(format!(
                "UNHEARD lists {}, and nothing sends it. Remove the entry.",
                entry.name
            )),
            Some((name, _)) if contract.heard(name) => failures.push(format!(
                "UNHEARD lists {}, and a page listen hears it. Remove the entry.",
                entry.name
            )),
            Some(_) => {}
        }
    }
    failures
}

/// The identifier an event constant takes, its event's path in upper
/// snake case.
fn event_ident(name: &str) -> String {
    let path = name.split_once("://").map_or(name, |(_, path)| path);
    path.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// Every event constant on either side is named by its event's path in
/// upper snake case, so an event goes by one identifier on both.
fn misnamed_event_constants(contract: &Contract) -> Vec<String> {
    contract
        .event_constants
        .iter()
        .filter(|c| c.ident != event_ident(&c.name))
        .map(|c| {
            format!(
                "{} holds {} as {}. Name it by its path in upper snake case, {}.",
                c.file,
                c.name,
                c.ident,
                event_ident(&c.name)
            )
        })
        .collect()
}

/// Every page call that hears or sends a name the source spells out
/// names it by its constant from [`PAGE_EVENTS`], and every constant
/// there names the event of a call. A name built at run time is left to
/// [`BUILT_AT_RUN_TIME`].
fn events_not_named_by_constant(contract: &Contract) -> Vec<String> {
    let constants: BTreeMap<&str, &str> = contract
        .event_constants
        .iter()
        .filter(|c| c.file == PAGE_EVENTS)
        .map(|c| (c.ident.as_str(), c.name.as_str()))
        .collect();
    let mut failures = Vec::new();
    let mut named = BTreeSet::new();
    for call in contract.calls.iter().filter(|c| c.call != Call::Invoke) {
        let Name::Fixed(name) = &call.name else {
            continue;
        };
        if constants.get(call.arg.as_str()) == Some(&name.as_str()) {
            named.insert(call.arg.as_str());
        } else {
            failures.push(format!(
                "{} names {name} as {}. Name it by its constant in {PAGE_EVENTS}, and add \
                 one there if it has none.",
                call.at(),
                call.arg
            ));
        }
    }
    for ident in constants.keys().filter(|i| !named.contains(*i)) {
        failures.push(format!(
            "{PAGE_EVENTS} holds {ident}, and no call names it. Remove it."
        ));
    }
    failures
}

/// Every page call that runs a command, hears or sends an event sits in
/// [`PAGE_IPC`].
fn calls_outside_ipc(contract: &Contract) -> Vec<String> {
    contract
        .calls
        .iter()
        .filter(|c| !c.file.starts_with(PAGE_IPC))
        .map(|c| {
            format!(
                "{} calls {} outside {PAGE_IPC}. Call it from a wrapper in the topic \
                 file there that owns it.",
                c.at(),
                c.callee
            )
        })
        .collect()
}

/// The list of every name the page and the app share, so a change to one
/// shows as a diff in the commit that makes it.
const NAMES_FILE: &str = "fixtures/ipc/names.txt";

/// Every name the page and the app share, one to a line. Each registered
/// command with the keys its fn reads, `?` marking one the page may leave
/// out. Each event with who sends it and whether a page listen hears it,
/// a family of names ending in `*`. Events are the names in a scheme the
/// page listens or sends in.
fn shared_names(contract: &Contract, families: &[AppFamily]) -> String {
    let mut lines = Vec::new();
    for (command, params) in &contract.commands {
        let keys = match params {
            Some(params) => params
                .iter()
                .map(|p| format!("{}{}", p.key, if p.optional { "?" } else { "" }))
                .collect::<Vec<_>>()
                .join(", "),
            None => "unread".into(),
        };
        lines.push(format!("command {command}({keys})"));
    }
    let key = |name: &Name| match name {
        Name::Fixed(text) => Some(text.clone()),
        Name::Family(prefix) => Some(format!("{prefix}*")),
        Name::Unknown => None,
    };
    let schemes: BTreeSet<&str> = contract
        .calls
        .iter()
        .filter(|c| c.call != Call::Invoke)
        .filter_map(|c| match &c.name {
            Name::Fixed(text) | Name::Family(text) => text.split_once("://").map(|(s, _)| s),
            Name::Unknown => None,
        })
        .collect();
    // Each event by its key, with its name and whether the app and the
    // page send it.
    let mut events: BTreeMap<String, (Name, bool, bool)> = BTreeMap::new();
    let mut add = |name: &Name, app: bool, page: bool| {
        let Some(key) = key(name) else {
            return;
        };
        let entry = events.entry(key).or_insert((name.clone(), false, false));
        entry.1 |= app;
        entry.2 |= page;
    };
    for (name, _) in contract.app_sends(families) {
        add(&name, true, false);
    }
    for call in &contract.calls {
        match call.call {
            Call::Emit => add(&call.name, false, true),
            Call::Listen => add(&call.name, false, false),
            Call::Invoke => {}
        }
    }
    for (key, (name, app, page)) in &events {
        let scheme = key.split_once("://").map(|(s, _)| s);
        if !scheme.is_some_and(|s| schemes.contains(s)) {
            continue;
        }
        let from = match (app, page) {
            (true, true) => "app page",
            (true, false) => "app",
            (false, true) => "page",
            (false, false) => "nobody",
        };
        let heard = if contract.heard(name) {
            "heard"
        } else {
            "unheard"
        };
        lines.push(format!("event {key} {from} {heard}"));
    }
    lines.push(String::new());
    lines.join("\n")
}

#[test]
fn every_shared_name_is_on_the_list_in_fixtures() {
    let names = shared_names(contract(), APP_FAMILIES);
    let path = repo().join(NAMES_FILE);
    if std::env::var_os("VOSH_WRITE_IPC_NAMES").is_some() {
        fs::write(&path, &names).unwrap_or_else(|e| panic!("{NAMES_FILE} does not write, {e}"));
        return;
    }
    let saved = fs::read_to_string(&path).unwrap_or_default();
    if saved == names {
        return;
    }
    let saved: BTreeSet<&str> = saved.lines().collect();
    let now: BTreeSet<&str> = names.lines().collect();
    let gone = saved.difference(&now).map(|line| format!("- {line}"));
    let new = now.difference(&saved).map(|line| format!("+ {line}"));
    panic!(
        "\nThe names the page and the app share are not the ones {NAMES_FILE} lists.\n{}\n\
         A shared name changes only in a commit tied to a numbered bug or a lettered \
         decision, or with the dead code it belongs to. Write the list again with \
         VOSH_WRITE_IPC_NAMES=1 and commit its diff with that change.\n",
        gone.chain(new).collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn every_command_the_page_invokes_is_registered() {
    fail_with(unregistered_invokes(contract()));
}

#[test]
fn every_invoke_passes_the_keys_its_command_reads() {
    fail_with(invoke_arguments(contract()));
}

#[test]
fn every_event_the_page_listens_for_has_a_sender() {
    fail_with(unsent_listens(contract(), APP_FAMILIES));
}

#[test]
fn every_name_sent_has_a_listener() {
    fail_with(unheard_sends(contract(), APP_FAMILIES, UNHEARD));
}

#[test]
fn every_family_the_app_builds_is_on_the_list() {
    fail_with(unlisted_app_families(contract(), APP_FAMILIES));
}

#[test]
fn every_name_built_at_run_time_is_on_the_list() {
    fail_with(unlisted_run_time_names(contract(), BUILT_AT_RUN_TIME));
}

#[test]
fn every_event_constant_is_named_by_its_path() {
    fail_with(misnamed_event_constants(contract()));
}

#[test]
fn every_page_event_is_named_by_its_constant() {
    fail_with(events_not_named_by_constant(contract()));
}

#[test]
fn every_page_call_sits_in_src_ipc() {
    fail_with(calls_outside_ipc(contract()));
}

/// Every page call whose name is built at run time is on `list`, and
/// every entry there matches a call.
fn unlisted_run_time_names(contract: &Contract, list: &[BuiltAtRunTime]) -> Vec<String> {
    let mut failures = Vec::new();
    for call in &contract.calls {
        if matches!(call.name, Name::Fixed(_)) {
            continue;
        }
        if let Some(helper) = call.via {
            failures.push(format!(
                "{} passes {helper} a name the contract test cannot read, {}. Spell it \
                 out or use a constant.",
                call.at(),
                call.arg
            ));
            continue;
        }
        let entry = list
            .iter()
            .find(|e| call.within.as_deref() == Some(e.function) && e.callee == call.callee);
        let Some(entry) = entry else {
            let within = call
                .within
                .as_ref()
                .map(|f| format!(" in {f}"))
                .unwrap_or_default();
            failures.push(format!(
                "{} passes {} a name built at run time{within}, {}. Add it to \
                 BUILT_AT_RUN_TIME with where its names come from.",
                call.at(),
                call.callee,
                call.arg
            ));
            continue;
        };
        match entry.names {
            Names::Family(listed) if call.name != Name::Family(listed.into()) => {
                failures.push(format!(
                    "BUILT_AT_RUN_TIME says the names {} passes {} start {listed}, and \
                     {} builds {:?}.",
                    entry.function,
                    entry.callee,
                    call.at(),
                    call.name
                ));
            }
            Names::Param(index) if call.param != Some(index) => failures.push(format!(
                "BUILT_AT_RUN_TIME says {} passes {} its parameter {index}, and {} \
                 passes {}.",
                entry.function,
                entry.callee,
                call.at(),
                call.arg
            )),
            _ => {}
        }
    }
    for entry in list {
        if entry.why.trim().is_empty() {
            failures.push(format!(
                "BUILT_AT_RUN_TIME lists the {} in {} with no reason.",
                entry.callee, entry.function
            ));
        }
        let matches = contract.calls.iter().any(|c| {
            c.within.as_deref() == Some(entry.function)
                && c.callee == entry.callee
                && c.via.is_none()
                && !matches!(c.name, Name::Fixed(_))
        });
        if !matches {
            failures.push(format!(
                "BUILT_AT_RUN_TIME lists the {} in {}, and no page function by that \
                 name passes {} a name built at run time. Remove the entry.",
                entry.callee, entry.function, entry.callee
            ));
        }
        if let Names::Param(_) = entry.names {
            if !contract.calls.iter().any(|c| c.via == Some(entry.function)) {
                failures.push(format!(
                    "BUILT_AT_RUN_TIME says {} passes on the names its callers give it, \
                     and no page file calls it with one.",
                    entry.function
                ));
            }
        }
    }
    failures
}

#[test]
fn the_scan_follows_every_page_call_and_app_file() {
    let contract = contract();
    fail_with(contract.problems.clone());
    // Guard the scan itself. Each kind of call shows up at least once
    // today, so a scan that finds nothing fails here.
    for call in [Call::Invoke, Call::Listen, Call::Emit] {
        assert!(
            contract
                .calls
                .iter()
                .any(|c| c.call == call && matches!(c.name, Name::Fixed(_))),
            "the scan found no {call:?} call in the page"
        );
    }
    assert!(contract.commands.contains_key("session_connect"));
    assert!(contract.app_names.contains_key("session://output"));
    for file in [PAGE_EVENTS, APP_EVENTS] {
        assert!(
            contract
                .event_constants
                .iter()
                .any(|c| c.file == file && c.name == "session://output"),
            "the scan found no event constant in {file}"
        );
    }
}

/// A page call in `src/ipc/page.ts` for a contract built by hand.
fn page_call(call: Call, arg: &str, name: Name) -> PageCall {
    let &(callee, ..) = CALLEES.iter().find(|c| c.2 == call).unwrap();
    PageCall {
        file: "src/ipc/page.ts".into(),
        line: 1,
        callee,
        call,
        arg: arg.into(),
        name,
        via: None,
        within: None,
        param: None,
        args: Args::Absent,
    }
}

/// Assert that `failures` holds one failure for each of `want`, in order,
/// each naming its case.
fn assert_rejects(check: &str, failures: &[String], want: &[&str]) {
    assert_eq!(
        failures.len(),
        want.len(),
        "{check} reports {failures:#?}, and should report {want:?}"
    );
    for (failure, want) in failures.iter().zip(want) {
        assert!(
            failure.contains(want),
            "{check} reports {failure:?}, not {want:?}"
        );
    }
}

/// A contract built by hand, with a case each check must reject next to
/// cases it must pass. An edit that empties a check fails here.
#[test]
fn each_check_rejects_the_case_it_guards() {
    let fixed = |s: &str| Name::Fixed(s.into());
    let family = |s: &str| Name::Family(s.into());
    let within = |function: &str, call: PageCall| PageCall {
        within: Some(function.into()),
        ..call
    };
    let via = |call: PageCall| PageCall {
        via: Some("tell"),
        ..call
    };
    let invoke = |args: Args| PageCall {
        args,
        ..page_call(Call::Invoke, "'registered'", fixed("registered"))
    };
    let keys = |keys: &[&str]| Args::Keys(vec![keys.iter().map(|&k| k.into()).collect()]);
    let param = |key: &str, optional| Param {
        key: key.into(),
        optional,
    };
    let contract = Contract {
        commands: BTreeMap::from([
            (
                "registered".to_string(),
                Some(vec![param("presetId", false), param("limit", true)]),
            ),
            ("unread".to_string(), None),
        ]),
        app_names: [
            "vosh://sent",
            "vosh://lost",
            "vosh://built/{}",
            "vosh://unlisted/{}",
            "{scheme}://{host}",
        ]
        .into_iter()
        .map(|n| {
            (
                n.to_string(),
                BTreeSet::from(["src-tauri/src/app.rs".to_string()]),
            )
        })
        .collect(),
        calls: vec![
            invoke(keys(&["presetId", "limit"])),
            invoke(keys(&["presetId"])),
            invoke(keys(&["id"])),
            invoke(Args::Absent),
            invoke(Args::Unknown),
            page_call(Call::Invoke, "'unread'", fixed("unread")),
            page_call(Call::Invoke, "'unregistered'", fixed("unregistered")),
            page_call(Call::Listen, "SENT", fixed("vosh://sent")),
            PageCall {
                file: "src/raw.ts".into(),
                ..page_call(Call::Listen, "SENT", fixed("vosh://sent"))
            },
            page_call(Call::Listen, "'vosh://built/x'", fixed("vosh://built/x")),
            page_call(Call::Emit, "PAGE", fixed("vosh://page")),
            page_call(Call::Emit, "QUIET", fixed("vosh://quiet")),
            page_call(Call::Listen, "PAGE", fixed("vosh://page")),
            page_call(Call::Listen, "UNSENT", fixed("vosh://unsent")),
            within(
                "hear",
                page_call(Call::Listen, "built", family("vosh://built/")),
            ),
            within(
                "hearNobody",
                page_call(Call::Listen, "nobody", family("vosh://nobody/")),
            ),
            within("other", page_call(Call::Listen, "unlisted", Name::Unknown)),
            PageCall {
                param: Some(0),
                ..within("tell", page_call(Call::Emit, "event", Name::Unknown))
            },
            via(page_call(Call::Emit, "TOLD_EVENT", fixed("vosh://told"))),
            via(page_call(Call::Emit, "name", Name::Unknown)),
            PageCall {
                param: Some(1),
                ..within("misled", page_call(Call::Emit, "other", Name::Unknown))
            },
        ],
        event_constants: [
            (PAGE_EVENTS, "SENT", "vosh://sent"),
            (PAGE_EVENTS, "PAGE", "vosh://page"),
            (PAGE_EVENTS, "QUIET", "vosh://quiet"),
            (PAGE_EVENTS, "UNSENT", "vosh://unsent"),
            (PAGE_EVENTS, "TOLD_EVENT", "vosh://told"),
            (PAGE_EVENTS, "STALE", "vosh://stale"),
            (APP_EVENTS, "SENT", "vosh://sent"),
            (APP_EVENTS, "PROMPT_VARS", "session://prompt-vars"),
            (APP_EVENTS, "LOST_EVENT", "vosh://lost"),
        ]
        .into_iter()
        .map(|(file, ident, name)| EventConstant {
            file,
            ident: ident.into(),
            name: name.into(),
        })
        .collect(),
        problems: Vec::new(),
    };
    assert_rejects(
        "unregistered_invokes",
        &unregistered_invokes(&contract),
        &["invokes unregistered,"],
    );
    assert_rejects(
        "invoke_arguments",
        &invoke_arguments(&contract),
        &[
            "passes registered the key id,",
            "invokes registered without the key presetId,",
            "invokes registered without the key presetId,",
            "passes registered arguments the contract test cannot read.",
        ],
    );
    let family = |prefix| AppFamily {
        prefix,
        why: "A case.",
    };
    let families = [
        family("vosh://built/"),
        family("vosh://"),
        family("vosh://stale/"),
    ];
    assert_rejects(
        "unsent_listens",
        &unsent_listens(&contract, &families),
        &["listens for vosh://unsent,", "start vosh://nobody/,"],
    );
    assert_rejects(
        "unlisted_app_families",
        &unlisted_app_families(&contract, &families),
        &[
            "the name \"vosh://unlisted/{}\" at run time",
            "the name \"{scheme}://{host}\" at run time",
            "lists \"vosh://\", which ends at or before its ://",
            "lists \"vosh://\", and the app builds no name",
            "lists \"vosh://stale/\", and the app builds no name",
        ],
    );
    let unheard = |name| Unheard {
        name,
        why: "A case.",
    };
    assert_rejects(
        "unheard_sends",
        &unheard_sends(
            &contract,
            &families,
            &[
                unheard("vosh://quiet"),
                unheard("vosh://sent"),
                unheard("vosh://stale"),
            ],
        ),
        &[
            "src-tauri/src/app.rs sends vosh://lost,",
            "src/ipc/page.ts line 1 sends vosh://told,",
            "lists vosh://sent, and a page listen hears it.",
            "lists vosh://stale, and nothing sends it.",
        ],
    );
    let entry = |function, callee, names| BuiltAtRunTime {
        function,
        callee,
        names,
        why: "A case.",
    };
    let list = [
        entry("hear", "listen", Names::Family("vosh://built/")),
        entry("hearNobody", "listen", Names::Family("vosh://nobody/")),
        entry("tell", "emit", Names::Param(0)),
        entry("misled", "emit", Names::Param(0)),
        entry("stale", "listen", Names::Family("vosh://built/")),
    ];
    assert_rejects(
        "unlisted_run_time_names",
        &unlisted_run_time_names(&contract, &list),
        &[
            "a name built at run time in other, unlisted.",
            "passes tell a name the contract test cannot read, name.",
            "says misled passes emit its parameter 0,",
            "says misled passes on the names its callers give it,",
            "lists the listen in stale,",
        ],
    );
    assert_rejects(
        "misnamed_event_constants",
        &misnamed_event_constants(&contract),
        &[
            "src/ipc/events.ts holds vosh://told as TOLD_EVENT. Name it by its path in \
             upper snake case, TOLD.",
            "src-tauri/src/app/events.rs holds vosh://lost as LOST_EVENT.",
        ],
    );
    assert_rejects(
        "events_not_named_by_constant",
        &events_not_named_by_constant(&contract),
        &[
            "src/ipc/page.ts line 1 names vosh://built/x as 'vosh://built/x'.",
            "src/ipc/events.ts holds STALE, and no call names it.",
        ],
    );
    assert_rejects(
        "calls_outside_ipc",
        &calls_outside_ipc(&contract),
        &["src/raw.ts line 1 calls listen outside src/ipc/."],
    );
}

#[test]
fn the_page_scan_finds_calls_wherever_the_page_makes_them() {
    let source = r#"
import { invoke } from '@tauri-apps/api/core';
const quoted = s.replace(/'/g, "\"");
const half = total / 2;
export function Card() {
  return (
    <p title="don't" className={`a ${b}`}>
      Don't press `Enter` here, // or /* there
      <button onClick={() => void invoke<number>('in_jsx', { at: `${invoke('in_template')}` })} />
      {open && <Row label={<b>{invoke('in_child')}</b>} />}
    </p>
  );
}
const pick = <T,>(value: T) => value;
const typed = invoke<Record<string, (a: number) => void>>(
  'with_generics',
);
"#;
    let tokens = page_tokens(source, true).unwrap();
    let names: Vec<String> = calls_to(&tokens, "invoke")
        .into_iter()
        .map(|c| render(c.args[0]))
        .collect();
    assert_eq!(
        names,
        ["'in_jsx'", "'in_template'", "'in_child'", "'with_generics'"]
    );
}

#[test]
fn the_page_scan_skips_names_in_comments_strings_and_jsx_text() {
    let source = r#"
// invoke('in_a_comment')
/* invoke('in_a_block') */
const s = "invoke('in_a_string')";
const t = `invoke('in_a_template')`;
const el = <p>invoke('in_jsx_text')</p>;
other.invoke('a_method');
"#;
    let tokens = page_tokens(source, true).unwrap();
    assert!(calls_to(&tokens, "invoke").is_empty());
}

#[test]
fn the_page_scan_reads_names_through_constants_and_templates() {
    let source = r"
const LOCAL_EVENT = 'vosh://local';
export const SHARED_EVENT: string = 'vosh://shared' as const;
const channel = `session://gmcp/${name}`;
listen(LOCAL_EVENT, cb);
listen(channel, cb);
listen(`vosh://fixed`, cb);
listen(prefix + name, cb);
for (const ev of LIST) {
  listen(ev, cb);
}
export const LATER_EVENT = 'vosh://later';
for (const key in TABLE) listen(key, cb);
const key = 'vosh://key';
let mutable = 'vosh://let';
listen(mutable, cb);
function pick() {
  const { picked } = names;
  listen(picked, cb);
}
const picked = 'vosh://picked';
const TWICE = 'vosh://one';
listen(TWICE, cb);
function inner() {
  const TWICE = 'vosh://two';
}
";
    let tokens = page_tokens(source, false).unwrap();
    let (consts, exported) = constants(&tokens);
    assert_eq!(exported, ["SHARED_EVENT", "LATER_EVENT"]);
    let names: Vec<Name> = calls_to(&tokens, "listen")
        .into_iter()
        .map(|c| classify(c.args[0], |id| consts.get(id).cloned().flatten()))
        .collect();
    assert_eq!(
        names,
        [
            Name::Fixed("vosh://local".into()),
            Name::Family("session://gmcp/".into()),
            Name::Fixed("vosh://fixed".into()),
            // prefix + name
            Name::Unknown,
            // A loop variable takes no later constant's value.
            Name::Unknown,
            Name::Unknown,
            // let, a destructured name and a name given two values.
            Name::Unknown,
            Name::Unknown,
            Name::Unknown,
        ]
    );
}

#[test]
fn the_page_scan_finds_the_function_each_call_sits_in() {
    let source = r"
export async function onPackage<T = any>(
  name: string,
  cb: (data: T) => void,
): Promise<UnlistenFn> {
  const channel = `session://gmcp/${name}`;
  return listen<T>(channel, (event) => cb(event.payload));
}
const tell = async <T,>(event: string, value: T): Promise<void> => {
  await emit(event, value);
};
function typed(): { a: string } {
  listen('vosh://typed', cb);
}
export const useEvent = (name: string, cb: Cb) =>
  useEffect(() => {
    listen(name, cb);
  }, [name]);
const EVENT = 'vosh://event';
function hides(EVENT: string) {
  listen(EVENT, cb);
}
LIST.forEach(({ EVENT }) => listen(EVENT, cb));
listen(EVENT, cb);
";
    let tokens = page_tokens(source, false).unwrap();
    let fns = page_functions(&tokens);
    let (consts, _) = constants(&tokens);
    let read = |callee| {
        calls_to(&tokens, callee)
            .iter()
            .map(|c| {
                let arg = read_arg(&fns, c, 0, |id| consts.get(id).cloned().flatten());
                (arg.text, arg.name, arg.within, arg.param)
            })
            .collect::<Vec<_>>()
    };
    let some = |s: &str| Some(s.to_string());
    assert_eq!(
        read("listen"),
        [
            (
                "channel".into(),
                Name::Family("session://gmcp/".into()),
                some("onPackage"),
                None
            ),
            (
                "'vosh://typed'".into(),
                Name::Fixed("vosh://typed".into()),
                some("typed"),
                None
            ),
            ("name".into(), Name::Unknown, some("useEvent"), Some(0)),
            ("EVENT".into(), Name::Unknown, some("hides"), Some(0)),
            ("EVENT".into(), Name::Unknown, None, None),
            (
                "EVENT".into(),
                Name::Fixed("vosh://event".into()),
                None,
                None
            ),
        ]
    );
    assert_eq!(
        read("emit"),
        [("event".into(), Name::Unknown, some("tell"), Some(0))]
    );
}

#[test]
fn the_page_scan_reads_the_keys_each_invoke_passes() {
    let source = r"
export async function search(
  pattern: string,
  options: { caseSensitive: boolean; readonly maxResults?: number },
) {
  return invoke('a', { pattern, ...options });
}
invoke('b', { presetId, 'with-dash': 1, count: n + 1, });
invoke('c', options?.asIs ? { config, asIs: true } : { config });
invoke('d');
invoke('e', args);
invoke('f', { ...rest });
invoke('g', { [key]: 1 });
invoke('h', options?.args ?? { a });
";
    let tokens = page_tokens(source, false).unwrap();
    let fns = page_functions(&tokens);
    let keys = |objects: &[&[&str]]| {
        Args::Keys(
            objects
                .iter()
                .map(|keys| keys.iter().map(|&k| k.into()).collect())
                .collect(),
        )
    };
    let read: Vec<Args> = calls_to(&tokens, "invoke")
        .iter()
        .map(|c| read_args(c.args.get(1).copied(), |name| spread_keys(&fns, c.at, name)))
        .collect();
    assert_eq!(
        read,
        [
            keys(&[&["caseSensitive", "maxResults", "pattern"]]),
            keys(&[&["count", "presetId", "with-dash"]]),
            keys(&[&["asIs", "config"], &["config"]]),
            Args::Absent,
            Args::Unknown,
            Args::Unknown,
            Args::Unknown,
            Args::Unknown,
        ]
    );
}

#[test]
fn the_app_scan_reads_the_keys_each_command_takes() {
    /// A command's name, its keys with whether each is optional, and
    /// whether its attribute is plain.
    type Read = (String, Vec<(String, bool)>, bool);
    let source = r#"
#[tauri::command]
pub(crate) async fn presets_remove(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
    preset_id: String,
    limit: Option<u32>,
) -> Result<usize, String> {
    Ok(0)
}
/// A doc comment.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn generic<R: Runtime>(
    window: tauri::WebviewWindow<R>,
    mut delta_y: f64,
    _hide_local: bool,
    map: HashMap<String, Vec<u8>>,
    each: Box<dyn Fn(u8) -> u8>,
    text: &'static str,
) {}
#[cfg(test)]
#[tauri::command]
fn only_in_tests(x: u8) {}
#[tauri::command(rename_all = "snake_case")]
fn renamed(preset_id: String) {}
"#;
    let code = app_code(&rust_tokens(source).unwrap());
    let read: Vec<Read> = code
        .commands
        .iter()
        .map(|&(start, plain)| {
            let (name, params) = command_fn(&code.tokens, start).unwrap();
            let params = params.into_iter().map(|p| (p.key, p.optional)).collect();
            (name, params, plain)
        })
        .collect();
    let keys = |keys: &[(&str, bool)]| {
        keys.iter()
            .map(|&(k, optional)| (k.to_string(), optional))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        read,
        [
            (
                "presets_remove".into(),
                keys(&[("presetId", false), ("limit", true)]),
                true
            ),
            (
                "generic".into(),
                keys(&[
                    ("deltaY", false),
                    ("hideLocal", false),
                    ("map", false),
                    ("each", false),
                    ("text", false),
                ]),
                true
            ),
            ("renamed".into(), keys(&[("presetId", false)]), false),
        ]
    );
}

#[test]
fn the_app_scan_reads_strings_outside_tests_only() {
    let source = r##"
//! "vosh://in-a-doc-comment"
const SENT: &str = "vosh://sent";
const RAW: &str = r#"vosh://raw"#;
fn pick<'a>(x: &'a str) -> char {
    let _ = '"';
    app.emit("vosh://inline", ());
    let _ = format!("session://family/{}", x);
    let _ = format!("{scheme}://{host}");
    let _ = format!("vosh://{{literal}}");
    '\''
}
#[cfg(test)]
const HEARD: &str = "vosh://only-in-tests";
#[cfg(test)]
let _ = app.emit("vosh://a-test-statement", ());
#[cfg(all(test, native_surface))]
mod probe {
    const P: &str = "vosh://probe";
}
#[cfg(test)]
#[path = "elsewhere.rs"]
mod elsewhere;
#[cfg(target_os = "macos")]
pub(crate) mod platform;
#[cfg(any(test, debug_assertions))]
const KEPT: &str = "vosh://kept";
"##;
    let code = app_code(&rust_tokens(source).unwrap());
    let names: BTreeSet<&str> = code
        .tokens
        .iter()
        .filter_map(|t| match t {
            RustTok::Str(s) if s.contains("://") => Some(s.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        names,
        BTreeSet::from([
            "session://family/{}",
            "vosh://inline",
            "vosh://kept",
            "vosh://raw",
            "vosh://sent",
            "vosh://{{literal}}",
            "{scheme}://{host}",
        ])
    );
    // A family starts at the first placeholder. One before the :// leaves
    // a head that names no scheme, which the family check rejects.
    let heads: BTreeSet<&str> = names.iter().filter_map(|n| template_head(n)).collect();
    assert_eq!(heads, BTreeSet::from(["session://family/", ""]));
    let mods: Vec<(&str, Option<&str>, bool)> = code
        .mods
        .iter()
        .map(|m| (m.name.as_str(), m.path.as_deref(), m.test))
        .collect();
    assert_eq!(
        mods,
        [
            ("elsewhere", Some("elsewhere.rs"), true),
            ("platform", None, false)
        ]
    );
}

// The page side.

/// What the page scan reads from the code it lexes.
#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    /// A quoted string, or a template with nothing substituted.
    Str(String),
    /// A template with substitutions, holding its text before the first.
    /// The tokens of each substitution follow inside `{` and `}`.
    TplOpen(String),
    TplClose,
    Punct(char),
    Arrow,
    /// A number, a regular expression or a whole JSX element.
    Value,
}

#[derive(Clone, Debug)]
struct Token {
    tok: Tok,
    line: usize,
}

fn is_word(tok: &Tok, word: &str) -> bool {
    matches!(tok, Tok::Ident(w) if w == word)
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '$'
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Words after which the page code starts a new value, so a `/` opens a
/// regular expression and a `<` a JSX element.
const VALUE_KEYWORDS: &[&str] = &[
    "return",
    "typeof",
    "case",
    "do",
    "else",
    "in",
    "of",
    "new",
    "delete",
    "void",
    "throw",
    "instanceof",
    "yield",
    "await",
];

/// A lexer for the page's TypeScript. It keeps what a call needs and
/// passes over comments, the text inside strings, templates and regular
/// expressions, and the text of JSX elements, so a name there never
/// reads as a call. It fails rather than guess when the code does not
/// read.
struct PageLexer {
    src: Vec<char>,
    pos: usize,
    line: usize,
    out: Vec<Token>,
    jsx: bool,
}

/// The tokens of one page file. `jsx` is true for a .tsx file.
fn page_tokens(text: &str, jsx: bool) -> Result<Vec<Token>, String> {
    let mut lexer = PageLexer {
        src: text.chars().collect(),
        pos: 0,
        line: 1,
        out: Vec::new(),
        jsx,
    };
    lexer.code(false)?;
    Ok(lexer.out)
}

impl PageLexer {
    fn peek(&self, ahead: usize) -> Option<char> {
        self.src.get(self.pos + ahead).copied()
    }

    fn push(&mut self, tok: Tok) {
        self.out.push(Token {
            tok,
            line: self.line,
        });
    }

    fn fail<T>(&self, what: &str) -> Result<T, String> {
        Err(format!("{what} at line {}", self.line))
    }

    /// Whether the next token starts a value.
    fn expects_value(&self) -> bool {
        match self.out.last().map(|t| &t.tok) {
            None | Some(Tok::Arrow) => true,
            Some(Tok::Punct(c)) => !matches!(c, ')' | ']' | '}'),
            Some(Tok::Ident(word)) => VALUE_KEYWORDS.contains(&word.as_str()),
            _ => false,
        }
    }

    /// Read code to the end of the file, or with `nested` through the `}`
    /// that closes the block the caller opened.
    fn code(&mut self, nested: bool) -> Result<(), String> {
        let mut depth = 0usize;
        while let Some(c) = self.peek(0) {
            match c {
                '\n' => {
                    self.line += 1;
                    self.pos += 1;
                }
                c if c.is_whitespace() => self.pos += 1,
                '/' if self.peek(1) == Some('/') => {
                    while self.peek(0).is_some_and(|c| c != '\n') {
                        self.pos += 1;
                    }
                }
                '/' if self.peek(1) == Some('*') => self.block_comment()?,
                '/' if self.expects_value() => self.regex()?,
                '\'' | '"' => self.string(c)?,
                '`' => self.template()?,
                '<' if self.jsx
                    && self.expects_value()
                    && self.peek(1).is_some_and(|n| n == '>' || is_ident_start(n)) =>
                {
                    let (pos, line, len) = (self.pos, self.line, self.out.len());
                    if self.element().is_ok() {
                        self.push(Tok::Value);
                    } else {
                        // A generic arrow function such as `<T,>(v: T) => v`.
                        self.pos = pos + 1;
                        self.line = line;
                        self.out.truncate(len);
                        self.push(Tok::Punct('<'));
                    }
                }
                '=' if self.peek(1) == Some('>') => {
                    self.pos += 2;
                    self.push(Tok::Arrow);
                }
                c if is_ident_start(c) => {
                    let start = self.pos;
                    while self.peek(0).is_some_and(is_ident_char) {
                        self.pos += 1;
                    }
                    let word = self.src[start..self.pos].iter().collect();
                    self.push(Tok::Ident(word));
                }
                c if c.is_ascii_digit() => {
                    while self.peek(0).is_some_and(|c| is_ident_char(c) || c == '.') {
                        self.pos += 1;
                    }
                    self.push(Tok::Value);
                }
                '{' => {
                    depth += 1;
                    self.pos += 1;
                    self.push(Tok::Punct('{'));
                }
                '}' => {
                    self.pos += 1;
                    self.push(Tok::Punct('}'));
                    if depth == 0 {
                        return if nested {
                            Ok(())
                        } else {
                            self.fail("a } that closes nothing")
                        };
                    }
                    depth -= 1;
                }
                c => {
                    self.pos += 1;
                    self.push(Tok::Punct(c));
                }
            }
        }
        if nested || depth > 0 {
            self.fail("the end of the file inside a block")
        } else {
            Ok(())
        }
    }

    fn block_comment(&mut self) -> Result<(), String> {
        self.pos += 2;
        loop {
            match self.peek(0) {
                None => return self.fail("a comment that never closes"),
                Some('*') if self.peek(1) == Some('/') => {
                    self.pos += 2;
                    return Ok(());
                }
                Some('\n') => self.line += 1,
                Some(_) => {}
            }
            self.pos += 1;
        }
    }

    fn string(&mut self, quote: char) -> Result<(), String> {
        self.pos += 1;
        let mut text = String::new();
        loop {
            match self.peek(0) {
                None | Some('\n') => return self.fail("a string that runs past its line"),
                Some('\\') => {
                    if self.peek(1) == Some('\n') {
                        self.line += 1;
                    }
                    text.extend(self.peek(1));
                    self.pos += 2;
                }
                Some(c) if c == quote => {
                    self.pos += 1;
                    self.push(Tok::Str(text));
                    return Ok(());
                }
                Some(c) => {
                    text.push(c);
                    self.pos += 1;
                }
            }
        }
    }

    fn template(&mut self) -> Result<(), String> {
        self.pos += 1;
        let mut text = String::new();
        let mut substituted = false;
        loop {
            match self.peek(0) {
                None => return self.fail("a template that never closes"),
                Some('`') => {
                    self.pos += 1;
                    self.push(if substituted {
                        Tok::TplClose
                    } else {
                        Tok::Str(text)
                    });
                    return Ok(());
                }
                Some('\\') => {
                    text.extend(self.peek(1));
                    if self.peek(1) == Some('\n') {
                        self.line += 1;
                    }
                    self.pos += 2;
                }
                Some('$') if self.peek(1) == Some('{') => {
                    if !substituted {
                        self.push(Tok::TplOpen(text.clone()));
                        substituted = true;
                    }
                    self.pos += 2;
                    self.push(Tok::Punct('{'));
                    self.code(true)?;
                }
                Some(c) => {
                    if c == '\n' {
                        self.line += 1;
                    }
                    text.push(c);
                    self.pos += 1;
                }
            }
        }
    }

    fn regex(&mut self) -> Result<(), String> {
        self.pos += 1;
        let mut class = false;
        loop {
            match self.peek(0) {
                None | Some('\n') => {
                    return self.fail("a regular expression that runs past its line")
                }
                Some('\\') => self.pos += 1,
                Some('[') => class = true,
                Some(']') => class = false,
                Some('/') if !class => break,
                Some(_) => {}
            }
            self.pos += 1;
        }
        self.pos += 1;
        while self.peek(0).is_some_and(is_ident_char) {
            self.pos += 1;
        }
        self.push(Tok::Value);
        Ok(())
    }

    fn space(&mut self) {
        while let Some(c) = self.peek(0).filter(|c| c.is_whitespace()) {
            if c == '\n' {
                self.line += 1;
            }
            self.pos += 1;
        }
    }

    fn jsx_name(&mut self) -> String {
        let start = self.pos;
        while self
            .peek(0)
            .is_some_and(|c| is_ident_char(c) || matches!(c, '.' | '-' | ':'))
        {
            self.pos += 1;
        }
        self.src[start..self.pos].iter().collect()
    }

    /// The code inside a `{` and `}` of a JSX element.
    fn jsx_code(&mut self) -> Result<(), String> {
        self.pos += 1;
        self.push(Tok::Punct('{'));
        self.code(true)
    }

    /// A JSX element from its `<`. Only the code inside its braces makes
    /// tokens.
    fn element(&mut self) -> Result<(), String> {
        self.pos += 1;
        let name = self.jsx_name();
        if !name.is_empty() && self.peek(0) == Some('<') {
            self.type_arguments()?;
        }
        loop {
            self.space();
            match self.peek(0) {
                Some('/') if self.peek(1) == Some('>') => {
                    self.pos += 2;
                    return Ok(());
                }
                Some('/') if self.peek(1) == Some('/') => {
                    while self.peek(0).is_some_and(|c| c != '\n') {
                        self.pos += 1;
                    }
                }
                Some('/') if self.peek(1) == Some('*') => self.block_comment()?,
                Some('>') => {
                    self.pos += 1;
                    return self.children(&name);
                }
                Some('{') => self.jsx_code()?,
                Some(c) if is_ident_start(c) && !name.is_empty() => {
                    self.jsx_name();
                    self.space();
                    if self.peek(0) != Some('=') {
                        continue;
                    }
                    self.pos += 1;
                    self.space();
                    match self.peek(0) {
                        Some(quote @ ('"' | '\'')) => {
                            self.pos += 1;
                            while self.peek(0).is_some_and(|c| c != quote) {
                                if self.peek(0) == Some('\n') {
                                    self.line += 1;
                                }
                                self.pos += 1;
                            }
                            if self.peek(0).is_none() {
                                return self.fail("an attribute that never closes");
                            }
                            self.pos += 1;
                        }
                        Some('{') => self.jsx_code()?,
                        Some('<') => self.element()?,
                        _ => return self.fail("an attribute with no value"),
                    }
                }
                _ => return self.fail("not a JSX element"),
            }
        }
    }

    /// The type arguments of an element, as in `<List<Row> />`.
    fn type_arguments(&mut self) -> Result<(), String> {
        let mut depth = 0usize;
        loop {
            match self.peek(0) {
                None => return self.fail("type arguments that never close"),
                Some('=') if self.peek(1) == Some('>') => self.pos += 1,
                Some('<') => depth += 1,
                Some('>') => {
                    depth -= 1;
                    if depth == 0 {
                        self.pos += 1;
                        return Ok(());
                    }
                }
                Some('\n') => self.line += 1,
                Some(_) => {}
            }
            self.pos += 1;
        }
    }

    fn children(&mut self, name: &str) -> Result<(), String> {
        loop {
            match self.peek(0) {
                None => return self.fail("an element that never closes"),
                Some('{') => self.jsx_code()?,
                Some('<') if self.peek(1) == Some('/') => {
                    self.pos += 2;
                    self.space();
                    let closing = self.jsx_name();
                    self.space();
                    if closing != name || self.peek(0) != Some('>') {
                        return self.fail("a closing tag that does not match");
                    }
                    self.pos += 1;
                    return Ok(());
                }
                Some('<') => self.element()?,
                Some(c) => {
                    if c == '\n' {
                        self.line += 1;
                    }
                    self.pos += 1;
                }
            }
        }
    }
}

/// The index of the `(` that opens a call to the name at `at`, past any
/// type arguments, or None when the name is not called there.
fn open_paren(tokens: &[Token], at: usize) -> Option<usize> {
    let mut i = at + 1;
    if tokens.get(i)?.tok == Tok::Punct('<') {
        let mut depth = 0usize;
        loop {
            match tokens.get(i)?.tok {
                Tok::Punct('<') => depth += 1,
                Tok::Punct('>') => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        i += 1;
    }
    (tokens.get(i)?.tok == Tok::Punct('(')).then_some(i)
}

/// A call the page code makes, by the index of the name it calls.
struct Called<'t> {
    at: usize,
    line: usize,
    args: Vec<&'t [Token]>,
}

/// The calls to the function `callee`, each with its line and its
/// arguments. A method of the same name is not the function.
fn calls_to<'t>(tokens: &'t [Token], callee: &str) -> Vec<Called<'t>> {
    let mut found = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        if !is_word(&token.tok, callee) {
            continue;
        }
        let before = i.checked_sub(1).map(|j| &tokens[j].tok);
        if matches!(before, Some(Tok::Punct('.'))) || before.is_some_and(|t| is_word(t, "function"))
        {
            continue;
        }
        if let Some(open) = open_paren(tokens, i) {
            found.push(Called {
                at: i,
                line: token.line,
                args: arguments(&tokens[open + 1..]),
            });
        }
    }
    found
}

/// The arguments of a call, read from just after its `(`.
fn arguments(tokens: &[Token]) -> Vec<&[Token]> {
    let mut args = Vec::new();
    let (mut depth, mut start) = (0usize, 0usize);
    for (i, token) in tokens.iter().enumerate() {
        match token.tok {
            Tok::Punct('(' | '[' | '{') | Tok::TplOpen(_) => depth += 1,
            Tok::Punct(')' | ']' | '}') | Tok::TplClose if depth > 0 => depth -= 1,
            Tok::Punct(')') => {
                if i > start {
                    args.push(&tokens[start..i]);
                }
                break;
            }
            Tok::Punct(',') if depth == 0 => {
                args.push(&tokens[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    args
}

/// Tokens back as source text, close enough to name an argument.
fn render(tokens: &[Token]) -> String {
    tokens
        .iter()
        .map(|t| match &t.tok {
            Tok::Ident(w) => w.clone(),
            Tok::Str(s) => format!("'{s}'"),
            Tok::TplOpen(s) => format!("`{s}"),
            Tok::TplClose => "`".into(),
            Tok::Punct(c) => c.to_string(),
            Tok::Arrow => "=>".into(),
            Tok::Value => "_".into(),
        })
        .collect()
}

/// What an expression gives as a name. `constant` looks up a name the
/// expression is no more than.
fn classify(arg: &[Token], constant: impl Fn(&str) -> Option<Name>) -> Name {
    match arg {
        [one] => match &one.tok {
            Tok::Str(s) => Name::Fixed(s.clone()),
            Tok::Ident(id) => constant(id).unwrap_or(Name::Unknown),
            _ => Name::Unknown,
        },
        [first, .., last] => match (&first.tok, &last.tok) {
            (Tok::TplOpen(prefix), Tok::TplClose) if closes_at_end(arg) => {
                Name::Family(prefix.clone())
            }
            _ => Name::Unknown,
        },
        [] => Name::Unknown,
    }
}

/// Whether the template that opens `tokens` closes at their end.
fn closes_at_end(tokens: &[Token]) -> bool {
    let mut depth = 0usize;
    for (i, token) in tokens.iter().enumerate() {
        match token.tok {
            Tok::TplOpen(_) => depth += 1,
            Tok::TplClose => {
                depth -= 1;
                if depth == 0 {
                    return i + 1 == tokens.len();
                }
            }
            _ => {}
        }
    }
    false
}

/// The constants a file names with a string or a template, and the ones
/// it exports. A name the file gives two values, or binds any other way,
/// maps to None. That covers `let`, a `const` with no value the scan can
/// read such as a loop variable, and a destructured name.
fn constants(tokens: &[Token]) -> (BTreeMap<String, Option<Name>>, Vec<String>) {
    let mut consts: BTreeMap<String, Option<Name>> = BTreeMap::new();
    let mut exported = Vec::new();
    let mut bind = |id: &str, name: Option<Name>| {
        consts
            .entry(id.to_string())
            .and_modify(|old| {
                if *old != name {
                    *old = None;
                }
            })
            .or_insert(name);
    };
    for (i, token) in tokens.iter().enumerate() {
        let Tok::Ident(word) = &token.tok else {
            continue;
        };
        if !matches!(word.as_str(), "const" | "let" | "var")
            || (i > 0 && tokens[i - 1].tok == Tok::Punct('.'))
        {
            continue;
        }
        match tokens.get(i + 1).map(|t| &t.tok) {
            Some(Tok::Ident(id)) => {
                let name = if word == "const" {
                    declared(&tokens[i + 2..])
                } else {
                    None
                };
                if name.is_some() && i > 0 && is_word(&tokens[i - 1].tok, "export") {
                    exported.push(id.clone());
                }
                bind(id, name);
            }
            Some(Tok::Punct('{' | '[')) => {
                let end = closing(tokens, i + 1).unwrap_or(tokens.len());
                for t in &tokens[i + 2..end] {
                    if let Tok::Ident(id) = &t.tok {
                        bind(id, None);
                    }
                }
            }
            _ => {}
        }
    }
    (consts, exported)
}

/// The name a `const` declares, read from just past its identifier: past
/// any type to the `=`, then a string or a template, with or without `as
/// const`. None when it declares anything else, or nothing, as the loop
/// variable of a `for (const x of list)` does.
fn declared(tokens: &[Token]) -> Option<Name> {
    let mut depth = 0usize;
    let mut j = 0;
    loop {
        match &tokens.get(j)?.tok {
            Tok::Punct('(' | '[' | '{' | '<') => depth += 1,
            Tok::Punct(')' | ']' | '}' | '>') => depth = depth.checked_sub(1)?,
            Tok::Punct('=') if depth == 0 => break,
            Tok::Punct(';' | ',') if depth == 0 => return None,
            Tok::Ident(w) if depth == 0 && (w == "of" || w == "in") => return None,
            _ => {}
        }
        j += 1;
    }
    let rest = &tokens[j + 1..];
    let end = rest
        .iter()
        .position(|t| t.tok == Tok::Punct(';'))
        .unwrap_or(rest.len());
    let mut value = &rest[..end];
    if let [head @ .., as_, c] = value {
        if is_word(&as_.tok, "as") && is_word(&c.tok, "const") {
            value = head;
        }
    }
    match classify(value, |_| None) {
        Name::Unknown => None,
        name => Some(name),
    }
}

/// The index of the bracket that closes the one at `open`.
fn closing(tokens: &[Token], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, token) in tokens.iter().enumerate().skip(open) {
        match token.tok {
            Tok::Punct('(' | '[' | '{') => depth += 1,
            Tok::Punct(')' | ']' | '}') => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// The index of the bracket that opens the one that closes at `close`.
fn opening(tokens: &[Token], close: usize) -> Option<usize> {
    let mut depth = 0usize;
    for i in (0..=close).rev() {
        match tokens[i].tok {
            Tok::Punct(')' | ']' | '}') => depth += 1,
            Tok::Punct('(' | '[' | '{') => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// A function the page code defines, with `function` or with `=>`.
struct PageFn {
    /// Its name, for a function declaration or a function that a `const`,
    /// `let` or `var` holds.
    name: Option<String>,
    /// Its parameters in order.
    params: Vec<PageParam>,
    /// Every name its parameters bind, destructured ones too.
    binds: BTreeSet<String>,
    /// Its body, as token indexes.
    body: std::ops::Range<usize>,
}

/// A parameter of a page function.
struct PageParam {
    /// Its name, or None for one the scan cannot name, such as a
    /// destructured one.
    name: Option<String>,
    /// The keys of its type, when that is an object type written out.
    keys: Option<BTreeSet<String>>,
}

/// Every function a page file defines.
fn page_functions(tokens: &[Token]) -> Vec<PageFn> {
    let mut found = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        let (name, params, body) = match &token.tok {
            Tok::Ident(w) if w == "function" => {
                let name = match tokens.get(i + 1).map(|t| &t.tok) {
                    Some(Tok::Ident(name)) => Some(name.clone()),
                    _ => None,
                };
                let at = if name.is_some() { i + 1 } else { i };
                let Some(open) = open_paren(tokens, at) else {
                    continue;
                };
                let Some(close) = closing(tokens, open) else {
                    continue;
                };
                let Some(body) = function_body(tokens, close + 1) else {
                    continue;
                };
                let Some(end) = closing(tokens, body) else {
                    continue;
                };
                let name = name.or_else(|| held_by(tokens, i));
                (name, open + 1..close, body..end + 1)
            }
            Tok::Arrow => {
                let Some((params, start)) = arrow_params(tokens, i) else {
                    continue;
                };
                (held_by(tokens, start), params, arrow_body(tokens, i + 1))
            }
            _ => continue,
        };
        let (params, binds) = parameters(&tokens[params]);
        found.push(PageFn {
            name,
            params,
            binds,
            body,
        });
    }
    found
}

/// The `{` that opens a function body, from just past its parameters and
/// past any return type.
fn function_body(tokens: &[Token], from: usize) -> Option<usize> {
    if tokens.get(from)?.tok != Tok::Punct(':') {
        return (tokens.get(from)?.tok == Tok::Punct('{')).then_some(from);
    }
    let mut depth = 0usize;
    let mut j = from + 1;
    loop {
        match tokens.get(j)?.tok {
            Tok::Punct('<' | '(' | '[') => depth += 1,
            Tok::Punct('>' | ')' | ']') => depth = depth.checked_sub(1)?,
            // An object type follows `:`, `|`, `&` or `=>`, or sits inside
            // brackets. Any other `{` opens the body.
            Tok::Punct('{')
                if depth > 0
                    || matches!(tokens[j - 1].tok, Tok::Punct(':' | '|' | '&') | Tok::Arrow) =>
            {
                j = closing(tokens, j)?;
            }
            Tok::Punct('{') => return Some(j),
            Tok::Punct(';') if depth == 0 => return None,
            _ => {}
        }
        j += 1;
    }
}

/// The parameters of the arrow function whose `=>` is at `arrow`, as the
/// tokens between its parentheses or its one bare parameter, and the
/// index where the function starts.
fn arrow_params(tokens: &[Token], arrow: usize) -> Option<(std::ops::Range<usize>, usize)> {
    let last = arrow.checked_sub(1)?;
    let typed = last >= 2
        && tokens[last - 1].tok == Tok::Punct(':')
        && tokens[last - 2].tok == Tok::Punct(')');
    let close = match tokens[last].tok {
        Tok::Punct(')') => last,
        Tok::Ident(_) if !typed => return Some((last..arrow, last)),
        // A return type sits between the parameters and the `=>`, so walk
        // back over it to the `:` that follows the `)`.
        _ => {
            let mut depth = 0usize;
            let mut j = last;
            loop {
                match tokens[j].tok {
                    Tok::Punct(')' | ']' | '}' | '>') => depth += 1,
                    Tok::Punct('(' | '[' | '{' | '<') => depth = depth.checked_sub(1)?,
                    Tok::Punct(':') if depth == 0 && tokens[j - 1].tok == Tok::Punct(')') => {
                        break j - 1;
                    }
                    Tok::Punct(';' | '=' | ',') | Tok::Arrow if depth == 0 => return None,
                    _ => {}
                }
                j = j.checked_sub(1)?;
            }
        }
    };
    let open = opening(tokens, close)?;
    Some((open + 1..close, open))
}

/// The body of an arrow function from just past its `=>`, a block or an
/// expression.
fn arrow_body(tokens: &[Token], from: usize) -> std::ops::Range<usize> {
    if tokens.get(from).map(|t| &t.tok) == Some(&Tok::Punct('{')) {
        if let Some(close) = closing(tokens, from) {
            return from..close + 1;
        }
    }
    let mut depth = 0usize;
    for (j, token) in tokens.iter().enumerate().skip(from) {
        match token.tok {
            Tok::Punct('(' | '[' | '{') | Tok::TplOpen(_) => depth += 1,
            Tok::Punct(')' | ']' | '}') | Tok::TplClose => match depth.checked_sub(1) {
                Some(d) => depth = d,
                None => return from..j,
            },
            Tok::Punct(';' | ',') if depth == 0 => return from..j,
            _ => {}
        }
    }
    from..tokens.len()
}

/// The name a declaration gives the function that starts at `start`, as
/// in `const name = async <T,>(value: T) => value`.
fn held_by(tokens: &[Token], start: usize) -> Option<String> {
    let mut j = start;
    if j > 0 && tokens[j - 1].tok == Tok::Punct('>') {
        let mut depth = 0usize;
        loop {
            j = j.checked_sub(1)?;
            match tokens[j].tok {
                Tok::Punct('>') => depth += 1,
                Tok::Punct('<') => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    if j > 0 && is_word(&tokens[j - 1].tok, "async") {
        j -= 1;
    }
    match tokens.get(j.checked_sub(3)?..j)? {
        [decl, name, eq]
            if eq.tok == Tok::Punct('=')
                && ["const", "let", "var"]
                    .iter()
                    .any(|w| is_word(&decl.tok, w)) =>
        {
            match &name.tok {
                Tok::Ident(name) => Some(name.clone()),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The parameters between a function's parentheses, and every name they
/// bind.
fn parameters(tokens: &[Token]) -> (Vec<PageParam>, BTreeSet<String>) {
    let mut params = Vec::new();
    let mut binds = BTreeSet::new();
    let mut depth = 0usize;
    let mut each = Vec::new();
    let mut start = 0;
    for (j, token) in tokens.iter().enumerate() {
        match token.tok {
            Tok::Punct('(' | '[' | '{' | '<') => depth += 1,
            Tok::Punct(')' | ']' | '}' | '>') => depth = depth.saturating_sub(1),
            Tok::Punct(',') if depth == 0 => {
                each.push(&tokens[start..j]);
                start = j + 1;
            }
            _ => {}
        }
    }
    each.push(&tokens[start..]);
    for mut param in each {
        while param.first().map(|t| &t.tok) == Some(&Tok::Punct('.')) {
            param = &param[1..];
        }
        match param.first().map(|t| &t.tok) {
            None => {}
            Some(Tok::Ident(name)) => {
                params.push(PageParam {
                    name: Some(name.clone()),
                    keys: object_type(&param[1..]),
                });
                binds.insert(name.clone());
            }
            Some(Tok::Punct('{' | '[')) => {
                params.push(PageParam {
                    name: None,
                    keys: None,
                });
                let end = closing(param, 0).unwrap_or(param.len());
                for t in &param[..end] {
                    if let Tok::Ident(name) = &t.tok {
                        binds.insert(name.clone());
                    }
                }
            }
            Some(_) => params.push(PageParam {
                name: None,
                keys: None,
            }),
        }
    }
    (params, binds)
}

/// The keys of the type annotation that starts a parameter after its
/// name, when the type is an object type written out, as in
/// `options: { caseSensitive: boolean; maxResults?: number }`.
fn object_type(tokens: &[Token]) -> Option<BTreeSet<String>> {
    let mut at = 0;
    if tokens.first()?.tok == Tok::Punct('?') {
        at += 1;
    }
    if tokens.get(at)?.tok != Tok::Punct(':') || tokens.get(at + 1)?.tok != Tok::Punct('{') {
        return None;
    }
    let close = closing(tokens, at + 1)?;
    if !matches!(
        tokens.get(close + 1).map(|t| &t.tok),
        None | Some(Tok::Punct('='))
    ) {
        return None;
    }
    let mut keys = BTreeSet::new();
    let mut depth = 0usize;
    let mut member = true;
    for token in &tokens[at + 2..close] {
        match &token.tok {
            Tok::Punct('(' | '[' | '{' | '<') => depth += 1,
            Tok::Punct(')' | ']' | '}' | '>') => depth = depth.saturating_sub(1),
            Tok::Punct(';' | ',') if depth == 0 => member = true,
            Tok::Ident(w) if depth == 0 && member && w == "readonly" => {}
            Tok::Ident(w) if depth == 0 && member => {
                keys.insert(w.clone());
                member = false;
            }
            _ if depth == 0 && member => return None,
            _ => {}
        }
    }
    Some(keys)
}

/// The keys of `name` spread into an object at `at`, when it is a
/// parameter of a function around it whose type is an object type
/// written out.
fn spread_keys(fns: &[PageFn], at: usize, name: &str) -> Option<BTreeSet<String>> {
    around(fns, at)
        .iter()
        .find_map(|f| f.params.iter().find(|p| p.name.as_deref() == Some(name)))?
        .keys
        .clone()
}

/// The functions around the token at `at`, innermost first.
fn around(fns: &[PageFn], at: usize) -> Vec<&PageFn> {
    let mut found: Vec<&PageFn> = fns.iter().filter(|f| f.body.contains(&at)).collect();
    found.sort_by_key(|f| f.body.len());
    found
}

/// The specifiers of the `import { ... } from` that ends at the module
/// string at `at`, or None for any other form.
fn named_imports(tokens: &[Token], at: usize) -> Option<Vec<Vec<Tok>>> {
    if at < 2 || !is_word(&tokens[at - 1].tok, "from") || tokens[at - 2].tok != Tok::Punct('}') {
        return None;
    }
    let open = (0..at - 2)
        .rev()
        .find(|&j| tokens[j].tok == Tok::Punct('{'))?;
    let import = match open.checked_sub(1).map(|j| &tokens[j].tok) {
        Some(t) if is_word(t, "import") => true,
        Some(t) if is_word(t, "type") => open >= 2 && is_word(&tokens[open - 2].tok, "import"),
        _ => false,
    };
    import.then(|| {
        tokens[open + 1..at - 2]
            .split(|t| t.tok == Tok::Punct(','))
            .filter(|spec| !spec.is_empty())
            .map(|spec| spec.iter().map(|t| t.tok.clone()).collect())
            .collect()
    })
}

struct PageFile {
    path: String,
    tokens: Vec<Token>,
    consts: BTreeMap<String, Option<Name>>,
    exported: Vec<String>,
}

/// The Tauri functions a page file imports under their own names. Any
/// other way into the core or event module is a problem for the scan.
fn tauri_imports(file: &PageFile, problems: &mut Vec<String>) -> BTreeSet<&'static str> {
    let mut imported = BTreeSet::new();
    for (i, token) in file.tokens.iter().enumerate() {
        let Tok::Str(module) = &token.tok else {
            continue;
        };
        if module != CORE && module != EVENT {
            continue;
        }
        let Some(specs) = named_imports(&file.tokens, i) else {
            problems.push(format!(
                "{} line {} reaches {module} in a way the contract test cannot follow. \
                 Import its functions by name.",
                file.path, token.line
            ));
            continue;
        };
        for spec in specs {
            let (name, alias) = match spec.as_slice() {
                [Tok::Ident(name)] => (name, None),
                [Tok::Ident(name), Tok::Ident(r#as), Tok::Ident(alias)] if r#as == "as" => {
                    (name, Some(alias))
                }
                _ => continue,
            };
            let Some(&(callee, ..)) = CALLEES.iter().find(|(c, m, ..)| c == name && m == module)
            else {
                continue;
            };
            match alias {
                None => {
                    imported.insert(callee);
                }
                Some(alias) => problems.push(format!(
                    "{} line {} imports {callee} as {alias}. The contract test reads \
                     calls by the Tauri name, so import it under that name.",
                    file.path, token.line
                )),
            }
        }
    }
    imported
}

/// A name argument as the page scan reads it.
struct Arg {
    /// As the source writes it.
    text: String,
    name: Name,
    /// The innermost named function the call sits in.
    within: Option<String>,
    /// Which parameter of that function the argument is, when it is no
    /// more than one.
    param: Option<usize>,
}

/// The argument at `index` of a call, read with the file's functions and
/// `constant`. A parameter of a function around the call hides a
/// constant of the same name.
fn read_arg(
    fns: &[PageFn],
    called: &Called,
    index: usize,
    constant: impl Fn(&str) -> Option<Name>,
) -> Arg {
    let arg = called.args.get(index).copied().unwrap_or_default();
    let around = around(fns, called.at);
    let name = classify(arg, |id| {
        if around.iter().any(|f| f.binds.contains(id)) {
            None
        } else {
            constant(id)
        }
    });
    let within = around.iter().find(|f| f.name.is_some());
    let param = match arg {
        [Token {
            tok: Tok::Ident(id),
            ..
        }] => within.and_then(|f| f.params.iter().position(|p| p.name.as_ref() == Some(id))),
        _ => None,
    };
    Arg {
        text: render(arg),
        name,
        within: within.and_then(|f| f.name.clone()),
        param,
    }
}

/// The arguments object an invoke passes, from the argument after its
/// name: an object literal, or two in the branches of a `?:`.
fn read_args(arg: Option<&[Token]>, spread: impl Fn(&str) -> Option<BTreeSet<String>>) -> Args {
    let Some(arg) = arg else {
        return Args::Absent;
    };
    let object_keys = |tokens: &[Token]| object_keys(tokens, &spread);
    if let Some(keys) = object_keys(arg) {
        return Args::Keys(vec![keys]);
    }
    let (mut depth, mut question, mut colon) = (0usize, None, None);
    for (i, token) in arg.iter().enumerate() {
        let next = arg.get(i + 1).map(|t| &t.tok);
        let prev = i.checked_sub(1).map(|j| &arg[j].tok);
        match token.tok {
            Tok::Punct('(' | '[' | '{') | Tok::TplOpen(_) => depth += 1,
            Tok::Punct(')' | ']' | '}') | Tok::TplClose => depth = depth.saturating_sub(1),
            // Not `?.` or `??`.
            Tok::Punct('?')
                if depth == 0
                    && question.is_none()
                    && !matches!(next, Some(Tok::Punct('.' | '?')))
                    && prev != Some(&Tok::Punct('?')) =>
            {
                question = Some(i);
            }
            Tok::Punct(':') if depth == 0 && question.is_some() && colon.is_none() => {
                colon = Some(i);
            }
            _ => {}
        }
    }
    if let (Some(q), Some(c)) = (question, colon) {
        if let (Some(yes), Some(no)) = (object_keys(&arg[q + 1..c]), object_keys(&arg[c + 1..])) {
            return Args::Keys(vec![yes, no]);
        }
    }
    Args::Unknown
}

/// The keys of an object literal that is all of `tokens`, or None when
/// it is not one or a key cannot be read. `spread` gives the keys of a
/// name spread into it, as `...options`.
fn object_keys(
    tokens: &[Token],
    spread: impl Fn(&str) -> Option<BTreeSet<String>>,
) -> Option<BTreeSet<String>> {
    if tokens.first()?.tok != Tok::Punct('{') || closing(tokens, 0)? + 1 != tokens.len() {
        return None;
    }
    let inner = &tokens[1..tokens.len() - 1];
    let mut keys = BTreeSet::new();
    let (mut depth, mut start) = (0usize, 0usize);
    let mut props = Vec::new();
    for (i, token) in inner.iter().enumerate() {
        match token.tok {
            Tok::Punct('(' | '[' | '{') | Tok::TplOpen(_) => depth += 1,
            Tok::Punct(')' | ']' | '}') | Tok::TplClose => depth = depth.saturating_sub(1),
            Tok::Punct(',') if depth == 0 => {
                props.push(&inner[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    props.push(&inner[start..]);
    for prop in props {
        match prop {
            [] => {}
            // `key`, `key: value` and `'key': value`.
            [Token {
                tok: Tok::Ident(key),
                ..
            }]
            | [Token {
                tok: Tok::Ident(key) | Tok::Str(key),
                ..
            }, Token {
                tok: Tok::Punct(':'),
                ..
            }, ..] => {
                keys.insert(key.clone());
            }
            [dot1, dot2, dot3, Token {
                tok: Tok::Ident(name),
                ..
            }] if [dot1, dot2, dot3].iter().all(|t| t.tok == Tok::Punct('.')) => {
                keys.extend(spread(name)?);
            }
            _ => return None,
        }
    }
    Some(keys)
}

/// Whether a page file defines the page function `name` or imports it
/// under that name. An import under another name is a problem for the
/// scan.
fn reaches(file: &PageFile, fns: &[PageFn], name: &str, problems: &mut Vec<String>) -> bool {
    let mut found = fns.iter().any(|f| f.name.as_deref() == Some(name));
    for (i, token) in file.tokens.iter().enumerate() {
        if !matches!(token.tok, Tok::Str(_)) {
            continue;
        }
        for spec in named_imports(&file.tokens, i).unwrap_or_default() {
            match spec.as_slice() {
                [Tok::Ident(n)] if n == name => found = true,
                [Tok::Ident(n), Tok::Ident(r#as), Tok::Ident(alias)]
                    if n == name && r#as == "as" =>
                {
                    problems.push(format!(
                        "{} line {} imports {name} as {alias}. The contract test reads \
                         calls to {name} by that name, so import it under that name.",
                        file.path, token.line
                    ));
                }
                _ => {}
            }
        }
    }
    found
}

struct Page {
    calls: Vec<PageCall>,
    events: Vec<EventConstant>,
    problems: Vec<String>,
}

/// Every call the page code makes with a name. Tests and the test
/// helpers in `src/test` are not page code.
fn read_page() -> Page {
    let src = repo().join("src");
    let mut files = Vec::new();
    for path in files_under(&src) {
        let rel = relative(&path);
        let ext = path.extension().and_then(|e| e.to_str());
        let tsx = ext == Some("tsx");
        if !(tsx || ext == Some("ts")) || rel.contains(".test.") || rel.starts_with("src/test/") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{rel} does not open, {e}"));
        let tokens = page_tokens(&text, tsx).unwrap_or_else(|e| {
            panic!("the contract test lost its place in {rel}, {e}. Teach page_tokens this code.")
        });
        let (consts, exported) = constants(&tokens);
        files.push(PageFile {
            path: rel,
            tokens,
            consts,
            exported,
        });
    }
    let events = files
        .iter()
        .filter(|f| f.path == PAGE_EVENTS)
        .flat_map(|f| &f.consts)
        .filter_map(|(ident, name)| match name {
            Some(Name::Fixed(name)) => Some(EventConstant {
                file: PAGE_EVENTS,
                ident: ident.clone(),
                name: name.clone(),
            }),
            _ => None,
        })
        .collect();
    // A constant another file exports, when only one file exports it.
    let mut exported: BTreeMap<&str, Vec<&Name>> = BTreeMap::new();
    for file in &files {
        for id in &file.exported {
            if let Some(Some(name)) = file.consts.get(id) {
                let values = exported.entry(id).or_default();
                if !values.contains(&name) {
                    values.push(name);
                }
            }
        }
    }
    let mut calls = Vec::new();
    let mut problems = Vec::new();
    for file in &files {
        let fns = page_functions(&file.tokens);
        let imported = tauri_imports(file, &mut problems);
        let constant = |id: &str| match file.consts.get(id) {
            Some(name) => name.clone(),
            None => match exported.get(id).map(Vec::as_slice) {
                Some([name]) => Some((*name).clone()),
                _ => None,
            },
        };
        let mut add = |called: &Called,
                       (callee, call): (&'static str, Call),
                       index: usize,
                       via: Option<&'static str>| {
            let arg = read_arg(&fns, called, index, constant);
            let args = match (call, via) {
                (Call::Invoke, None) => read_args(called.args.get(index + 1).copied(), |name| {
                    spread_keys(&fns, called.at, name)
                }),
                _ => Args::Absent,
            };
            calls.push(PageCall {
                file: file.path.clone(),
                line: called.line,
                callee,
                call,
                arg: arg.text,
                name: arg.name,
                via,
                within: arg.within,
                param: arg.param,
                args,
            });
        };
        for &(callee, _, call, index) in CALLEES {
            let found = calls_to(&file.tokens, callee);
            if !imported.contains(callee) {
                for called in found {
                    problems.push(format!(
                        "{} line {} calls {callee} without importing it from Tauri. \
                         The contract test reads only the Tauri function.",
                        file.path, called.line
                    ));
                }
                continue;
            }
            for called in &found {
                add(called, (callee, call), index, None);
            }
        }
        // The names a page function on the list passes on come from its
        // callers, in every file that defines or imports it.
        for entry in BUILT_AT_RUN_TIME {
            let Names::Param(index) = entry.names else {
                continue;
            };
            if !reaches(file, &fns, entry.function, &mut problems) {
                continue;
            }
            let &(callee, _, call, _) = CALLEES.iter().find(|c| c.0 == entry.callee).unwrap();
            for called in &calls_to(&file.tokens, entry.function) {
                add(called, (callee, call), index, Some(entry.function));
            }
        }
        // A method such as a window's listen hears or sends a name the
        // scan cannot see.
        for (i, token) in file.tokens.iter().enumerate() {
            let method = match &token.tok {
                Tok::Ident(w)
                    if CALLEES.iter().any(|c| c.0 == w)
                        || BUILT_AT_RUN_TIME
                            .iter()
                            .any(|e| matches!(e.names, Names::Param(_)) && e.function == w) =>
                {
                    w
                }
                _ => continue,
            };
            if i > 0
                && file.tokens[i - 1].tok == Tok::Punct('.')
                && open_paren(&file.tokens, i).is_some()
            {
                problems.push(format!(
                    "{} line {} calls a method named {method}. The contract test reads \
                     only the Tauri functions, so teach it this call.",
                    file.path, token.line
                ));
            }
        }
    }
    Page {
        calls,
        events,
        problems,
    }
}

// The app side.

/// What the app scan reads from Rust code.
#[derive(Clone, Debug, PartialEq)]
enum RustTok {
    Ident(String),
    Str(String),
    Punct(char),
    /// A number or a char.
    Other,
}

/// The tokens of one Rust file, with comments gone and every kind of
/// string read as a string.
fn rust_tokens(text: &str) -> Result<Vec<RustTok>, String> {
    let s: Vec<char> = text.chars().collect();
    let at = |i: usize| s.get(i).copied();
    let mut out = Vec::new();
    let (mut i, mut line) = (0usize, 1usize);
    while let Some(c) = at(i) {
        match c {
            '\n' => {
                line += 1;
                i += 1;
            }
            c if c.is_whitespace() => i += 1,
            '/' if at(i + 1) == Some('/') => {
                while at(i).is_some_and(|c| c != '\n') {
                    i += 1;
                }
            }
            '/' if at(i + 1) == Some('*') => {
                let mut depth = 0usize;
                loop {
                    match (at(i), at(i + 1)) {
                        (Some('/'), Some('*')) => {
                            depth += 1;
                            i += 2;
                        }
                        (Some('*'), Some('/')) => {
                            depth -= 1;
                            i += 2;
                            if depth == 0 {
                                break;
                            }
                        }
                        (Some(c), _) => {
                            if c == '\n' {
                                line += 1;
                            }
                            i += 1;
                        }
                        (None, _) => {
                            return Err(format!("a comment that never closes at line {line}"))
                        }
                    }
                }
            }
            '"' => {
                let (text, end) = rust_string(&s, i + 1, &mut line)?;
                out.push(RustTok::Str(text));
                i = end;
            }
            '\'' => {
                // A char, or a lifetime whose name follows as a word.
                if at(i + 1) == Some('\\') {
                    i += 3;
                    while at(i).is_some_and(|c| c != '\'') {
                        i += 1;
                    }
                    i += 1;
                    out.push(RustTok::Other);
                } else if at(i + 2) == Some('\'') {
                    i += 3;
                    out.push(RustTok::Other);
                } else {
                    i += 1;
                }
            }
            c if c.is_alphabetic() || c == '_' => {
                let start = i;
                while at(i).is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    i += 1;
                }
                let word: String = s[start..i].iter().collect();
                let hashes = s[i..].iter().take_while(|&&c| c == '#').count();
                if matches!(word.as_str(), "r" | "br" | "cr") && at(i + hashes) == Some('"') {
                    let body = i + hashes + 1;
                    let close: Vec<char> = format!("\"{}", "#".repeat(hashes)).chars().collect();
                    let end = (body..s.len())
                        .find(|&j| s[j..].starts_with(&close))
                        .ok_or_else(|| format!("a raw string that never closes at line {line}"))?;
                    line += s[body..end].iter().filter(|&&c| c == '\n').count();
                    out.push(RustTok::Str(s[body..end].iter().collect()));
                    i = end + close.len();
                } else if matches!(word.as_str(), "b" | "c") && at(i) == Some('"') {
                    let (text, end) = rust_string(&s, i + 1, &mut line)?;
                    out.push(RustTok::Str(text));
                    i = end;
                } else {
                    out.push(RustTok::Ident(word));
                }
            }
            c if c.is_ascii_digit() => {
                while at(i).is_some_and(|c| c.is_alphanumeric() || c == '_')
                    || (at(i) == Some('.') && at(i + 1).is_some_and(|c| c.is_ascii_digit()))
                {
                    i += 1;
                }
                out.push(RustTok::Other);
            }
            c => {
                out.push(RustTok::Punct(c));
                i += 1;
            }
        }
    }
    Ok(out)
}

/// A string from just after its opening quote, with the index after its
/// closing one.
fn rust_string(s: &[char], start: usize, line: &mut usize) -> Result<(String, usize), String> {
    let mut text = String::new();
    let mut i = start;
    loop {
        match s.get(i) {
            None => return Err(format!("a string that never closes at line {line}")),
            Some('"') => return Ok((text, i + 1)),
            Some('\\') => {
                if s.get(i + 1) == Some(&'\n') {
                    *line += 1;
                }
                text.extend(s.get(i + 1));
                i += 2;
            }
            Some(&c) => {
                if c == '\n' {
                    *line += 1;
                }
                text.push(c);
                i += 1;
            }
        }
    }
}

/// A `mod name;` in a Rust file.
#[derive(Debug)]
struct ModDecl {
    name: String,
    /// Its `#[path]`, if it has one.
    path: Option<String>,
    /// Whether it builds only for tests.
    test: bool,
}

/// One Rust file with the code that builds only for tests taken out.
#[derive(Default)]
struct Code {
    tokens: Vec<RustTok>,
    mods: Vec<ModDecl>,
    /// Where each item marked `#[tauri::command]` starts in `tokens`, and
    /// whether the attribute is plain, with no arguments.
    commands: Vec<(usize, bool)>,
}

/// The attribute that starts at `at`, as the index after it, whether it
/// is an inner one, and the tokens inside its brackets.
fn attribute(t: &[RustTok], at: usize) -> Option<(usize, bool, &[RustTok])> {
    if t.get(at)? != &RustTok::Punct('#') {
        return None;
    }
    let inner = t.get(at + 1)? == &RustTok::Punct('!');
    let open = if inner { at + 2 } else { at + 1 };
    if t.get(open)? != &RustTok::Punct('[') {
        return None;
    }
    let close = matching(t, open)?;
    Some((close + 1, inner, &t[open + 1..close]))
}

/// The index of the bracket that closes the one at `open`.
fn matching(t: &[RustTok], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, tok) in t.iter().enumerate().skip(open) {
        match tok {
            RustTok::Punct('(' | '[' | '{') => depth += 1,
            RustTok::Punct(')' | ']' | '}') => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether an attribute builds what follows only for tests.
fn is_test_gate(attr: &[RustTok]) -> bool {
    let word = |t: &RustTok, w: &str| matches!(t, RustTok::Ident(x) if x == w);
    match attr {
        [only] => word(only, "test"),
        [_, RustTok::Punct(':'), RustTok::Punct(':'), last] => word(last, "test"),
        [cfg, RustTok::Punct('('), pred @ .., RustTok::Punct(')')] if word(cfg, "cfg") => {
            requires_test(pred)
        }
        _ => false,
    }
}

/// Whether a cfg predicate holds only in a test build.
fn requires_test(pred: &[RustTok]) -> bool {
    match pred {
        [RustTok::Ident(w)] => w == "test",
        [RustTok::Ident(all), RustTok::Punct('('), args @ .., RustTok::Punct(')')]
            if all == "all" =>
        {
            let mut depth = 0usize;
            args.split(|t| {
                match t {
                    RustTok::Punct('(') => depth += 1,
                    RustTok::Punct(')') => depth -= 1,
                    _ => {}
                }
                depth == 0 && *t == RustTok::Punct(',')
            })
            .any(requires_test)
        }
        _ => false,
    }
}

/// The index just past the item or statement that starts at `start`.
fn item_end(t: &[RustTok], start: usize) -> usize {
    let mut depth = 0usize;
    for (i, tok) in t.iter().enumerate().skip(start) {
        match tok {
            RustTok::Punct('(' | '[' | '{') => depth += 1,
            RustTok::Punct(c @ (')' | ']' | '}')) => {
                if depth == 0 {
                    return i;
                }
                depth -= 1;
                if depth == 0 && *c == '}' {
                    return i + 1;
                }
            }
            RustTok::Punct(';') if depth == 0 => return i + 1,
            _ => {}
        }
    }
    t.len()
}

/// The module a `mod name;` at `at` declares, past any `pub`.
fn mod_name(t: &[RustTok], mut at: usize) -> Option<String> {
    if t.get(at) == Some(&RustTok::Ident("pub".into())) {
        at += 1;
        if t.get(at) == Some(&RustTok::Punct('(')) {
            at = matching(t, at)? + 1;
        }
    }
    match (t.get(at)?, t.get(at + 1)?, t.get(at + 2)?) {
        (RustTok::Ident(m), RustTok::Ident(name), RustTok::Punct(';')) if m == "mod" => {
            Some(name.clone())
        }
        _ => None,
    }
}

/// The code of a Rust file that builds outside tests, and the modules it
/// declares.
fn app_code(t: &[RustTok]) -> Code {
    let mut code = Code::default();
    let mut path = None;
    let mut only_tests = false;
    let mut i = 0;
    while i < t.len() {
        let mut end = i;
        let (mut gate, mut inner) = (false, false);
        let mut command = None;
        while let Some((next, is_inner, attr)) = attribute(t, end) {
            gate |= is_test_gate(attr);
            inner |= is_inner;
            command = command.or_else(|| command_attribute(attr));
            if let [RustTok::Ident(key), RustTok::Punct('='), RustTok::Str(value)] = attr {
                if key == "path" {
                    path = Some(value.clone());
                }
            }
            end = next;
        }
        if end > i {
            if gate && inner {
                only_tests = true;
            } else if gate {
                if let Some(name) = mod_name(t, end) {
                    code.mods.push(ModDecl {
                        name,
                        path: path.take(),
                        test: true,
                    });
                }
                path = None;
                end = item_end(t, end);
            } else if let Some(plain) = command.filter(|_| !only_tests) {
                code.commands.push((code.tokens.len(), plain));
            }
            i = end;
            continue;
        }
        if t[i] == RustTok::Ident("mod".into()) {
            if let Some(name) = mod_name(t, i) {
                code.mods.push(ModDecl {
                    name,
                    path: path.take(),
                    test: only_tests,
                });
            }
        }
        if !only_tests {
            code.tokens.push(t[i].clone());
        }
        i += 1;
    }
    code
}

/// Whether an attribute is `#[tauri::command]`, and if so whether it is
/// plain. One with arguments can rename the command or its keys.
fn command_attribute(attr: &[RustTok]) -> Option<bool> {
    let word = |t: &RustTok, w: &str| matches!(t, RustTok::Ident(x) if x == w);
    let rest = match attr {
        [tauri, RustTok::Punct(':'), RustTok::Punct(':'), command, rest @ ..]
            if word(tauri, "tauri") && word(command, "command") =>
        {
            rest
        }
        [command, rest @ ..] if word(command, "command") => rest,
        _ => return None,
    };
    Some(rest.is_empty())
}

/// The parameter types Tauri hands a command itself. They read no key
/// from the page.
const INJECTED: &[&str] = &["State", "AppHandle", "Window", "WebviewWindow", "Webview"];

/// A key a command reads from the arguments the page passes.
#[derive(Clone, Debug, PartialEq)]
struct Param {
    /// The parameter's name in lower camel case, as Tauri reads it.
    key: String,
    /// Whether the page may leave it out, for an `Option`.
    optional: bool,
}

/// A parameter name as the key Tauri reads, `preset_id` as `presetId`.
fn camel(name: &str) -> String {
    let mut key = String::new();
    for word in name.split('_').filter(|w| !w.is_empty()) {
        let mut chars = word.chars();
        if key.is_empty() {
            key.extend(chars.flat_map(char::to_lowercase));
        } else if let Some(first) = chars.next() {
            key.extend(first.to_uppercase());
            key.extend(chars.flat_map(char::to_lowercase));
        }
    }
    key
}

/// The index of the `>` that closes the `<` at `open`. A `->` inside is
/// no bracket.
fn angle_close(t: &[RustTok], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, tok) in t.iter().enumerate().skip(open) {
        match tok {
            RustTok::Punct('<') => depth += 1,
            RustTok::Punct('>') if t[i - 1] != RustTok::Punct('-') => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// The parts of `t` between commas outside any bracket.
fn split_top(t: &[RustTok]) -> Vec<&[RustTok]> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0usize, 0usize);
    for (i, tok) in t.iter().enumerate() {
        match tok {
            RustTok::Punct('(' | '[' | '{' | '<') => depth += 1,
            RustTok::Punct('>') if i > 0 && t[i - 1] == RustTok::Punct('-') => {}
            RustTok::Punct(')' | ']' | '}' | '>') => depth = depth.saturating_sub(1),
            RustTok::Punct(',') if depth == 0 => {
                parts.push(&t[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&t[start..]);
    parts
}

/// The name of the command fn that starts at `at`, and the keys it reads.
fn command_fn(t: &[RustTok], mut at: usize) -> Result<(String, Vec<Param>), String> {
    let word = |i: usize, w: &str| matches!(t.get(i), Some(RustTok::Ident(x)) if x == w);
    if word(at, "pub") {
        at += 1;
        if t.get(at) == Some(&RustTok::Punct('(')) {
            at = matching(t, at).ok_or("a pub( that never closes")? + 1;
        }
    }
    while word(at, "async") || word(at, "unsafe") || word(at, "const") {
        at += 1;
    }
    let name = match t.get(at + 1) {
        Some(RustTok::Ident(name)) if word(at, "fn") => name.clone(),
        _ => return Err("a #[tauri::command] on something other than a fn".into()),
    };
    at += 2;
    if t.get(at) == Some(&RustTok::Punct('<')) {
        at = angle_close(t, at).ok_or(format!("{name} has generics that never close"))? + 1;
    }
    if t.get(at) != Some(&RustTok::Punct('(')) {
        return Err(format!(
            "{name} has parameters the contract test cannot find"
        ));
    }
    let close = matching(t, at).ok_or(format!("the parameters of {name} never close"))?;
    let mut params = Vec::new();
    for mut param in split_top(&t[at + 1..close]) {
        while let Some((next, _, _)) = attribute(param, 0) {
            param = &param[next..];
        }
        if param.first() == Some(&RustTok::Ident("mut".into())) {
            param = &param[1..];
        }
        match param {
            [] => {}
            [RustTok::Ident(key), RustTok::Punct(':'), ty @ ..]
                if ty.first() != Some(&RustTok::Punct(':')) =>
            {
                let before = ty
                    .iter()
                    .position(|t| *t == RustTok::Punct('<'))
                    .unwrap_or(ty.len());
                let head = ty[..before].iter().rev().find_map(|t| match t {
                    RustTok::Ident(w) => Some(w.as_str()),
                    _ => None,
                });
                if head.is_some_and(|h| INJECTED.contains(&h)) {
                    continue;
                }
                params.push(Param {
                    key: camel(key),
                    optional: head == Some("Option"),
                });
            }
            _ => {
                return Err(format!(
                    "{name} has a parameter the contract test cannot read, {param:?}"
                ))
            }
        }
    }
    Ok((name, params))
}

/// The file a module declared in `parent` lives in.
fn module_file(parent: &Path, m: &ModDecl) -> PathBuf {
    let dir = parent.parent().unwrap();
    if let Some(path) = &m.path {
        return dir.join(path);
    }
    let stem = parent.file_stem().unwrap().to_string_lossy();
    let base = match stem.as_ref() {
        "lib" | "main" | "mod" => dir.to_path_buf(),
        _ => dir.join(stem.as_ref()),
    };
    let flat = base.join(format!("{}.rs", m.name));
    if flat.exists() {
        flat
    } else {
        base.join(&m.name).join("mod.rs")
    }
}

/// The commands `generate_handler!` registers in ipc.rs.
fn registered(t: &[RustTok]) -> Result<BTreeSet<String>, String> {
    let start = t
        .windows(3)
        .position(|w| {
            w[0] == RustTok::Ident("generate_handler".into())
                && w[1] == RustTok::Punct('!')
                && w[2] == RustTok::Punct('[')
        })
        .ok_or("ipc.rs has no generate_handler! the contract test can find")?;
    let open = start + 2;
    let close = matching(t, open).ok_or("generate_handler! never closes")?;
    let mut commands = BTreeSet::new();
    for entry in t[open + 1..close].split(|tok| *tok == RustTok::Punct(',')) {
        match entry {
            [] => {}
            [.., RustTok::Ident(name)]
                if entry
                    .iter()
                    .all(|tok| matches!(tok, RustTok::Ident(_) | RustTok::Punct(':'))) =>
            {
                commands.insert(name.clone());
            }
            _ => {
                return Err(format!(
                    "generate_handler! holds an entry the contract test cannot read, {entry:?}"
                ))
            }
        }
    }
    Ok(commands)
}

/// Each `const NAME: &str = "scheme://path";` in Rust code, as an event
/// constant of [`APP_EVENTS`].
fn app_event_constants(t: &[RustTok]) -> Vec<EventConstant> {
    let word = |i: usize, w: &str| matches!(t.get(i), Some(RustTok::Ident(x)) if x == w);
    let mut found = Vec::new();
    for i in 0..t.len() {
        let (true, Some(RustTok::Ident(ident)), Some(RustTok::Punct(':'))) =
            (word(i, "const"), t.get(i + 1), t.get(i + 2))
        else {
            continue;
        };
        // Past `&str` or `&'static str`, whose lifetime lexes as a word.
        let mut j = i + 3;
        while t.get(j) == Some(&RustTok::Punct('&')) || word(j, "static") || word(j, "str") {
            j += 1;
        }
        if let (Some(RustTok::Punct('=')), Some(RustTok::Str(name)), Some(RustTok::Punct(';'))) =
            (t.get(j), t.get(j + 1), t.get(j + 2))
        {
            if name.contains("://") {
                found.push(EventConstant {
                    file: APP_EVENTS,
                    ident: ident.clone(),
                    name: name.clone(),
                });
            }
        }
    }
    found
}

struct App {
    commands: BTreeMap<String, Option<Vec<Param>>>,
    names: BTreeMap<String, BTreeSet<String>>,
    events: Vec<EventConstant>,
    problems: Vec<String>,
}

/// The commands the app registers and the event names its code holds,
/// read from every module the app builds, from lib.rs and main.rs down.
fn read_app() -> App {
    let src = repo().join("src-tauri").join("src");
    let mut queue = vec![(src.join("lib.rs"), false), (src.join("main.rs"), false)];
    let mut seen = BTreeSet::new();
    let mut defined: BTreeMap<String, Vec<Vec<Param>>> = BTreeMap::new();
    let mut registered_names = BTreeSet::new();
    let mut app = App {
        commands: BTreeMap::new(),
        names: BTreeMap::new(),
        events: Vec::new(),
        problems: Vec::new(),
    };
    while let Some((file, test)) = queue.pop() {
        if !seen.insert(file.clone()) {
            continue;
        }
        let rel = relative(&file);
        let text = fs::read_to_string(&file).unwrap_or_else(|e| panic!("{rel} does not open, {e}"));
        let tokens = rust_tokens(&text)
            .unwrap_or_else(|e| panic!("the contract test lost its place in {rel}, {e}"));
        let code = app_code(&tokens);
        for m in &code.mods {
            queue.push((module_file(&file, m), test || m.test));
        }
        if test {
            continue;
        }
        for &(start, plain) in &code.commands {
            match command_fn(&code.tokens, start) {
                Ok((name, _)) if !plain => app.problems.push(format!(
                    "{rel} marks {name} with a #[tauri::command] that has arguments. \
                     The contract test reads only the plain attribute, so teach it these."
                )),
                Ok((name, params)) => defined.entry(name).or_default().push(params),
                Err(e) => app.problems.push(format!("{rel} holds {e}.")),
            }
        }
        for (i, tok) in code.tokens.iter().enumerate() {
            match tok {
                RustTok::Str(s) if s.contains("://") => {
                    app.names.entry(s.clone()).or_default().insert(rel.clone());
                }
                RustTok::Ident(w)
                    if matches!(w.as_str(), "listen" | "listen_any" | "once" | "once_any")
                        && i > 0
                        && code.tokens[i - 1] == RustTok::Punct('.')
                        && code.tokens.get(i + 1) == Some(&RustTok::Punct('(')) =>
                {
                    app.problems.push(format!(
                        "{rel} hears an event with .{w}. The contract test counts every \
                         event name in the app as one it sends, so teach it the names the \
                         app hears."
                    ));
                }
                _ => {}
            }
        }
        if rel == "src-tauri/src/ipc.rs" {
            match registered(&code.tokens) {
                Ok(commands) => registered_names = commands,
                Err(e) => app.problems.push(e),
            }
        }
        if rel == APP_EVENTS {
            app.events = app_event_constants(&code.tokens);
        }
    }
    // A command defined once per platform must read the same keys on each.
    for name in registered_names {
        let params = match defined.get(&name).map(Vec::as_slice) {
            Some([first, rest @ ..]) if rest.iter().all(|p| p == first) => Some(first.clone()),
            Some([_, ..]) => {
                app.problems.push(format!(
                    "{name} has #[tauri::command] fns that read different keys."
                ));
                None
            }
            _ => {
                app.problems.push(format!(
                    "generate_handler! registers {name}, and the contract test finds no \
                     #[tauri::command] fn by that name."
                ));
                None
            }
        };
        app.commands.insert(name, params);
    }
    for file in files_under(&src) {
        if file.extension().is_some_and(|e| e == "rs") && !seen.contains(&file) {
            app.problems.push(format!(
                "{} is no module of the app or of its tests, so the contract test \
                 cannot tell what it sends.",
                relative(&file)
            ));
        }
    }
    app
}

// Files.

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// A path from the repo root, with forward slashes.
fn relative(path: &Path) -> String {
    path.strip_prefix(repo())
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Every file under `dir`, in a stable order.
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut dirs = vec![dir.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in
            fs::read_dir(&dir).unwrap_or_else(|e| panic!("{} does not open, {e}", dir.display()))
        {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}
