//! Alias engine with positional substitution and a recursion-depth guard.
//!
//! An alias matches the first whitespace-separated word of a command. The
//! expansion may contain:
//!
//! - `%0` — the entire arg tail.
//! - `%1` through `%9` — the Nth whitespace-separated word.
//! - `%1-` through `%9-` — word N and the rest of the input, preserving the
//!   original whitespace between words. `%1-` is equivalent to `%0`. `%N-`
//!   with fewer than N words present expands to empty.
//! - `%%` — a literal `%`.
//!
//! Multiple commands separated by `;` in an expansion are split and each is
//! re-fed through the engine, bounded by a maximum recursion depth.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::groups::{clean_group, compare_groups, GroupSwitch};
use crate::revision::next_revision;
use crate::split::split_commands;
use crate::stops::{split_stop_id, stop_id, StopKey, Stops};
use crate::ScriptCall;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alias {
    pub name: String,
    pub expansion: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Optional group tag. Aliases sharing a group can be turned
    /// on or off together via `AliasStore::set_group_enabled` without
    /// losing their individual `enabled` flags. `None` means the
    /// alias is ungrouped and only its own `enabled` controls it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Optional Lua body. When `Some`, the alias evaluates the body
    /// via the session's `ScriptEngine` with the words typed after its
    /// name bound to a local `captures` table, and `expansion` is
    /// ignored. Lets aliases branch / loop / call `mud.send` rather
    /// than just expand a template. `None` keeps the legacy
    /// template-substitution path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
}

fn default_enabled() -> bool {
    true
}

impl Alias {
    pub fn new(name: impl Into<String>, expansion: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            expansion: expansion.into(),
            enabled: true,
            group: None,
            script: None,
        }
    }

    /// Builder-style setter for the optional Lua script body. Only tests
    /// call it, the app's tests through the `testkit` feature.
    #[cfg(any(test, feature = "testkit"))]
    #[must_use]
    pub fn with_script(mut self, script: impl Into<String>) -> Self {
        self.script = Some(script.into());
        self
    }

    /// Builder-style setter for the optional group tag. Test only, like
    /// `with_script`.
    #[cfg(any(test, feature = "testkit"))]
    #[must_use]
    pub fn with_group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }
}

/// The aliases plugins make. Each lasts for the session and belongs to
/// the plugin that made it, and Vosh never saves one, so it lives apart
/// from the [`AliasStore`] the profile file holds. While one lasts it
/// takes the place of a saved alias of its name. Two plugins may each
/// make an alias of one name, and the one made last expands, so when it
/// goes the other expands again.
#[derive(Debug, Clone, Default)]
pub struct PluginAliases {
    /// Each alias with the plugin that made it, in the order they were
    /// made.
    aliases: Vec<(String, Alias)>,
    /// See [`PluginAliases::revision`].
    revision: u64,
}

impl PluginAliases {
    /// Moves each time a plugin makes or drops an alias, as
    /// [`AliasStore::revision`] does for yours.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Make the alias `name` for `plugin`, in place of one of that name
    /// the same plugin made.
    pub fn set(&mut self, plugin: &str, name: impl Into<String>, expansion: impl Into<String>) {
        let alias = Alias::new(name, expansion);
        self.aliases
            .retain(|(by, old)| !(by == plugin && old.name == alias.name));
        self.aliases.push((plugin.to_string(), alias));
        self.revision = next_revision();
    }

    /// Remove the alias `name` when `plugin` made it. True when it went.
    pub fn remove(&mut self, plugin: &str, name: &str) -> bool {
        let before = self.aliases.len();
        self.aliases
            .retain(|(by, alias)| !(by == plugin && alias.name == name));
        let removed = self.aliases.len() != before;
        if removed {
            self.revision = next_revision();
        }
        removed
    }

    /// Remove every alias `plugin` made.
    pub fn remove_plugin(&mut self, plugin: &str) {
        let before = self.aliases.len();
        self.aliases.retain(|(by, _)| by != plugin);
        if self.aliases.len() != before {
            self.revision = next_revision();
        }
    }

    /// Every alias by name, with the plugin that made it.
    pub fn list(&self) -> Vec<(&str, &Alias)> {
        let mut out: Vec<(&str, &Alias)> = self
            .aliases
            .iter()
            .map(|(by, alias)| (by.as_str(), alias))
            .collect();
        out.sort_by(|a, b| a.1.name.cmp(&b.1.name).then_with(|| a.0.cmp(b.0)));
        out
    }

    /// The alias of `name` that expands: the one a plugin made last.
    fn get(&self, name: &str) -> Option<&Alias> {
        self.aliases
            .iter()
            .rev()
            .find(|(_, alias)| alias.name == name)
            .map(|(_, alias)| alias)
    }
}

/// One step of an expanded line. A line expands to its steps in the
/// order you typed them, so a script alias runs between the commands
/// around it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpandStep {
    /// A command to send to the game.
    Command(String),
    /// The Lua body of a script alias, to run at this point in the line.
    Script(ScriptCall),
}

#[derive(Debug, Error)]
pub enum ExpandError {
    /// The aliases ran `depth` deep from the line you typed. `chain`
    /// labels each alias expanded on the way down, the one you typed
    /// first, so the message can name the alias that calls itself.
    #[error("{}", recursion_text(.chain, *.depth))]
    RecursionLimit { depth: usize, chain: Vec<String> },
}

/// What Vosh prints when an alias runs too deep: the alias that calls
/// itself, and through which others when it goes around them first.
fn recursion_text(chain: &[String], depth: usize) -> String {
    for (i, label) in chain.iter().enumerate() {
        let Some(again) = chain[i + 1..].iter().position(|l| l == label) else {
            continue;
        };
        let mut between: Vec<&str> = Vec::new();
        for other in &chain[i + 1..i + 1 + again] {
            if !between.contains(&other.as_str()) {
                between.push(other);
            }
        }
        return if between.is_empty() {
            format!("alias {label} calls itself, so Vosh stopped it after {depth} steps")
        } else {
            format!(
                "alias {label} calls itself through {}, so Vosh stopped it after {depth} steps",
                between.join(" and ")
            )
        };
    }
    match chain.first() {
        Some(first) => format!("alias {first} runs aliases {depth} deep, so Vosh stopped it there"),
        None => format!("aliases ran {depth} deep, so Vosh stopped them"),
    }
}

/// Default cap on alias recursion depth. Matches the `TinTin++` default.
pub const DEFAULT_MAX_DEPTH: usize = 16;

/// `alias` as the store keeps it, its name and group trimmed and an
/// empty group as none. A name you typed with a space before or after it
/// would never match the first word of a line otherwise.
fn cleaned(mut alias: Alias) -> Alias {
    let name = alias.name.trim();
    if name.len() != alias.name.len() {
        alias.name = name.to_string();
    }
    alias.group = clean_group(alias.group.as_deref()).map(str::to_string);
    alias
}

impl Alias {
    /// What the store knows this alias by: its group, None for no
    /// group, and its name, both trimmed.
    pub fn id(&self) -> (Option<&str>, &str) {
        (clean_group(self.group.as_deref()), self.name.trim())
    }

    /// How a message names this alias: its name, and its group after
    /// it when it has one, like `ds in Tolliver`.
    pub fn label(&self) -> String {
        match &self.group {
            Some(group) => format!("{} in {group}", self.name),
            None => self.name.clone(),
        }
    }
}

/// Your aliases. An alias is known by its group and its name together,
/// so two groups may each hold an alias of one name, like one group of
/// aliases for each character you play. When more than one of them
/// could expand, the one whose group Settings lists first does: an alias
/// in no group, then the groups in [`compare_groups`] order.
#[derive(Debug, Clone)]
pub struct AliasStore {
    /// Every alias by name. The aliases of one name sit in the order
    /// Settings lists their groups, one for each group.
    aliases: HashMap<String, Vec<Alias>>,
    max_depth: usize,
    /// The groups you turned off. An alias in an off group passes
    /// through whatever its own `enabled` flag says.
    groups: GroupSwitch,
    /// See [`AliasStore::revision`].
    revision: u64,
    /// The aliases whose Lua Vosh stopped, each under the key of the
    /// session it stopped in, by [`stop_id`]. A stopped alias passes
    /// through there, as an off one does, until you save it again.
    stopped: Stops,
}

impl Default for AliasStore {
    fn default() -> Self {
        Self::new()
    }
}

impl AliasStore {
    pub fn new() -> Self {
        Self {
            aliases: HashMap::new(),
            max_depth: DEFAULT_MAX_DEPTH,
            groups: GroupSwitch::default(),
            revision: 0,
            stopped: Stops::default(),
        }
    }

    /// Moves each time an alias is added, replaced, or removed, or Vosh
    /// stops one, so a caller can tell whether the list or what expands
    /// changed across a step without comparing it. Turning a group on or
    /// off leaves it alone, since the group toggles count those.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// True when the named group is effectively enabled. Returns true
    /// for an empty / missing group name (ungrouped aliases never
    /// participate in the group-disable mechanism).
    pub fn is_group_enabled(&self, group: &str) -> bool {
        self.groups.is_enabled(group)
    }

    /// Toggle a whole group. Calling with `enabled = true` removes
    /// the group from the disabled set; with `false` adds it. Returns
    /// whether the group turned.
    pub fn set_group_enabled(&mut self, group: &str, enabled: bool) -> bool {
        self.groups.set_enabled(group, enabled)
    }

    /// Sorted list of every group name referenced by at least one
    /// alias, paired with whether that group is currently enabled.
    /// Used by the Settings UI to render the per-group toggle row.
    pub fn groups(&self) -> Vec<(String, bool)> {
        self.groups.list(
            self.aliases
                .values()
                .flatten()
                .filter_map(|a| a.group.as_deref()),
        )
    }

    /// Persistence accessor for the disabled-groups set. Returns the
    /// names that should be saved alongside the alias list.
    pub fn disabled_groups(&self) -> Vec<String> {
        self.groups.disabled()
    }

    /// Persistence inverse of `disabled_groups()`. Replaces the
    /// current disabled-set with the supplied names.
    pub fn set_disabled_groups<I, S>(&mut self, groups: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.groups.set_disabled(groups);
    }

    /// Lowers the recursion cap so a test can reach it in a few steps.
    #[cfg(test)]
    #[must_use]
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    /// Insert an alias, or replace the one of its name in its group.
    /// The name and the group are trimmed first. Saving an alias Vosh
    /// stopped turns it back on everywhere.
    pub fn set(&mut self, alias: Alias) {
        let alias = cleaned(alias);
        self.stopped
            .clear(&stop_id(alias.group.as_deref(), &alias.name));
        let list = self.aliases.entry(alias.name.clone()).or_default();
        match list.binary_search_by(|a| compare_groups(a.group.as_deref(), alias.group.as_deref()))
        {
            Ok(at) => list[at] = alias,
            Err(at) => list.insert(at, alias),
        }
        self.revision = next_revision();
    }

    /// Remove the alias `name` of `group`, None for the one in no group.
    /// True when it went.
    pub fn remove(&mut self, group: Option<&str>, name: &str) -> bool {
        let group = clean_group(group);
        let name = name.trim();
        self.stopped.clear(&stop_id(group, name));
        let Some(list) = self.aliases.get_mut(name) else {
            return false;
        };
        let before = list.len();
        list.retain(|a| a.group.as_deref() != group);
        let removed = list.len() != before;
        if list.is_empty() {
            self.aliases.remove(name);
        }
        if removed {
            self.revision = next_revision();
        }
        removed
    }

    /// The alias `name` of `group`, None for the one in no group.
    pub fn get_in(&self, group: Option<&str>, name: &str) -> Option<&Alias> {
        let group = clean_group(group);
        self.named(name)
            .iter()
            .find(|a| a.group.as_deref() == group)
    }

    /// The first alias of `name` Settings lists, whatever its group.
    pub fn get(&self, name: &str) -> Option<&Alias> {
        self.named(name).first()
    }

    /// Every alias of `name`, in the order Settings lists their groups,
    /// which is the order they win in.
    pub fn named(&self, name: &str) -> &[Alias] {
        self.aliases.get(name.trim()).map_or(&[], Vec::as_slice)
    }

    /// The alias of `name` that expands when you type it in the session
    /// `key` names: the first Settings lists that is on, in a group that
    /// is on, and not stopped there.
    pub fn winner(&self, name: &str, key: StopKey) -> Option<&Alias> {
        self.named(name).iter().find(|a| self.fires(a, key))
    }

    /// The alias a bare name means to `#alias` and `mud.alias`: the one
    /// that expands in the session `key` names, else the first Settings
    /// lists.
    pub fn chosen(&self, name: &str, key: StopKey) -> Option<&Alias> {
        self.winner(name, key).or_else(|| self.get(name))
    }

    /// Turn the alias `name` of `group` off under `key`, after Vosh
    /// stopped its Lua there. It stays off there until you save it again.
    pub fn stop(&mut self, group: Option<&str>, name: &str, key: StopKey) {
        if self.get_in(group, name).is_some()
            && self.stopped.stop(&stop_id(clean_group(group), name), key)
        {
            self.revision = next_revision();
        }
    }

    /// True while Vosh holds the alias `name` of `group` off under `key`
    /// after a stop.
    pub fn is_stopped(&self, group: Option<&str>, name: &str, key: StopKey) -> bool {
        self.stopped
            .contains(&stop_id(clean_group(group), name), key)
    }

    /// Drop every stop under `key`, as the session it names closes.
    pub fn forget_stops(&mut self, key: StopKey) {
        self.stopped.forget(key);
    }

    /// Take the stops of `old`, each under its key, for each alias this
    /// store holds as `old` held it. The Settings editor saves the whole
    /// list at once, so only the alias you changed comes back on.
    pub fn keep_stops_from(&mut self, old: &AliasStore) {
        self.stopped = old.stopped.kept(|id| {
            let (group, name) = split_stop_id(id);
            self.get_in(group, name) == old.get_in(group, name)
        });
    }

    /// True when the saved alias `alias` expands under `key`: it is on,
    /// its group is on, and Vosh has not stopped its Lua there.
    fn fires(&self, alias: &Alias, key: StopKey) -> bool {
        alias.enabled
            && self.groups.allows(alias.group.as_deref())
            && !self.is_stopped(alias.group.as_deref(), &alias.name, key)
    }

    /// The names that expand if you press Enter now in the session
    /// `key` names, sorted and each once: the saved aliases that fire
    /// there and every alias in `plugins`. Names match case sensitively,
    /// as expansion matches them.
    pub fn live_names(&self, plugins: &PluginAliases, key: StopKey) -> Vec<String> {
        let saved = self
            .aliases
            .values()
            .flatten()
            .filter(|a| self.fires(a, key));
        let made = plugins.list().into_iter().map(|(_, alias)| alias);
        let mut names: Vec<String> = saved.chain(made).map(|a| a.name.clone()).collect();
        names.sort();
        names.dedup();
        names
    }

    /// Every alias, sorted by name, and the aliases of one name in the
    /// order Settings lists their groups.
    pub fn list(&self) -> Vec<&Alias> {
        let mut names: Vec<&String> = self.aliases.keys().collect();
        names.sort();
        names
            .into_iter()
            .flat_map(|name| self.aliases[name].iter())
            .collect()
    }

    /// Expand a single command line and return only the resulting send
    /// commands. Script aliases that fire during expansion are discarded.
    /// Test only, since the input path runs script bodies and so calls
    /// [`expand_line_full`](Self::expand_line_full).
    #[cfg(test)]
    pub fn expand_line(&self, line: &str, key: StopKey) -> Result<Vec<String>, ExpandError> {
        Ok(self
            .expand_line_full(line, &PluginAliases::default(), key)?
            .into_iter()
            .filter_map(|step| match step {
                ExpandStep::Command(command) => Some(command),
                ExpandStep::Script(_) => None,
            })
            .collect())
    }

    /// Full expansion result: the commands to send and the Lua bodies
    /// script aliases queue, in the order you typed them. The input
    /// pipeline runs each body where it stands, so what a body sends goes
    /// out between the commands around it. An alias in `plugins` takes
    /// the place of a saved one of its name. A saved alias Vosh stopped
    /// under `key`, the session you typed the line in, passes through.
    pub fn expand_line_full(
        &self,
        line: &str,
        plugins: &PluginAliases,
        key: StopKey,
    ) -> Result<Vec<ExpandStep>, ExpandError> {
        let mut steps = Vec::new();
        let mut chain = Vec::new();
        for raw in split_commands(line) {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            self.expand_into(trimmed, plugins, key, &mut chain, &mut steps)?;
        }
        Ok(steps)
    }

    /// Expand `command` into `out`. `chain` labels the aliases this
    /// command sits inside, the one you typed first, and its length is
    /// the depth.
    fn expand_into(
        &self,
        command: &str,
        plugins: &PluginAliases,
        key: StopKey,
        chain: &mut Vec<String>,
        out: &mut Vec<ExpandStep>,
    ) -> Result<(), ExpandError> {
        let (name, rest) = split_first_word(command);
        // The alias fires only when:
        //   * an entry of that name exists, AND
        //   * its own `enabled` flag is true, AND
        //   * its group is enabled (or it is ungrouped), AND
        //   * Vosh has not stopped its Lua in this session.
        // Disabled groups short-circuit to pass-through so the user
        // can flip whole "Combat" / "Crafting" loadouts off without
        // editing each row. Of the entries of one name that fire, the
        // one whose group Settings lists first wins. An alias a plugin
        // made has none of these and comes first.
        let saved = || self.winner(name, key);
        let Some(alias) = plugins.get(name).or_else(saved) else {
            out.push(ExpandStep::Command(command.to_string()));
            return Ok(());
        };
        if chain.len() >= self.max_depth {
            let mut chain = chain.clone();
            chain.push(alias.label());
            return Err(ExpandError::RecursionLimit {
                depth: self.max_depth,
                chain,
            });
        }

        // Script-bodied aliases bypass template expansion entirely.
        // Lua reads the words after the name as `captures[1]`,
        // `captures[2]`, ..., the words `%1`, `%2`, ... would take.
        if let Some(body) = &alias.script {
            out.push(ExpandStep::Script(ScriptCall {
                source: alias.name.clone(),
                group: alias.group.clone(),
                body: body.clone(),
                captures: rest.split_whitespace().map(str::to_string).collect(),
            }));
            return Ok(());
        }

        let expanded = substitute_params(&alias.expansion, rest);
        chain.push(alias.label());
        for raw in split_commands(&expanded) {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            self.expand_into(trimmed, plugins, key, chain, out)?;
        }
        chain.pop();
        Ok(())
    }
}

fn split_first_word(input: &str) -> (&str, &str) {
    let trimmed = input.trim_start();
    match trimmed.find(char::is_whitespace) {
        Some(idx) => {
            let (head, tail) = trimmed.split_at(idx);
            (head, tail.trim_start())
        }
        None => (trimmed, ""),
    }
}

fn substitute_params(expansion: &str, args: &str) -> String {
    let words: Vec<&str> = args.split_whitespace().collect();
    let mut out = String::with_capacity(expansion.len());
    let mut chars = expansion.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            out.push(ch);
            continue;
        }
        match chars.peek().copied() {
            Some('%') => {
                out.push('%');
                chars.next();
            }
            Some(d @ '0'..='9') => {
                chars.next();
                let idx = (d as u8 - b'0') as usize;
                // Range form `%N-` means "word N through the end of the
                // input, with the original whitespace between words
                // preserved". Detected as a trailing `-` after the digit.
                // Mirrors TinTin's `%1.-9` shorthand without the explicit
                // upper bound.
                if chars.peek().copied() == Some('-') {
                    chars.next();
                    out.push_str(args_from_word(args, idx));
                } else if idx == 0 {
                    out.push_str(args);
                } else if let Some(word) = words.get(idx - 1) {
                    out.push_str(word);
                }
            }
            _ => out.push('%'),
        }
    }
    out
}

/// Return the slice of `args` that starts at the Nth whitespace-separated
/// word (1-based). N=0 returns the full args string. When fewer than N
/// words are present, returns an empty slice.
///
/// Walks the original `args` byte-by-char so the whitespace between words
/// is preserved verbatim — `%2-` on input `"a   b\tc"` yields `"b\tc"`,
/// not `"b c"`. Used by the `%N-` range form so the user can write
/// `tell %1 %2-` and have the message text keep its original spacing.
fn args_from_word(args: &str, n: usize) -> &str {
    if n == 0 {
        return args;
    }
    let mut word_count = 0;
    let mut in_word = false;
    for (i, ch) in args.char_indices() {
        if ch.is_whitespace() {
            in_word = false;
            continue;
        }
        if !in_word {
            in_word = true;
            word_count += 1;
            if word_count == n {
                return &args[i..];
            }
        }
    }
    ""
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The session the lines here are typed in, and another.
    const SESSION: StopKey = StopKey(1);
    const OTHER: StopKey = StopKey(2);

    fn store(entries: &[(&str, &str)]) -> AliasStore {
        let mut s = AliasStore::new();
        for (n, e) in entries {
            s.set(Alias::new(*n, *e));
        }
        s
    }

    /// The commands `line` sends with `plugins` over `store`.
    fn sends(store: &AliasStore, plugins: &PluginAliases, line: &str) -> Vec<String> {
        store
            .expand_line_full(line, plugins, SESSION)
            .unwrap()
            .into_iter()
            .filter_map(|step| match step {
                ExpandStep::Command(command) => Some(command),
                ExpandStep::Script(_) => None,
            })
            .collect()
    }

    #[test]
    fn a_plugin_alias_takes_the_place_of_a_saved_one_while_it_lasts() {
        let saved = store(&[("hl", "cast heal %1"), ("bt", "bash %1")]);
        let mut plugins = PluginAliases::default();
        plugins.set("healer", "hl", "cast 'cure light' %1");
        // It expands through the saved aliases like any other.
        plugins.set("healer", "go", "bt %1;hl %1");
        assert_eq!(
            sends(&saved, &plugins, "go Orla"),
            ["bash Orla", "cast 'cure light' Orla"]
        );
        // It never joins the saved list.
        let names: Vec<&str> = saved.list().iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["bt", "hl"]);
        // Another plugin cannot remove it, and its own plugin can.
        assert!(!plugins.remove("other", "hl"));
        assert!(plugins.remove("healer", "hl"));
        assert_eq!(sends(&saved, &plugins, "hl Orla"), ["cast heal Orla"]);
        plugins.set("other", "kk", "kick");
        plugins.remove_plugin("healer");
        let left: Vec<(&str, &str)> = plugins
            .list()
            .into_iter()
            .map(|(by, alias)| (by, alias.name.as_str()))
            .collect();
        assert_eq!(left, [("other", "kk")]);
        assert_eq!(sends(&saved, &plugins, "go Orla"), ["go Orla"]);
    }

    #[test]
    fn two_plugins_may_each_make_an_alias_of_one_name() {
        let saved = store(&[("hl", "cast heal %1")]);
        let mut plugins = PluginAliases::default();
        plugins.set("healer", "hl", "cast 'cure light' %1");
        plugins.set("cleric", "hl", "cast 'cure serious' %1");
        // The one made last expands, and both list.
        assert_eq!(
            sends(&saved, &plugins, "hl Orla"),
            ["cast 'cure serious' Orla"]
        );
        let listed: Vec<&str> = plugins.list().into_iter().map(|(by, _)| by).collect();
        assert_eq!(listed, ["cleric", "healer"]);
        // Once the cleric's goes, the healer's expands again.
        plugins.remove_plugin("cleric");
        assert_eq!(
            sends(&saved, &plugins, "hl Orla"),
            ["cast 'cure light' Orla"]
        );
        // Making it again keeps one alias of the name per plugin.
        plugins.set("healer", "hl", "cast heal %1");
        assert_eq!(plugins.list().len(), 1);
        assert!(plugins.remove("healer", "hl"));
        assert_eq!(sends(&saved, &plugins, "hl Orla"), ["cast heal Orla"]);
    }

    #[test]
    fn no_alias_passes_through() {
        let s = AliasStore::new();
        assert_eq!(
            s.expand_line("look", SESSION).unwrap(),
            vec!["look".to_string()]
        );
    }

    #[test]
    fn simple_alias_expands() {
        let s = store(&[("greet", "wave")]);
        assert_eq!(
            s.expand_line("greet", SESSION).unwrap(),
            vec!["wave".to_string()]
        );
    }

    #[test]
    fn alias_with_param_zero_takes_full_args() {
        let s = store(&[("chat", "say %0")]);
        assert_eq!(
            s.expand_line("chat hello there", SESSION).unwrap(),
            vec!["say hello there".to_string()]
        );
    }

    #[test]
    fn alias_with_positional_params() {
        let s = store(&[("kill", "attack %1 with %2")]);
        assert_eq!(
            s.expand_line("kill goblin sword", SESSION).unwrap(),
            vec!["attack goblin with sword".to_string()]
        );
    }

    #[test]
    fn missing_positional_param_substitutes_empty() {
        // %1 with no args expands to nothing. Trailing whitespace is trimmed.
        let s = store(&[("strike", "kick %1")]);
        assert_eq!(
            s.expand_line("strike", SESSION).unwrap(),
            vec!["kick".to_string()]
        );
    }

    #[test]
    fn alias_self_recursion_hits_limit() {
        // `#alias say {say %0}` is the classic infinite loop. The depth
        // guard catches it instead of running away.
        let s = store(&[("say", "say %0")]);
        assert!(matches!(
            s.expand_line("say hello", SESSION),
            Err(ExpandError::RecursionLimit { .. })
        ));
    }

    #[test]
    fn range_param_takes_from_nth_word_onward() {
        let s = store(&[("tell", "%1 says: %2-")]);
        assert_eq!(
            s.expand_line("tell bob hello there friend", SESSION)
                .unwrap(),
            vec!["bob says: hello there friend".to_string()]
        );
    }

    #[test]
    fn range_param_preserves_original_whitespace() {
        // `%2-` must keep the literal spacing between words instead of
        // collapsing to single spaces via `split_whitespace`. This is
        // why `args_from_word` walks `args` directly.
        let s = store(&[("echo", "[%2-]")]);
        assert_eq!(
            s.expand_line("echo skip  foo   bar", SESSION).unwrap(),
            vec!["[foo   bar]".to_string()]
        );
    }

    #[test]
    fn range_param_one_dash_is_full_args() {
        // `%1-` is the same as `%0`: every word starting from the first.
        let s = store(&[("a", "%1-"), ("b", "%0")]);
        assert_eq!(
            s.expand_line("a foo bar baz", SESSION).unwrap(),
            vec!["foo bar baz".to_string()]
        );
        assert_eq!(
            s.expand_line("b foo bar baz", SESSION).unwrap(),
            vec!["foo bar baz".to_string()]
        );
    }

    #[test]
    fn range_param_zero_dash_is_full_args() {
        // `%0-` is a degenerate but harmless form: it carries the same
        // meaning as `%0` and `%1-`. Documented so the parser is
        // predictable rather than rejecting it.
        let s = store(&[("a", "%0-")]);
        assert_eq!(
            s.expand_line("a foo bar baz", SESSION).unwrap(),
            vec!["foo bar baz".to_string()]
        );
    }

    #[test]
    fn range_param_with_too_few_words_expands_empty() {
        let s = store(&[("tell", "%1 says: %3-")]);
        assert_eq!(
            s.expand_line("tell bob hi", SESSION).unwrap(),
            vec!["bob says:".to_string()]
        );
    }

    #[test]
    fn range_param_with_no_args_expands_empty() {
        let s = store(&[("emote", "[%1-]")]);
        assert_eq!(
            s.expand_line("emote", SESSION).unwrap(),
            vec!["[]".to_string()]
        );
    }

    #[test]
    fn double_percent_is_literal() {
        let s = store(&[("scream", "say 100%% effort")]);
        assert_eq!(
            s.expand_line("scream", SESSION).unwrap(),
            vec!["say 100% effort".to_string()]
        );
    }

    #[test]
    fn semicolon_separated_expansion_yields_multiple_commands() {
        let s = store(&[("morning", "wave;bow;say good morning")]);
        assert_eq!(
            s.expand_line("morning", SESSION).unwrap(),
            vec![
                "wave".to_string(),
                "bow".to_string(),
                "say good morning".to_string()
            ]
        );
    }

    #[test]
    fn recursive_alias_expands_chain() {
        let s = store(&[("a", "b"), ("b", "c")]);
        assert_eq!(s.expand_line("a", SESSION).unwrap(), vec!["c".to_string()]);
    }

    #[test]
    fn cyclic_alias_hits_recursion_limit() {
        let s = store(&[("a", "b"), ("b", "a")]);
        assert!(matches!(
            s.expand_line("a", SESSION),
            Err(ExpandError::RecursionLimit { .. })
        ));
    }

    #[test]
    fn disabled_alias_passes_through() {
        let mut s = store(&[("greet", "wave")]);
        let mut alias = s.get("greet").unwrap().clone();
        alias.enabled = false;
        s.set(alias);
        assert_eq!(
            s.expand_line("greet", SESSION).unwrap(),
            vec!["greet".to_string()]
        );
    }

    #[test]
    fn user_input_with_semicolons_splits_first() {
        let s = AliasStore::new();
        assert_eq!(
            s.expand_line("look;sip water", SESSION).unwrap(),
            vec!["look".to_string(), "sip water".to_string()]
        );
    }

    #[test]
    fn escaped_semicolon_stays_literal() {
        let s = AliasStore::new();
        assert_eq!(
            s.expand_line("say hello\\;world", SESSION).unwrap(),
            vec!["say hello;world".to_string()]
        );
    }

    #[test]
    fn disabled_group_passes_alias_through() {
        let mut s = AliasStore::new();
        s.set(Alias::new("kk", "kick %1").with_group("Combat"));
        // Group enabled by default — the alias fires.
        assert_eq!(
            s.expand_line("kk goblin", SESSION).unwrap(),
            vec!["kick goblin".to_string()]
        );
        // Disable the whole group and the alias passes through as
        // typed (no expansion, no error).
        s.set_group_enabled("Combat", false);
        assert_eq!(
            s.expand_line("kk goblin", SESSION).unwrap(),
            vec!["kk goblin".to_string()]
        );
        // Re-enable and it fires again.
        s.set_group_enabled("Combat", true);
        assert_eq!(
            s.expand_line("kk goblin", SESSION).unwrap(),
            vec!["kick goblin".to_string()]
        );
    }

    #[test]
    fn ungrouped_alias_ignores_group_state() {
        // Sanity check that the disabled-set never affects ungrouped
        // aliases (a buggy is_group_enabled check might reach for an
        // empty-string entry).
        let mut s = AliasStore::new();
        s.set(Alias::new("greet", "wave"));
        s.set_disabled_groups([String::new(), "Combat".to_string()]);
        assert_eq!(
            s.expand_line("greet", SESSION).unwrap(),
            vec!["wave".to_string()]
        );
    }

    #[test]
    fn groups_lists_referenced_names_with_enabled_state() {
        let mut s = AliasStore::new();
        s.set(Alias::new("kk", "kick %1").with_group("Combat"));
        s.set(Alias::new("forge", "smith %1").with_group("Crafting"));
        s.set(Alias::new("greet", "wave"));
        s.set_group_enabled("Combat", false);
        let mut groups = s.groups();
        groups.sort();
        assert_eq!(
            groups,
            vec![
                ("Combat".to_string(), false),
                ("Crafting".to_string(), true)
            ]
        );
    }

    #[test]
    fn disabled_groups_round_trip_through_setters() {
        let mut s = AliasStore::new();
        s.set_disabled_groups(["Combat", "Crafting"]);
        let mut listed = s.disabled_groups();
        listed.sort();
        assert_eq!(listed, vec!["Combat".to_string(), "Crafting".to_string()]);
    }

    #[test]
    fn remove_alias() {
        let mut s = store(&[("greet", "wave")]);
        assert!(s.remove(None, "greet"));
        assert!(!s.remove(None, "greet"));
        assert_eq!(
            s.expand_line("greet", SESSION).unwrap(),
            vec!["greet".to_string()]
        );
    }

    #[test]
    fn revision_moves_only_when_the_list_changes() {
        let mut s = AliasStore::new();
        let empty = s.revision();
        s.set(Alias::new("greet", "wave").with_group("social"));
        let after_add = s.revision();
        assert_ne!(after_add, empty);
        s.set_group_enabled("social", false);
        s.set_disabled_groups(["social"]);
        assert!(!s.remove(None, "missing"));
        assert_eq!(s.revision(), after_add);
        s.set(Alias::new("greet", "bow"));
        let after_replace = s.revision();
        assert_ne!(after_replace, after_add);
        assert!(s.remove(None, "greet"));
        assert_ne!(s.revision(), after_replace);
    }

    #[test]
    fn a_replacement_store_reads_as_a_change() {
        let first = store(&[("greet", "wave")]);
        let second = store(&[("greet", "wave")]);
        assert_ne!(first.revision(), second.revision());
    }

    #[test]
    fn set_overwrites_existing_alias() {
        let mut s = store(&[("greet", "wave")]);
        s.set(Alias::new("greet", "bow"));
        assert_eq!(
            s.expand_line("greet", SESSION).unwrap(),
            vec!["bow".to_string()]
        );
    }

    #[test]
    fn live_names_are_the_aliases_that_expand_now() {
        let mut s = store(&[("kk", "kick"), ("hl", "cast heal"), ("Kk", "kick")]);
        let mut off = Alias::new("off", "rest");
        off.enabled = false;
        s.set(off);
        s.set(Alias::new("fl", "flee").with_group("combat"));
        s.set_group_enabled("combat", false);
        s.stop(None, "hl", SESSION);
        let mut plugins = PluginAliases::default();
        plugins.set("mapper", "go", "run %1");
        plugins.set("other", "kk", "kick hard");
        // A disabled alias, one in an off group and one stopped here are
        // left out. A plugin alias is kept, and a name shows once.
        assert_eq!(s.live_names(&plugins, SESSION), ["Kk", "go", "kk"]);
        // The stop holds only in the session it went under.
        assert_eq!(s.live_names(&plugins, OTHER), ["Kk", "go", "hl", "kk"]);
    }

    #[test]
    fn a_stopped_alias_passes_through_in_its_session_until_you_save_it() {
        let mut s = store(&[("hl", "cast heal")]);
        s.stop(None, "hl", SESSION);
        assert!(s.is_stopped(None, "hl", SESSION));
        assert_eq!(
            s.expand_line("hl", SESSION).unwrap(),
            vec!["hl".to_string()]
        );
        // The other session still expands it.
        assert!(!s.is_stopped(None, "hl", OTHER));
        assert_eq!(
            s.expand_line("hl", OTHER).unwrap(),
            vec!["cast heal".to_string()]
        );
        s.stop(None, "hl", OTHER);
        s.set(Alias::new("hl", "cast heal"));
        for key in [SESSION, OTHER] {
            assert!(!s.is_stopped(None, "hl", key));
            assert_eq!(
                s.expand_line("hl", key).unwrap(),
                vec!["cast heal".to_string()]
            );
        }
        // The stops of a session that closed go with it.
        s.stop(None, "hl", SESSION);
        s.forget_stops(SESSION);
        assert!(!s.is_stopped(None, "hl", SESSION));
    }

    #[test]
    fn a_whole_list_save_keeps_only_the_unchanged_alias_stops_in_each_session() {
        let mut old = store(&[("hl", "cast heal"), ("kk", "kick")]);
        old.stop(None, "hl", SESSION);
        old.stop(None, "kk", SESSION);
        old.stop(None, "kk", OTHER);
        let mut saved = store(&[("hl", "cast heal"), ("kk", "kick %1")]);
        saved.keep_stops_from(&old);
        assert!(saved.is_stopped(None, "hl", SESSION));
        assert!(!saved.is_stopped(None, "hl", OTHER));
        for key in [SESSION, OTHER] {
            assert!(!saved.is_stopped(None, "kk", key));
        }
        assert!(saved.remove(None, "hl"));
        assert!(!saved.is_stopped(None, "hl", SESSION));
    }

    #[test]
    fn list_returns_sorted_aliases() {
        let s = store(&[("zeta", "z"), ("alpha", "a"), ("mu", "m")]);
        let names: Vec<_> = s.list().iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "mu", "zeta"]);
    }

    #[test]
    fn a_script_alias_queues_its_body_with_the_words_after_its_name() {
        let mut s = store(&[("hunt", "kk %1;flee")]);
        let body = "mud.send('kick ' .. captures[1])";
        s.set(Alias::new("kk", "ignored").with_script(body));
        let kick = |captures: &[&str]| {
            ExpandStep::Script(ScriptCall {
                source: "kk".into(),
                group: None,
                body: body.into(),
                captures: captures.iter().map(|c| (*c).to_string()).collect(),
            })
        };
        // Each body stands where its alias was typed, between the
        // commands around it, at any depth.
        assert_eq!(
            s.expand_line_full(
                "kk  big   dragon;look;hunt rat;wave",
                &PluginAliases::default(),
                SESSION
            )
            .unwrap(),
            vec![
                kick(&["big", "dragon"]),
                ExpandStep::Command("look".into()),
                kick(&["rat"]),
                ExpandStep::Command("flee".into()),
                ExpandStep::Command("wave".into()),
            ]
        );
        assert_eq!(
            s.expand_line("kk dragon;look;hunt rat", SESSION).unwrap(),
            vec!["look".to_string(), "flee".to_string()]
        );
    }

    /// The store with `ds` in the groups Tolliver and Maren, each casting
    /// for its own character.
    fn two_characters() -> AliasStore {
        let mut s = AliasStore::new();
        s.set(Alias::new("ds", "cast 'detect scry' tolliver").with_group("Tolliver"));
        s.set(Alias::new("ds", "cast 'detect scry' maren").with_group("Maren"));
        s
    }

    #[test]
    fn two_groups_each_keep_an_alias_of_one_name() {
        let mut s = two_characters();
        let groups: Vec<Option<&str>> = s.list().iter().map(|a| a.group.as_deref()).collect();
        // Settings lists Maren before Tolliver, and so does the store.
        assert_eq!(groups, [Some("Maren"), Some("Tolliver")]);
        assert_eq!(
            s.get_in(Some("Tolliver"), "ds").unwrap().expansion,
            "cast 'detect scry' tolliver"
        );
        // Saving one again replaces only the one in its group.
        s.set(Alias::new("ds", "cast 'detect scry' self").with_group("Maren"));
        assert_eq!(s.list().len(), 2);
        assert_eq!(
            s.get_in(Some("Maren"), "ds").unwrap().expansion,
            "cast 'detect scry' self"
        );
        // One in no group is a third alias of the name.
        s.set(Alias::new("ds", "say no group"));
        assert_eq!(s.named("ds").len(), 3);
        assert_eq!(s.get("ds").unwrap().group, None);
    }

    #[test]
    fn the_group_settings_lists_first_wins() {
        let mut s = two_characters();
        assert_eq!(
            s.expand_line("ds", SESSION).unwrap(),
            ["cast 'detect scry' maren"]
        );
        // An alias in no group comes before every group.
        s.set(Alias::new("ds", "say first"));
        assert_eq!(s.expand_line("ds", SESSION).unwrap(), ["say first"]);
        assert!(s.remove(None, "ds"));
        // An alias you turned off gives way to the next one.
        let mut off = s.get_in(Some("Maren"), "ds").unwrap().clone();
        off.enabled = false;
        s.set(off);
        assert_eq!(
            s.expand_line("ds", SESSION).unwrap(),
            ["cast 'detect scry' tolliver"]
        );
    }

    #[test]
    fn switching_a_group_off_lets_the_other_fire() {
        let mut s = two_characters();
        s.set_group_enabled("Maren", false);
        assert_eq!(
            s.expand_line("ds", SESSION).unwrap(),
            ["cast 'detect scry' tolliver"]
        );
        assert_eq!(
            s.winner("ds", SESSION).unwrap().group.as_deref(),
            Some("Tolliver")
        );
        s.set_group_enabled("Tolliver", false);
        assert_eq!(s.expand_line("ds", SESSION).unwrap(), ["ds"]);
        assert_eq!(
            s.chosen("ds", SESSION).unwrap().group.as_deref(),
            Some("Maren")
        );
    }

    #[test]
    fn unalias_removes_the_alias_of_one_group() {
        let mut s = two_characters();
        assert!(!s.remove(None, "ds"));
        assert!(!s.remove(Some("Orla"), "ds"));
        assert!(s.remove(Some(" Maren "), "ds"));
        assert_eq!(
            s.expand_line("ds", SESSION).unwrap(),
            ["cast 'detect scry' tolliver"]
        );
        assert!(s.remove(Some("Tolliver"), "ds"));
        let leftover = s.list();
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn a_stop_holds_off_the_alias_of_one_group() {
        let mut s = two_characters();
        s.stop(Some("Maren"), "ds", SESSION);
        assert!(!s.is_stopped(Some("Tolliver"), "ds", SESSION));
        assert_eq!(
            s.expand_line("ds", SESSION).unwrap(),
            ["cast 'detect scry' tolliver"]
        );
        let mut saved = two_characters();
        saved.keep_stops_from(&s);
        assert!(saved.is_stopped(Some("Maren"), "ds", SESSION));
    }

    #[test]
    fn names_and_groups_are_trimmed() {
        let mut s = AliasStore::new();
        let mut alias = Alias::new("  ds ", "cast 'detect scry'");
        alias.group = Some(" Orla ".into());
        s.set(alias);
        let mut blank = Alias::new("hl", "cast heal");
        blank.group = Some("  ".into());
        s.set(blank);
        let ds = s.get_in(Some("Orla"), "ds").unwrap();
        assert_eq!(
            (ds.name.as_str(), ds.group.as_deref()),
            ("ds", Some("Orla"))
        );
        assert_eq!(s.get("hl").unwrap().group, None);
        assert_eq!(
            s.expand_line("ds", SESSION).unwrap(),
            ["cast 'detect scry'"]
        );
    }

    #[test]
    fn a_runaway_alias_names_itself() {
        let s = store(&[("say", "say %0")]);
        let err = s.expand_line("say hello", SESSION).unwrap_err();
        assert_eq!(
            err.to_string(),
            "alias say calls itself, so Vosh stopped it after 16 steps"
        );
        let mut s = store(&[("a", "b"), ("b", "c")]);
        s.set(Alias::new("c", "a").with_group("Orla"));
        let err = s.expand_line("a", SESSION).unwrap_err();
        assert_eq!(
            err.to_string(),
            "alias a calls itself through b and c in Orla, so Vosh stopped it after 16 steps"
        );
    }

    #[test]
    fn recursion_limit_can_be_lowered() {
        let s = store(&[("a", "a")]).with_max_depth(2);
        assert!(matches!(
            s.expand_line("a", SESSION),
            Err(ExpandError::RecursionLimit { depth: 2, .. })
        ));
    }
}
