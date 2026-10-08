//! The values a template reads.
//!
//! [`CATALOG`] names every field Vosh knows, with its label, kind, group,
//! sources, a sample for previews and the words the picker searches.
//! [`Vars`] holds what feeds them in a session: script values, the last
//! recognized prompt (the capture), the GMCP snapshot, and whether the
//! Forsaken Lands rules hold. `Resolver` answers the renderer with each
//! field's state. [`Values`] is what the renderer asks, of the resolver,
//! the samples and a preview's overrides alike.
//!
//! Sources, first fresh one wins.
//!
//! 1. Script values from `mud.set_prompt_var`. A value for a name the
//!    capture or GMCP also supplies lasts for the pulse it was set in. A
//!    name neither supplies keeps its value, Vosh's own among them.
//! 2. The capture, replaced whole by each recognized prompt, and fresh
//!    while no pulse has started since.
//! 3. GMCP, the latest packet per package.
//! 4. Vosh itself, the tick, your target, the clock and the profile.
//!
//! A value the game hides is Hidden whatever the sources hold, and Vosh
//! never fills it from another one. [`Hidden`] is worked out from the
//! latest packets and what the prompt read this pulse, through the
//! capture or a prompt trigger's script values, never stored.

pub mod format;
pub mod gmcp;
pub mod overrides;

mod catalog;
mod hidden;
mod resolver;
mod samples;

pub use catalog::{entry, is_sourced, known, FormatId, Group, Source, CATALOG};
pub use hidden::Hidden;
pub use samples::Samples;

pub(crate) use catalog::{entry_for, feeds, Entry, Kind, Pair};
pub(crate) use resolver::Resolver;
pub(crate) use samples::value_of;

use std::collections::BTreeMap;

use chrono::{DateTime, FixedOffset, NaiveDateTime};
use serde_json::Value as Json;

use crate::design::FieldRef;
use crate::values::format::Resolved;
use crate::values::gmcp::{Observed, Snapshot, ROOM_INFO, ROOM_WEATHER};

use catalog::capture_keys;

// ---------------------------------------------------------------------
// What the renderer asks
// ---------------------------------------------------------------------

/// What the renderer asks about each field.
pub trait Values {
    fn resolve(&self, field: &FieldRef) -> Resolved;

    /// The field's label, drawn as a placeholder and by the `on` and `off`
    /// formats.
    fn label(&self, field: &FieldRef) -> String {
        field.to_string()
    }
}

/// The names a prompt var's max goes by, in the order the first renderer
/// tried them: `mhp`, `hp_max`, `max_hp`, `maxhp`.
///
/// `is_max_of` in design/pieces.rs knows the same four spellings, in
/// another order, to find a current and max piece. The order matters only
/// here, where the first spelling a session holds a value for wins, so it
/// stays the first renderer's, since a merge would change what the live
/// prompt draws.
pub(crate) fn max_spellings(name: &str) -> [String; 4] {
    [
        format!("m{name}"),
        format!("{name}_max"),
        format!("max_{name}"),
        format!("max{name}"),
    ]
}

// ---------------------------------------------------------------------
// The session's variables
// ---------------------------------------------------------------------

/// The last recognized prompt. Every group the matched shape has is a
/// key. A group that printed nothing, such as `%p` under lamented tears
/// or the `(Wizi N)` prefix when you are visible, is an empty string. A
/// name the shape does not have is no key at all.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Capture {
    pub values: BTreeMap<String, String>,
    /// The block as the game sent it, colors included, for `%{raw}`.
    pub raw: Option<String>,
}

/// A script value and the pulse it was set in. None for a name only
/// scripts supply, which never goes stale.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Scripted {
    value: String,
    pulse: Option<u64>,
}

/// What Vosh itself supplies.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientValues {
    pub tick: Option<Tick>,
    /// Your target, the client one (`#target`), not Char.Combat's.
    pub target: Option<String>,
    /// The active profile's display name.
    pub profile: Option<String>,
    /// The clock, None for the local time now.
    pub now: Option<NaiveDateTime>,
    /// The profile's tracked affects, for `missing`.
    pub tracked: Vec<String>,
}

/// The tick timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tick {
    /// Whole seconds until the next tick.
    pub remaining: i64,
    /// Seconds between ticks, when known.
    pub interval: Option<i64>,
    /// Whole seconds since the tick last turned, when known. They keep
    /// counting past the interval while a tick is late, where
    /// `remaining` stays at 0.
    pub since: Option<i64>,
}

/// The seconds since the tick for a count of seconds left, when the
/// interval says: the interval less what is left, never below 0.
fn since_of(secs: i64, interval: Option<i64>) -> Option<i64> {
    interval.map(|i| (i - secs).max(0))
}

/// True when the Forsaken Lands rules hold. They hold when the host
/// is The Forsaken Lands or the active capture reads Aabahran's codes.
pub(crate) fn forsaken_lands(known_host: bool, aabahran_capture: bool) -> bool {
    known_host || aabahran_capture
}

/// The variables of one session.
#[derive(Debug, Clone, Default)]
pub struct Vars {
    forsaken: bool,
    gmcp: Snapshot,
    capture: Option<(Capture, u64)>,
    script: BTreeMap<String, Scripted>,
    hidden: Hidden,
    emitted: Hidden,
    disagreements: u64,
}

impl Vars {
    /// Empty variables. `forsaken` is whether the Forsaken Lands rules
    /// hold. Test only. The tests in `tests/` reach it through the
    /// `testkit` feature.
    #[cfg(any(test, feature = "testkit"))]
    pub fn new(forsaken: bool) -> Self {
        Self {
            forsaken,
            ..Self::default()
        }
    }

    pub(crate) fn forsaken(&self) -> bool {
        self.forsaken
    }

    /// Change whether the Forsaken Lands rules hold, as a profile switch
    /// or a new capture kind can.
    pub(crate) fn set_forsaken(&mut self, forsaken: bool) {
        self.forsaken = forsaken;
        self.recompute();
    }

    pub fn gmcp(&self) -> &Snapshot {
        &self.gmcp
    }

    /// The server is the new build: the Forsaken Lands rules hold and a
    /// Char.Prompt has come since the socket connected.
    pub fn new_build(&self) -> bool {
        self.forsaken && self.gmcp.prompt_seen()
    }

    /// Keep a GMCP packet.
    pub fn observe(&mut self, package: &str, data: Json, at: DateTime<FixedOffset>) -> Observed {
        let observed = self.gmcp.observe(package, data, at);
        self.recompute();
        observed
    }

    /// Note one of your own sends. It starts a pulse on a server that has
    /// sent no Char.Vitals.
    pub fn on_send(&mut self) -> bool {
        let pulse = self.gmcp.on_send();
        if pulse {
            self.recompute();
        }
        pulse
    }

    /// Take a recognized prompt's values. They are fresh until the next
    /// pulse starts. Returns the names whose value disagrees with GMCP,
    /// for the `vosh::prompt` log, and counts them.
    pub fn capture(&mut self, capture: Capture) -> Vec<&'static str> {
        self.capture = Some((capture, self.gmcp.pulse()));
        self.recompute();
        let disagree = self.disagreements_now();
        self.disagreements += disagree.len() as u64;
        disagree
    }

    /// How many disagreements between a fresh capture and GMCP this
    /// session. Test only. The tests in `tests/` reach it through the
    /// `testkit` feature.
    #[cfg(any(test, feature = "testkit"))]
    pub fn disagreements(&self) -> u64 {
        self.disagreements
    }

    /// A value from `mud.set_prompt_var`. A name a capture or GMCP also
    /// supplies lasts until the next pulse. A prompt trigger reads the
    /// prompt this way, so the hidden state follows it.
    pub fn set_script(&mut self, name: &str, value: &str) {
        let pulse = is_sourced(name).then(|| self.gmcp.pulse());
        self.script.insert(
            name.to_string(),
            Scripted {
                value: value.to_string(),
                pulse,
            },
        );
        self.recompute();
    }

    /// Clear a script value.
    pub fn remove_script(&mut self, name: &str) -> bool {
        let removed = self.script.remove(name).is_some();
        if removed {
            self.recompute();
        }
        removed
    }

    /// A profile switch keeps the GMCP snapshot and the new build sign,
    /// and clears the capture and script values.
    pub fn switch_profile(&mut self, forsaken: bool) {
        self.capture = None;
        self.script.clear();
        self.forsaken = forsaken;
        self.recompute();
    }

    /// A disconnect clears everything but the rules and what was last
    /// emitted, so the next [`Vars::take_hidden_change`] clears the panes.
    pub fn disconnect(&mut self) {
        self.capture = None;
        self.script.clear();
        self.gmcp.clear();
        self.recompute();
    }

    /// What the game hides right now. Test only. The tests in `tests/`
    /// reach it through the `testkit` feature.
    #[cfg(any(test, feature = "testkit"))]
    pub fn hidden(&self) -> Hidden {
        self.hidden
    }

    /// The hidden state when it changed since the last call, for one
    /// `session://hidden` per socket read.
    pub fn take_hidden_change(&mut self) -> Option<Hidden> {
        (self.hidden != self.emitted).then(|| {
            self.emitted = self.hidden;
            self.hidden
        })
    }

    /// The hidden state the last [`Vars::take_hidden_change`] reported,
    /// which every open window has heard. A window that opens or reloads
    /// later reads this, since each change is reported once.
    pub fn reported(&self) -> Hidden {
        self.emitted
    }

    /// The fresh capture and script values for `session://prompt-vars`, a
    /// hidden one as `?`. Stale values are left out.
    pub fn prompt_vars(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        if let Some(capture) = self.fresh_capture() {
            for (name, value) in &capture.values {
                out.insert(name.clone(), value.clone());
            }
        }
        for (name, scripted) in &self.script {
            if self.script_fresh(scripted) {
                out.insert(name.clone(), scripted.value.clone());
            }
        }
        for (name, value) in &mut out {
            if self.name_hidden(name) {
                *value = "?".to_string();
            }
        }
        out
    }

    /// A resolver over these variables and what Vosh supplies.
    pub fn resolver<'a>(&'a self, client: &'a ClientValues) -> Resolver<'a> {
        Resolver { vars: self, client }
    }

    /// Which source a field reads from now, in the order the resolver
    /// tries them: a fresh script value, the fresh capture, the latest
    /// packet of its package, then Vosh. None when none has it yet.
    pub(crate) fn source(&self, e: &Entry, client: &ClientValues) -> Option<Source> {
        let keys = capture_keys(e);
        let fresh_script = |key: &str| self.script.get(key).is_some_and(|s| self.script_fresh(s));
        if keys.iter().any(|k| fresh_script(k)) {
            return Some(Source::Script);
        }
        let captured = self.fresh_capture().is_some_and(|c| {
            keys.iter()
                .any(|k| c.values.get(*k).is_some_and(|v| !v.trim().is_empty()))
        });
        if captured {
            return Some(Source::Capture);
        }
        let sent = match e.name {
            "region" => self.gmcp.has(ROOM_WEATHER) || self.gmcp.has(ROOM_INFO),
            "exits" => self.new_build() && self.gmcp.has(ROOM_INFO),
            _ => e.package.is_some_and(|p| self.gmcp.has(p)),
        };
        if sent {
            return Some(Source::Gmcp);
        }
        let vosh_has = match e.name {
            "tick" => client.tick.is_some(),
            "time" | "date" => true,
            "target" => client
                .target
                .as_deref()
                .is_some_and(|t| !t.trim().is_empty()),
            "profile" => client
                .profile
                .as_deref()
                .is_some_and(|p| !p.trim().is_empty()),
            _ => false,
        };
        vosh_has.then_some(Source::Vosh)
    }

    /// The names a script set that no catalog field has, each with a
    /// fresh value, for the picker's Your scripts group.
    pub(crate) fn script_names(&self) -> Vec<&str> {
        self.script
            .iter()
            .filter(|(name, scripted)| self.script_fresh(scripted) && !known(name))
            .map(|(name, _)| name.as_str())
            .collect()
    }

    fn recompute(&mut self) {
        self.hidden = self.work_out_hidden();
    }

    fn fresh_capture(&self) -> Option<&Capture> {
        self.capture
            .as_ref()
            .filter(|(_, pulse)| *pulse == self.gmcp.pulse())
            .map(|(capture, _)| capture)
    }

    fn script_fresh(&self, scripted: &Scripted) -> bool {
        scripted.pulse.map_or(true, |p| p == self.gmcp.pulse())
    }

    /// A fresh prompt value: a script value, else the capture's. An empty
    /// string means the prompt printed nothing there.
    fn var(&self, name: &str) -> Option<&str> {
        if let Some(scripted) = self.script.get(name).filter(|s| self.script_fresh(s)) {
            return Some(&scripted.value);
        }
        self.fresh_capture()
            .and_then(|c| c.values.get(name))
            .map(String::as_str)
    }

    /// A pair's fresh max from the prompt, under the first of its
    /// spellings that has one, the order the resolver reads them in.
    fn max_var(&self, pair: Pair) -> Option<&str> {
        let aliases: &[&str] = entry(pair.max()).map_or(&[], |e| e.aliases);
        std::iter::once(pair.max())
            .chain(aliases.iter().copied())
            .find_map(|name| self.var(name))
    }

    /// True when a script or any capture, stale or not, has had the name,
    /// so it is known even without a fresh value.
    fn known_var(&self, name: &str) -> bool {
        self.script.contains_key(name)
            || self
                .capture
                .as_ref()
                .is_some_and(|(c, _)| c.values.contains_key(name))
    }
}
