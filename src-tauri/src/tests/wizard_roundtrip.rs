//! A generated round trip of the shared catalog wizard. A seeded
//! generator builds hundreds of profile sets, each with two to four
//! profiles that hold aliases, triggers, and macros in groups, items
//! several profiles share alike or in conflict, a second copy of a name
//! further on in a file, a key bound twice in one file, where the input
//! bar fires the last copy that is on, triggers in orders and at priorities of each
//! profile's own, groups off for one kind and on for another under one
//! name, presets on and off, preset triggers a file lacks, a profile that
//! never saved a file, which you now and then switch to and run the
//! wizard as before anything saves it, and settings that are not
//! automation. For each set
//! it launches as every character in per profile mode to see what each
//! has on, and what each has on after each of a run of `#group` commands.
//! It runs the wizard, then launches as every character in loadout mode,
//! runs the same `#group` commands, switches between the characters with
//! saves in between, and launches as each again and switches to each,
//! running the `#group` commands once more. Every time, each character
//! must have on exactly the aliases, triggers, and macros it had on
//! before, with the same content, its triggers in the same order, and
//! every other setting as it was.
//!
//! The presets that are on are shared in loadout mode, so where the
//! characters had different preset lists, each character is compared to
//! the per profile launch with the shared list in place of its own.
//! Where two characters had different versions of one alias or macro,
//! the wizard keeps the one you pick, so each character is compared to
//! that version. Each version of a trigger stays, under a name of its
//! own, so a trigger is compared without its name.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use vosh_automation::alias::Alias;
use vosh_automation::trigger::{Trigger, TriggerAction, TriggerPattern, TriggerTarget};

use crate::app::state::{AppState, SharedState};
use crate::disk::save::PERSIST_LOCK;
use crate::loadouts::presets::PRESETS_ON_BY_DEFAULT;
use crate::loadouts::wizard::plan::{ItemKind, ItemPayload};
use crate::profile::file::ProfileConfig;
use crate::profile::live::{Macro, Profile, Timer};
use crate::profile::panes::PaneLayoutPersist;
use crate::profile::set::{ProfileSet, DEFAULT_PROFILE_NAME};
use crate::profile::shared::{Scope, ScopeConfig};
use crate::profile::ui::TrackedAffect;

/// How many profile sets the round trip builds.
const SETS: u64 = 300;

/// `SplitMix64`, so every run builds the same sets without a new crate.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }

    fn pick<T: Clone>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())].clone()
    }
}

const PROFILES: [&str; 4] = [DEFAULT_PROFILE_NAME, "Healer", "Test-Prompt", "Bard"];
const GROUPS: [&str; 3] = ["combat", "loot", "buffs"];
const ALIASES: [&str; 6] = ["kk", "bash", "loot", "heal", "cc", "dd"];
const TRIGGERS: [&str; 4] = ["flee", "loot", "greet", "tell"];
const MACROS: [&str; 4] = ["f1", "f2", "ctrl+1", "f3"];

/// The preset library the launch below installs from, as src/automation/presets.ts
/// holds it: each preset id with the names of its triggers. The first
/// three are among [`PRESETS_ON_BY_DEFAULT`], so an empty list turns them
/// on. The last ships off, as a preset added after the defaults froze
/// does, so an empty list leaves it off and a list beside an empty one
/// can name it.
const LIBRARY: [(&str, &[&str]); 4] = [
    ("healing_basics", &["preset heal 1", "preset heal 2"]),
    ("potion_labels", &["preset potion 1"]),
    ("herb_labels", &["preset herb 1", "preset herb 2"]),
    ("later_labels", &["preset later 1"]),
];

/// A preset an older build had, which [`LIBRARY`] no longer holds. A
/// launch installs it for no one, and takes out every copy it finds.
const DROPPED_PRESET: &str = "old_labels";

/// The id of every preset in [`LIBRARY`], as the wizard takes them.
fn library_ids() -> Vec<&'static str> {
    LIBRARY.iter().map(|(id, _)| *id).collect()
}

/// A preset trigger as the library holds it, or as an older build left
/// it in a profile file.
fn preset_trigger(preset: &str, name: &str, older: bool) -> Trigger {
    let pattern = if older {
        format!("^{name} older$")
    } else {
        format!("^{name}$")
    };
    Trigger {
        priority: 5,
        preset: Some(preset.to_string()),
        ..Trigger::new(
            name,
            pattern,
            TriggerAction::Send {
                template: format!("say {name}"),
            },
        )
    }
}

/// The presets that are on for a stored `enabled_presets` list, in
/// library order, as `enabledPresetIds` in src/automation/automationRecords.ts
/// reads it. An empty list means the defaults, `PRESETS_ON_BY_DEFAULT`.
fn presets_on(stored: &[String]) -> Vec<&'static str> {
    LIBRARY
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| {
            if stored.is_empty() {
                PRESETS_ON_BY_DEFAULT.contains(id)
            } else {
                stored.iter().any(|s| s == id)
            }
        })
        .collect()
}

/// What the main window does with the preset triggers once a launch has
/// loaded the profile, as `installLaunchPresets` in
/// automation/automationRecords.ts does it. Every preset that is off
/// comes out through `presets_remove`, and every one that is on installs
/// again through `presets_install`. Each command saves, as the real
/// commands do.
async fn preset_launch_plan(state: &SharedState) {
    let (remove, install) = {
        let p = state.selected_profile().await;
        let on = presets_on(&p.ui.enabled_presets);
        let installed: BTreeSet<String> = p
            .triggers
            .list()
            .into_iter()
            .filter_map(|t| t.preset)
            .collect();
        let remove: Vec<String> = installed
            .into_iter()
            .filter(|id| !on.contains(&id.as_str()))
            .collect();
        (remove, on)
    };
    for id in remove {
        state
            .selected_profile()
            .await
            .triggers
            .remove_by_preset(&id);
        save(state).await;
    }
    let triggers: Vec<Trigger> = LIBRARY
        .iter()
        .filter(|(id, _)| install.contains(id))
        .flat_map(|(id, names)| names.iter().map(|n| preset_trigger(id, n, false)))
        .collect();
    if !triggers.is_empty() {
        crate::loadouts::presets::install_preset_triggers(
            &mut *state.selected_profile().await,
            triggers,
        )
        .unwrap();
        save(state).await;
    }
}

/// The save a Settings edit, a debounce, or a command runs.
async fn save(state: &SharedState) {
    let _persist_guard = PERSIST_LOCK.lock().await;
    crate::disk::save::persist_state(state, &state.selected_session().profile()).await;
}

/// Open Vosh as `name` over `dir` the way app/launch.rs launches it,
/// then let the main window bring the preset triggers in line.
async fn launch_as(dir: &Path, name: &str) -> SharedState {
    ProfileSet::load_or_migrate(dir.to_path_buf())
        .unwrap()
        .switch(name)
        .unwrap();
    let state: SharedState = Arc::new(AppState::default());
    crate::app::launch::load(&state, dir).await;
    preset_launch_plan(&state).await;
    state
}

/// One line per alias, trigger, or macro that is on in `p`, with its
/// content. The group and the enabled flag say whether it is on, so the
/// line leaves them out. The aliases and the macros come sorted, then the
/// triggers in the order the store runs them, since every trigger that
/// matches a line fires in that order.
fn on_rows(p: &Profile) -> Vec<String> {
    let mut rows = Vec::new();
    for a in p.aliases.list() {
        if a.enabled && p.aliases.is_group_enabled(a.group.as_deref().unwrap_or("")) {
            rows.push(alias_row(a));
        }
    }
    // The input bar fires the last copy of a key that is on and whose
    // group is on, as `rebuild` in src/input/useMacroKeys.ts builds its
    // map.
    let mut fired: BTreeMap<&str, &Macro> = BTreeMap::new();
    for m in &p.macros {
        let group_on = m
            .group
            .as_deref()
            .is_none_or(|g| g.is_empty() || !p.disabled_macro_groups.contains(g));
        if m.enabled && group_on {
            fired.insert(&m.key, m);
        }
    }
    for m in fired.values() {
        rows.push(macro_row(m));
    }
    rows.sort();
    for t in p.triggers.list() {
        if t.enabled
            && p.triggers
                .is_group_enabled(t.group.as_deref().unwrap_or(""))
        {
            rows.push(trigger_row(&t));
        }
    }
    rows
}

fn alias_row(a: &Alias) -> String {
    let mut a = a.clone();
    a.group = None;
    a.enabled = true;
    format!("alias\t{}\t{}", a.name, serde_json::to_string(&a).unwrap())
}

/// A trigger's name is only a label, and the wizard gives each other
/// version of one a name of its own, so the line leaves the name out too.
fn trigger_row(t: &Trigger) -> String {
    let mut t = t.clone();
    t.name.clear();
    t.group = None;
    t.enabled = true;
    format!("trigger\t\t{}", serde_json::to_string(&t).unwrap())
}

fn macro_row(m: &Macro) -> String {
    let mut m = m.clone();
    m.group = None;
    m.enabled = true;
    format!("macro\t{}\t{}", m.key, serde_json::to_string(&m).unwrap())
}

/// The kind and the name a row starts with.
fn row_key(row: &str) -> String {
    let mut fields = row.splitn(3, '\t');
    format!(
        "{}\t{}",
        fields.next().unwrap_or(""),
        fields.next().unwrap_or("")
    )
}

/// What `config` holds besides the aliases, triggers, and macros, the
/// preset list the catalog owns, and the group checkbox lists and the
/// folder map, which name the catalog groups in loadout mode. As TOML, so
/// it compares every field.
fn settings(mut config: ProfileConfig) -> String {
    config.clear_catalog_items();
    config.ui.enabled_presets.clear();
    config.disabled_alias_groups.clear();
    config.disabled_trigger_groups.clear();
    config.disabled_macro_groups.clear();
    config.group_folders = crate::profile::file::GroupFolders::default();
    config.to_toml().unwrap()
}

/// Copy the app data folder `from` into `to`, as a snapshot the per
/// profile launches below can write over without touching the original.
fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// One generated profile set.
struct Set {
    names: Vec<String>,
    /// The file of each profile, None for one that never saved a file.
    files: Vec<Option<ProfileConfig>>,
    scope: ScopeConfig,
    /// The profile you run the wizard as.
    wizard: usize,
}

/// A stored preset list: the defaults, every preset off, or some on.
fn preset_list(rng: &mut Rng) -> Vec<String> {
    match rng.below(4) {
        0 => Vec::new(),
        1 => vec!["none".to_string()],
        _ => {
            let on: Vec<String> = LIBRARY
                .iter()
                .filter(|_| rng.chance(50))
                .map(|(id, _)| (*id).to_string())
                .collect();
            if on.is_empty() {
                vec![LIBRARY[0].0.to_string()]
            } else {
                on
            }
        }
    }
}

/// `items` in an order `rng` picks.
fn shuffled<T: Clone>(rng: &mut Rng, items: &[T]) -> Vec<T> {
    let mut out = items.to_vec();
    for i in (1..out.len()).rev() {
        out.swap(i, rng.below(i + 1));
    }
    out
}

fn some_group(rng: &mut Rng) -> Option<String> {
    if rng.chance(25) {
        None
    } else {
        Some(rng.pick(&GROUPS).to_string())
    }
}

/// The group an item goes in: mostly the one every profile uses for its
/// name, so shared items often agree, and now and then another.
fn item_group(rng: &mut Rng, usual: Option<&String>) -> Option<String> {
    if rng.chance(75) {
        usual.cloned()
    } else {
        some_group(rng)
    }
}

fn generate(seed: u64) -> Set {
    let mut rng = Rng(seed);
    let count = 2 + rng.below(3);
    let names: Vec<String> = PROFILES[..count].iter().map(|s| (*s).to_string()).collect();
    // Half the sets have a profile that never saved a file. You run the
    // wizard as a profile that did, since a launch saves the live one.
    let never_saved = rng.chance(50).then(|| rng.below(count));
    let saved: Vec<usize> = (0..count).filter(|i| Some(*i) != never_saved).collect();
    let wizard = rng.pick(&saved);
    let shared_list = rng.chance(60).then(|| preset_list(&mut rng));
    let usual: BTreeMap<String, Option<String>> = ALIASES
        .iter()
        .map(|n| format!("alias {n}"))
        .chain(TRIGGERS.iter().map(|n| format!("trigger {n}")))
        .chain(MACROS.iter().map(|n| format!("macro {n}")))
        .map(|key| (key, some_group(&mut rng)))
        .collect();
    // The order and the priority each trigger usually has. Some share the
    // priority of the preset triggers.
    let trigger_order = shuffled(&mut rng, &TRIGGERS);
    let priorities: BTreeMap<&str, i32> = TRIGGERS
        .iter()
        .map(|name| (*name, if rng.chance(30) { 5 } else { 0 }))
        .collect();

    let mut files = Vec::new();
    for i in 0..count {
        if Some(i) == never_saved {
            files.push(None);
            continue;
        }
        let mut config = ProfileConfig::default();
        for group in GROUPS {
            if rng.chance(30) {
                config.disabled_alias_groups.push(group.to_string());
            }
            if rng.chance(30) {
                config.disabled_trigger_groups.push(group.to_string());
            }
            if rng.chance(30) {
                config.disabled_macro_groups.push(group.to_string());
            }
        }
        for name in ALIASES {
            if !rng.chance(55) {
                continue;
            }
            let version = if rng.chance(80) { "one" } else { "two" };
            let mut a = Alias::new(name, format!("{name} {version}"));
            if rng.chance(10) {
                a.script = Some(format!("mud.send('{name}')"));
            }
            a.enabled = rng.chance(85);
            a.group = item_group(&mut rng, usual[&format!("alias {name}")].as_ref());
            config.aliases.push(a);
        }
        // Most files hold their triggers in the order the set uses, and
        // some in an order of their own.
        let order = if rng.chance(50) {
            trigger_order.clone()
        } else {
            shuffled(&mut rng, &TRIGGERS)
        };
        for name in order {
            if !rng.chance(55) {
                continue;
            }
            let version = if rng.chance(80) { "one" } else { "two" };
            let priority = if rng.chance(90) {
                priorities[name]
            } else {
                5 - priorities[name]
            };
            config.triggers.push(Trigger {
                name: name.to_string(),
                patterns: vec![TriggerPattern::regex(format!("^{name} {version}$"))],
                priority,
                enabled: rng.chance(85),
                actions: vec![TriggerAction::Send {
                    template: format!("say {name}"),
                }],
                preset: None,
                group: item_group(&mut rng, usual[&format!("trigger {name}")].as_ref()),
                target: TriggerTarget::Line,
                alert: None,
            });
        }
        // Now and then a file holds a second copy of a name further on,
        // which is the one its store keeps, in its own place.
        for _ in 0..2 {
            if !config.triggers.is_empty() && rng.chance(10) {
                let mut again = rng.pick(&config.triggers);
                again.patterns[0].pattern.push_str(" again");
                again.enabled = rng.chance(85);
                config.triggers.push(again);
            }
        }
        if !config.aliases.is_empty() && rng.chance(10) {
            let mut again = rng.pick(&config.aliases);
            again.expansion.push_str(" again");
            config.aliases.push(again);
        }
        for key in MACROS {
            if !rng.chance(55) {
                continue;
            }
            let version = if rng.chance(80) { "one" } else { "two" };
            config.macros.push(Macro {
                key: key.to_string(),
                command: format!("cast {version}"),
                group: item_group(&mut rng, usual[&format!("macro {key}")].as_ref()),
                enabled: rng.chance(85),
                preset: None,
            });
        }
        // Now and then a file binds a key twice, in the same folder, and
        // the input bar fires the last copy that is on.
        if !config.macros.is_empty() && rng.chance(25) {
            let mut again = rng.pick(&config.macros);
            again.command.push_str(" again");
            again.enabled = rng.chance(50);
            config.macros.push(again);
        }
        // The presets this character has on, and their triggers as its
        // file holds them. A file saved before a preset came out lacks
        // its triggers, and an older build left older patterns.
        let list = match &shared_list {
            Some(list) => list.clone(),
            None => preset_list(&mut rng),
        };
        for (id, triggers) in LIBRARY {
            let on = presets_on(&list).contains(&id);
            if !(on && rng.chance(80) || !on && rng.chance(5)) {
                continue;
            }
            for name in triggers {
                let mut t = preset_trigger(id, name, rng.chance(15));
                if rng.chance(30) {
                    t.group = some_group(&mut rng);
                }
                t.enabled = rng.chance(90);
                config.triggers.push(t);
            }
        }
        // Now and then an older build left the trigger of a preset the
        // library no longer has.
        if rng.chance(20) {
            let mut t = preset_trigger(DROPPED_PRESET, "preset old 1", false);
            if rng.chance(30) {
                t.group = some_group(&mut rng);
            }
            config.triggers.push(t);
        }
        config.ui.enabled_presets = list;
        // Settings that are not automation.
        config
            .profile_vars
            .insert("target".into(), format!("orc {}", rng.below(100)));
        if rng.chance(50) {
            config
                .profile_vars
                .insert("pet".into(), format!("wolf {}", rng.below(100)));
        }
        config.tick.enabled = rng.chance(50);
        config.tick.interval_secs = 20 + rng.below(40) as u64;
        if rng.chance(50) {
            config.timers = vec![Timer {
                id: 1 + rng.below(9) as u32,
                name: format!("drink {i}"),
                interval_secs: 30 + rng.below(90) as u32,
                command: "drink".into(),
                enabled: rng.chance(50),
                group: None,
            }];
        }
        config.ui.vitals_values = rng.pick(&["current-max", "current", "percent"]).into();
        config.ui.chip_style = rng.pick(&["value_only", "caption", "icon"]).into();
        if rng.chance(50) {
            config.ui.tracked_affects = vec![TrackedAffect {
                name: format!("Sanctuary {i}"),
                label: rng.chance(50).then(|| format!("S{i}")),
            }];
        }
        config.ui.panes = Some(PaneLayoutPersist {
            panel_open: rng.chance(50),
            panel_width: Some(250 + 10 * rng.below(20) as u32),
            ..PaneLayoutPersist::default_layout()
        });
        if rng.chance(30) {
            config.plugins.enabled = vec![format!("plugin {i}")];
        }
        files.push(Some(config));
    }
    let scope = if rng.chance(50) {
        ScopeConfig::default()
    } else {
        ScopeConfig {
            theme: Scope::Profile,
            dock_layout: Scope::Profile,
            ..ScopeConfig::default()
        }
    };
    Set {
        names,
        files,
        scope,
        wizard,
    }
}

/// Write `set` into the app data folder `dir`.
fn write_set(set: &Set, dir: &Path) {
    let mut profiles = ProfileSet::load_or_migrate(dir.to_path_buf()).unwrap();
    for name in &set.names[1..] {
        profiles.create(name).unwrap();
    }
    profiles.set_scope(set.scope).unwrap();
    for (name, file) in set.names.iter().zip(&set.files) {
        if let Some(config) = file {
            config.save(&profiles.profile_path(name)).unwrap();
        }
    }
}

/// The shared preset list the catalog takes, by the rule
/// `first_catalog_presets` follows, worked out here on its own: every
/// preset that any profile file had on, or the live list when no
/// profile saved a file. The wizard saves the live profile before it
/// reads the files, so with `unsaved` naming the live profile when it has
/// no file yet, its live list counts as the one that save gives it.
fn shared_presets(
    dir: &Path,
    names: &[String],
    unsaved: Option<&str>,
    live: &[String],
) -> Vec<&'static str> {
    let profiles = ProfileSet::load_or_migrate(dir.to_path_buf()).unwrap();
    let lists: Vec<Vec<String>> = names
        .iter()
        .map(|n| profiles.profile_path(n))
        .filter(|p| p.exists())
        .map(|p| ProfileConfig::load(&p).unwrap().ui.enabled_presets)
        .chain(unsaved.map(|_| live.to_vec()))
        .collect();
    if lists.is_empty() {
        return presets_on(live);
    }
    let on: BTreeSet<&str> = lists.iter().flat_map(|l| presets_on(l)).collect();
    LIBRARY
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| on.contains(id))
        .collect()
}

/// A stored list that turns on exactly `on`.
fn stored_list(on: &[&str]) -> Vec<String> {
    if on.is_empty() {
        vec!["none".to_string()]
    } else {
        on.iter().map(|s| (*s).to_string()).collect()
    }
}

/// What one character has in per profile mode.
struct Before {
    /// Its settings besides automation, from the live profile.
    settings: String,
    /// The items it has on, with the shared preset list in place of its
    /// own.
    on: Vec<String>,
    /// The items it has on after each step of [`group_steps`].
    toggled: Vec<Vec<String>>,
}

/// The `#group` commands each character runs, the same before and after
/// the wizard: every folder name off and on again, in an order of the
/// set's own, and one of them off to end with. A character runs them for
/// folders it never had too.
fn group_steps(seed: u64) -> Vec<(&'static str, bool)> {
    let mut rng = Rng(seed ^ 0x6a09_e667);
    let folders = shuffled(&mut rng, &GROUPS);
    let mut steps: Vec<(&str, bool)> = folders
        .iter()
        .flat_map(|folder| [(*folder, false), (*folder, true)])
        .collect();
    steps.push((rng.pick(&GROUPS), false));
    steps
}

/// Run `steps` as `#group` commands on the live profile of `state`, and
/// return the items on after each.
async fn run_group_steps(state: &SharedState, steps: &[(&str, bool)]) -> Vec<Vec<String>> {
    let mut p = state.selected_profile().await;
    steps
        .iter()
        .map(|(folder, on)| {
            let word = if *on { "on" } else { "off" };
            crate::input::process(&mut p, &format!("#group {folder} {word}"));
            on_rows(&p)
        })
        .collect()
}

/// Launch as each character over a copy of `dir`, in per profile mode.
async fn before_wizard(
    dir: &Path,
    names: &[String],
    shared: &[&str],
    steps: &[(&str, bool)],
) -> Vec<Before> {
    let mut out = Vec::new();
    for name in names {
        let copy = tempfile::tempdir().unwrap();
        copy_dir(dir, copy.path());
        let mut state = launch_as(copy.path(), name).await;
        let (settings_now, list) = {
            let p = state.selected_profile().await;
            (
                settings(ProfileConfig::from_profile(&p)),
                p.ui.enabled_presets.clone(),
            )
        };
        if presets_on(&list) != shared {
            // The launch saved this profile's file. Give it the shared
            // list and launch again.
            let path = ProfileSet::load_or_migrate(copy.path().to_path_buf())
                .unwrap()
                .profile_path(name);
            let mut config = ProfileConfig::load(&path).unwrap();
            config.ui.enabled_presets = stored_list(shared);
            config.save(&path).unwrap();
            state = launch_as(copy.path(), name).await;
        }
        let on = on_rows(&*state.selected_profile().await);
        out.push(Before {
            settings: settings_now,
            on,
            toggled: run_group_steps(&state, steps).await,
        });
    }
    out
}

fn kind_word(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Alias => "alias",
        ItemKind::Trigger => "trigger",
        ItemKind::Macro => "macro",
        ItemKind::Preset => "preset",
    }
}

/// What differs between the rows a character has on and the rows it
/// should have on.
fn diff(name: &str, when: &str, got: &[String], want: &[String]) -> Result<(), String> {
    if got == want {
        return Ok(());
    }
    let missing: Vec<&String> = want.iter().filter(|r| !got.contains(r)).collect();
    let extra: Vec<&String> = got.iter().filter(|r| !want.contains(r)).collect();
    if missing.is_empty() && extra.is_empty() {
        return Err(format!(
            "{name} {when}: the triggers run in another order, {got:?} for {want:?}"
        ));
    }
    Err(format!(
        "{name} {when}: missing {missing:?}, extra {extra:?}"
    ))
}

async fn check(
    state: &SharedState,
    name: &str,
    when: &str,
    before: &Before,
    want: &[String],
) -> Result<(), String> {
    let p = state.selected_profile().await;
    diff(name, when, &on_rows(&p), want)?;
    let now = settings(ProfileConfig::from_profile(&p));
    if now != before.settings {
        return Err(format!(
            "{name} {when}: settings changed\n{}\nbecame\n{now}",
            before.settings
        ));
    }
    Ok(())
}

/// Build set `seed`, run the wizard over it, and check every character.
async fn round_trip(seed: u64) -> Result<(), String> {
    let set = generate(seed);
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path();
    write_set(&set, dir);
    let names = &set.names;

    // You play the wizard's character, as a launch leaves it.
    // The wizard saves the live profile before it reads the files, as any
    // save does, and the files as they stand after that save are the
    // ones it keeps.
    let wizard = launch_as(dir, &names[set.wizard]).await;
    save(&wizard).await;
    // Now and then you switch to the profile that never saved a file and
    // open the wizard before anything saves it. The preview used to leave
    // it out of the shared list, which apply then counted.
    let mut rng = Rng(seed ^ 0x3c6e_f372);
    let unsaved = set
        .files
        .iter()
        .position(Option::is_none)
        .filter(|_| rng.chance(50))
        .map(|n| names[n].as_str());
    if let Some(name) = unsaved {
        crate::profile::switch::switch_profile(&wizard, &wizard.selected_session(), name)
            .await
            .map_err(|e| format!("switch: {e}"))?;
    }
    let live = wizard.selected_profile().await.ui.enabled_presets.clone();
    let shared = shared_presets(dir, names, unsaved, &live);
    let files_before: Vec<Option<String>> = {
        let profiles = ProfileSet::load_or_migrate(dir.to_path_buf()).unwrap();
        names
            .iter()
            .map(|n| std::fs::read_to_string(profiles.profile_path(n)).ok())
            .collect()
    };
    let steps = group_steps(seed);
    let before = before_wizard(dir, names, &shared, &steps).await;

    // Pick a version of each item in conflict.
    let mut rng = Rng(seed ^ 0xa5a5_a5a5);
    let plan = crate::loadouts::wizard::apply::analyze_migration(&wizard, &library_ids())
        .await
        .map_err(|e| format!("analyze: {e}"))?;
    // The preview holds the preset list every character shares and the
    // list each one had, so it can say who gains or loses a preset. A
    // profile that never saved a file has the presets on by default on.
    let previewed = presets_on(&plan.shared_presets);
    if previewed != shared {
        return Err(format!("the preview shares {previewed:?}, not {shared:?}"));
    }
    for (n, name) in names.iter().enumerate() {
        let own = files_before[n].as_deref().map_or_else(Vec::new, |text| {
            ProfileConfig::from_toml(text).unwrap().ui.enabled_presets
        });
        let previewed = plan.profile_presets.get(n).map(|list| presets_on(list));
        if previewed != Some(presets_on(&own)) {
            return Err(format!(
                "{name}: the preview has {previewed:?} on, not {:?}",
                presets_on(&own)
            ));
        }
    }

    // Now and then leave a conflict to the version the wizard picks.
    let mut resolutions = Vec::new();
    let mut chosen: BTreeMap<String, String> = BTreeMap::new();
    for conflict in &plan.conflicts {
        let row = |item: &ItemPayload| match item {
            ItemPayload::Alias { item } => Ok(alias_row(item)),
            // Each version of a trigger stays, so none is asked about.
            ItemPayload::Trigger { item } => {
                Err(format!("the wizard asks about trigger {}", item.name))
            }
            ItemPayload::Macro { item } => Ok(macro_row(item)),
            // The characters here edit no preset.
            ItemPayload::Preset { .. } => {
                Err(format!("the wizard asks about preset {}", conflict.name))
            }
        };
        // The one version switched on anywhere, when exactly one is, is
        // the one the wizard keeps unless you pick another.
        let mut on = BTreeSet::new();
        for v in conflict.variants.iter().filter(|v| v.switched_on) {
            on.insert(row(&v.item)?);
        }
        let default = conflict
            .variants
            .iter()
            .find(|v| v.source_profile == conflict.default_source)
            .ok_or_else(|| format!("{}: the default is no variant", conflict.name))?;
        let first = &conflict.variants[0];
        match on.len() {
            1 if !default.switched_on || !on.contains(&row(&default.item)?) => {
                return Err(format!(
                    "{}: the wizard picks {}, not the one version that was on",
                    conflict.name, conflict.default_source
                ));
            }
            1 => {}
            _ if default.source_profile != first.source_profile => {
                return Err(format!(
                    "{}: the wizard picks {} over the first profile",
                    conflict.name, conflict.default_source
                ));
            }
            _ => {}
        }
        let variant = if rng.chance(30) {
            default
        } else {
            let variant = &conflict.variants[rng.below(conflict.variants.len())];
            resolutions.push(crate::loadouts::wizard::apply::ConflictResolution {
                kind: conflict.kind,
                name: conflict.name.clone(),
                source_profile: variant.source_profile.clone(),
            });
            variant
        };
        chosen.insert(
            format!("{}\t{}", kind_word(conflict.kind), conflict.name),
            row(&variant.item)?,
        );
    }
    // A kept version has the name of the one it stands for, so the
    // aliases and macros stay sorted.
    let kept = |rows: &[String]| -> Vec<String> {
        rows.iter()
            .map(|row| match chosen.get(&row_key(row)) {
                Some(kept) => kept.clone(),
                None => row.clone(),
            })
            .collect()
    };
    let want: Vec<Vec<String>> = before.iter().map(|b| kept(&b.on)).collect();
    let want_toggled: Vec<Vec<Vec<String>>> = before
        .iter()
        .map(|b| b.toggled.iter().map(|rows| kept(rows)).collect())
        .collect();

    crate::loadouts::wizard::apply::apply_migration(&wizard, &resolutions, &library_ids())
        .await
        .map_err(|e| format!("apply: {e}"))?;
    drop(wizard);

    // Every file keeps its settings, and a copy of each waits in legacy.
    let profiles = ProfileSet::load_or_migrate(dir.to_path_buf()).unwrap();
    for (n, name) in names.iter().enumerate() {
        let path = profiles.profile_path(name);
        match &files_before[n] {
            Some(text) => {
                let kept = ProfileConfig::load(&path).map_err(|e| e.to_string())?;
                if settings(kept) != settings(ProfileConfig::from_toml(text).unwrap()) {
                    return Err(format!("{name}: the wizard changed a setting in its file"));
                }
                let legacy = path
                    .parent()
                    .unwrap()
                    .join("legacy")
                    .join(format!("{name}.toml"));
                if std::fs::read_to_string(&legacy).ok().as_ref() != Some(text) {
                    return Err(format!("{name}: the legacy copy differs from the file"));
                }
            }
            // A new file holds what a switch to the profile loaded.
            None if path.exists() => {
                let kept = ProfileConfig::load(&path).map_err(|e| e.to_string())?;
                if settings(kept) != settings(ProfileConfig::fresh()) {
                    return Err(format!("{name}: the wizard gave a new file settings"));
                }
            }
            None => {}
        }
    }

    // The catalog turns the trigger of a preset the library no longer has
    // on for no character whose file lacked it. A launch takes it out, so
    // the checks below never see it.
    let (catalog, _) = crate::loadouts::load_at_launch(dir)
        .map_err(|e| format!("the catalog does not read: {e:?}"))?;
    let dropped: Vec<&Trigger> = catalog
        .triggers
        .iter()
        .filter(|t| t.preset.as_deref() == Some(DROPPED_PRESET))
        .collect();
    for (n, name) in names.iter().enumerate() {
        let had = files_before[n].as_deref().is_some_and(|text| {
            ProfileConfig::from_toml(text)
                .unwrap()
                .triggers
                .iter()
                .any(|t| t.preset.as_deref() == Some(DROPPED_PRESET))
        });
        let path = profiles.profile_path(name);
        let file = if path.exists() {
            ProfileConfig::load(&path).map_err(|e| e.to_string())?
        } else {
            ProfileConfig::default()
        };
        let on = dropped.iter().any(|t| {
            t.enabled
                && t.group.as_deref().map_or(true, |g| {
                    g.is_empty() || !file.disabled_trigger_groups.iter().any(|o| o == g)
                })
        });
        if on && !had {
            return Err(format!(
                "{name}: the catalog turns on {DROPPED_PRESET}, which its file never had"
            ));
        }
    }

    // Quit, then open Vosh as each character.
    for (n, name) in names.iter().enumerate() {
        let state = launch_as(dir, name).await;
        if state.global_catalog.lock().await.is_none() {
            return Err(format!("{name}: no loadout mode after the wizard"));
        }
        check(&state, name, "at the first launch", &before[n], &want[n]).await?;
        // `#group` with a folder name turns on and off what it did.
        check_group_steps(&state, name, &steps, &want_toggled[n]).await?;
    }

    // Switch between the characters with saves in between.
    let start = rng.below(names.len());
    let state = launch_as(dir, &names[start]).await;
    for step in 0..4 {
        let n = rng.below(names.len());
        crate::profile::switch::switch_profile(&state, &state.selected_session(), &names[n])
            .await
            .map_err(|e| format!("switch: {e}"))?;
        check(
            &state,
            &names[n],
            &format!("after switch {step}"),
            &before[n],
            &want[n],
        )
        .await?;
        if rng.chance(60) {
            save(&state).await;
        }
    }
    save(&state).await;
    drop(state);

    // And once more from a fresh launch as each, and after a switch to
    // each, where `#group` follows the folder map the saves kept.
    for (n, name) in names.iter().enumerate() {
        let state = launch_as(dir, name).await;
        check(&state, name, "at the last launch", &before[n], &want[n]).await?;
        check_group_steps(&state, name, &steps, &want_toggled[n]).await?;
        let state = launch_as(dir, &names[(n + 1) % names.len()]).await;
        crate::profile::switch::switch_profile(&state, &state.selected_session(), name)
            .await
            .map_err(|e| format!("switch: {e}"))?;
        check_group_steps(&state, name, &steps, &want_toggled[n]).await?;
    }
    Ok(())
}

/// Run `steps` as `name`, and check what is on after each against
/// `want`.
async fn check_group_steps(
    state: &SharedState,
    name: &str,
    steps: &[(&str, bool)],
    want: &[Vec<String>],
) -> Result<(), String> {
    let toggled = run_group_steps(state, steps).await;
    for (step, rows) in toggled.iter().enumerate() {
        let (folder, on) = steps[step];
        diff(
            name,
            &format!("after #group {folder} {}", if on { "on" } else { "off" }),
            rows,
            &want[step],
        )?;
    }
    Ok(())
}

/// Threads the sets spread over, each with a runtime of its own.
const THREADS: u64 = 4;

#[test]
fn every_character_keeps_what_it_had_on_after_the_wizard() {
    let mut failures: Vec<(u64, String)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..THREADS)
            .map(|first| {
                scope.spawn(move || {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();
                    runtime.block_on(async {
                        let mut failed = Vec::new();
                        for seed in (first..SETS).step_by(THREADS as usize) {
                            if let Err(e) = round_trip(seed).await {
                                failed.push((seed, e));
                            }
                        }
                        failed
                    })
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap())
            .collect()
    });
    failures.sort_by_key(|(seed, _)| *seed);
    if !failures.is_empty() {
        let first: Vec<String> = failures
            .iter()
            .take(5)
            .map(|(seed, e)| format!("seed {seed}: {e}"))
            .collect();
        panic!(
            "{} of {SETS} sets failed. Seeds {:?}\n{}",
            failures.len(),
            failures
                .iter()
                .map(|(s, _)| *s)
                .take(20)
                .collect::<Vec<_>>(),
            first.join("\n")
        );
    }
}
