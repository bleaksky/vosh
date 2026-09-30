//! How full each affect was when you last cast it, per character, so
//! the Affects pane can drain a gauge as the hours run down (the
//! Countdown meter and the Grouped chips).
//!
//! Char.Affects sends only the hours left (`duration`, -1 permanent),
//! and the game keeps nothing more (`affect_data` holds only `duration`,
//! merc.h 1939, and `gmcp_send_affects` sends it as is, gmcp.c 654). So
//! full is the most hours Vosh has seen for the affect since it was last
//! cast:
//!
//! - an affect seen for the first time starts full at the hours it shows,
//!   or at what the character's saved map says, whichever is more;
//! - hours that rise since the previous list are a recast, and full
//!   starts over at them;
//! - otherwise full keeps the most it has seen, so a gauge never passes
//!   one;
//! - a permanent affect, or one with no hours, has no full;
//! - an affect missing from a list has ended, and its full goes.
//!
//! A list the game hides (`"hidden": true` under lamented tears) changes
//! nothing and does not count as the previous list, so a recast under
//! the song still reads as a rise once the list comes back.
//!
//! Each character's map is kept across logins in
//! `<app_data_dir>/affect_full.toml`, keyed by `{host}:{port} {name}`.
//! The file is written only when the map would change what it holds, a
//! moment after a burst of changes settles, never on a tick and never
//! for a login that finds the fulls it saved. It is a cache, so it keeps
//! no backups, and a file Vosh cannot read is left alone while the store
//! runs in memory for the session.
//!
//! This module is the one place that decides full. A server field for
//! the cast length would be read here first, with the peak rule as the
//! fallback for older builds, and the frontend would not change.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde_json::Value;
use tauri::{AppHandle, Manager, State};
use tracing::warn;

use crate::affects_snapshot::AFFECTS_PACKAGE;
use crate::commands::SharedState;

/// Sent to every window with the whole map whenever it changes.
pub(crate) const AFFECT_FULL_CHANGED_EVENT: &str = "vosh://affect-full-changed";

/// The file under the app data folder.
pub(crate) const FILE_NAME: &str = "affect_full.toml";

/// The shape this build writes.
const FILE_VERSION: i64 = 1;

/// How long the write waits for a burst of changes, a round of buffs
/// cast one after another, to settle.
const WRITE_DEBOUNCE: Duration = Duration::from_secs(2);

/// Hours at full for each affect on the character, by affect key.
pub(crate) type FullMap = BTreeMap<String, i64>;

/// An affect's key: its name in lower case with runs of white space as
/// one space, trimmed. The same as `normalizeAffectName` in the
/// frontend, which keys the pane's rows.
pub(crate) fn affect_key(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The store's key for a character on a world.
pub(crate) fn character_key(host: &str, port: u16, character: &str) -> String {
    format!("{host}:{port} {}", affect_key(character))
}

/// How long an affect in a list lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hours {
    Timed(i64),
    Permanent,
    Unknown,
}

impl Hours {
    fn of(value: Option<&Value>) -> Self {
        let n = match value {
            Some(Value::Number(n)) => n.as_f64(),
            Some(Value::String(s)) => s.trim().parse::<f64>().ok(),
            _ => None,
        };
        match n {
            Some(n) if n.is_finite() && n < 0.0 => Self::Permanent,
            #[allow(clippy::cast_possible_truncation)]
            Some(n) if n.is_finite() => Self::Timed(n.floor() as i64),
            _ => Self::Unknown,
        }
    }

    /// Permanent outlasts any timed affect, and unknown loses to both.
    fn lasting(self) -> i64 {
        match self {
            Self::Permanent => i64::MAX,
            Self::Timed(h) => h,
            Self::Unknown => -1,
        }
    }
}

/// The affects in a Char.Affects payload by key, rows that share a key
/// keeping the longest, as the pane does. None for a list the game
/// hides, or a payload that is no list.
fn list_of(data: &Value) -> Option<BTreeMap<String, Hours>> {
    if data.get("hidden").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let rows = match data {
        Value::Array(rows) => rows,
        Value::Object(o) => o.get("affects")?.as_array()?,
        _ => return None,
    };
    let mut out: BTreeMap<String, Hours> = BTreeMap::new();
    for row in rows {
        let Some(name) = row.get("name").and_then(Value::as_str) else {
            continue;
        };
        let key = affect_key(name);
        if key.is_empty() {
            continue;
        }
        let hours = Hours::of(row.get("duration"));
        match out.get(&key) {
            Some(prev) if prev.lasting() >= hours.lasting() => {}
            _ => {
                out.insert(key, hours);
            }
        }
    }
    Some(out)
}

/// The store. One per app, in [`crate::commands::AppState`].
#[derive(Debug, Default)]
pub(crate) struct AffectFull {
    inner: Mutex<Inner>,
    /// Bumped by every change, so only the last write of a burst runs.
    write_gen: AtomicU64,
    /// Files written, for the tests.
    writes: AtomicUsize,
}

#[derive(Debug, Default)]
struct Inner {
    /// The file, set at launch. None writes nothing, as in tests that
    /// do not set one.
    path: Option<PathBuf>,
    /// The file did not read this session, so nothing is written over it.
    unreadable: bool,
    /// The logged in character's key, None until the game names it.
    character: Option<String>,
    full: FullMap,
    /// Hours of each timed affect in the previous list that was not
    /// hidden. None before the first list since you connected.
    last: Option<BTreeMap<String, i64>>,
    /// The character's saved map, held for the first list when the game
    /// names the character before it sends one.
    pending: Option<FullMap>,
    /// What the file holds for the character, as read when the game named
    /// it or as last written. None before the name, and after a write
    /// that failed, so the next flush tries again.
    stored: Option<FullMap>,
}

/// One write of a character's map.
#[derive(Debug)]
struct WriteJob {
    path: PathBuf,
    character: String,
    full: FullMap,
}

impl AffectFull {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Keep maps in `path` from now on.
    pub(crate) fn set_path(&self, path: PathBuf) {
        self.lock().path = Some(path);
    }

    /// The live map.
    pub(crate) fn map(&self) -> FullMap {
        self.lock().full.clone()
    }

    /// How many files this store wrote.
    #[cfg(test)]
    pub(crate) fn writes(&self) -> usize {
        self.writes.load(Ordering::Acquire)
    }

    /// Take in a GMCP packet. Returns the new map when a Char.Affects
    /// list changed it.
    pub(crate) fn observe(&self, package: &str, data: &Value) -> Option<FullMap> {
        if package != AFFECTS_PACKAGE {
            return None;
        }
        let list = list_of(data)?;
        let mut inner = self.lock();
        let before = inner.full.clone();
        let pending = inner.pending.take();
        let last = inner.last.take().unwrap_or_default();
        let mut full = FullMap::new();
        let mut hours_now = BTreeMap::new();
        for (key, hours) in list {
            let Hours::Timed(h) = hours else {
                continue;
            };
            hours_now.insert(key.clone(), h);
            let known = before.get(&key).copied();
            let value = match known {
                // A rise since the previous list is a recast.
                Some(_) if last.get(&key).is_some_and(|&prev| h > prev) => h,
                Some(top) => top.max(h),
                // First seen: the saved map says how full it was cast,
                // unless the hours on you are more, a newer cast.
                None => pending
                    .as_ref()
                    .and_then(|saved| saved.get(&key))
                    .map_or(h, |&saved| saved.max(h)),
            };
            full.insert(key, value);
        }
        inner.last = Some(hours_now);
        if full == before {
            return None;
        }
        inner.full.clone_from(&full);
        Some(full)
    }

    /// The game named the character, `key` from [`character_key`], and
    /// `saved` is its map from the file. When a list already came, the
    /// affects on you take the saved fulls where they are more, at once.
    /// Else the saved map waits for the first list. Either way affects
    /// not on you at login drop, and the result is the same. Returns the
    /// new map when it changed. A different character on the same
    /// connection starts over, after the previous one's map is written.
    fn character_known_with(&self, key: String, saved: FullMap) -> Option<FullMap> {
        let mut inner = self.lock();
        if inner.character.as_deref() == Some(key.as_str()) {
            return None;
        }
        let before = inner.full.clone();
        if inner.character.is_some() {
            inner.full.clear();
            inner.last = None;
        }
        inner.character = Some(key);
        inner.stored = Some(saved.clone());
        if inner.last.is_some() {
            // The map on you now belongs to this character, and the next
            // flush writes it where it differs from the file.
            for (affect, top) in &mut inner.full {
                if let Some(&stored) = saved.get(affect) {
                    *top = (*top).max(stored);
                }
            }
            inner.pending = None;
        } else {
            inner.pending = Some(saved);
        }
        (inner.full != before).then(|| inner.full.clone())
    }

    /// [`Self::character_known_with`] with the map the file holds.
    pub(crate) fn character_known(&self, key: String) -> Option<FullMap> {
        {
            let inner = self.lock();
            if inner.character.as_deref() == Some(key.as_str()) {
                return None;
            }
        }
        // A character that changes on one connection writes first.
        if self.lock().character.is_some() {
            self.flush();
        }
        let saved = self.read_saved(&key);
        self.character_known_with(key, saved)
    }

    /// Forget the connection's state, as a new connection starts.
    /// Returns true when a map was showing, so the windows hear it empty.
    pub(crate) fn connect(&self) -> bool {
        let mut inner = self.lock();
        let had = !inner.full.is_empty();
        let path = inner.path.take();
        let unreadable = inner.unreadable;
        *inner = Inner {
            path,
            unreadable,
            ..Inner::default()
        };
        had
    }

    /// The connection ended: write the character's map, then forget the
    /// connection's state. Returns true when a map was showing.
    pub(crate) fn disconnect(&self) -> bool {
        self.flush();
        self.connect()
    }

    /// Start a debounced write: returns the ticket [`Self::write_due`]
    /// checks once the wait is over.
    fn schedule(&self) -> u64 {
        self.write_gen.fetch_add(1, Ordering::AcqRel) + 1
    }

    /// Whether `ticket` is still the latest, so its write should run.
    fn write_due(&self, ticket: u64) -> bool {
        self.write_gen.load(Ordering::Acquire) == ticket
    }

    /// Write the character's map now when it differs from what the file
    /// holds. Nothing is written before the game names the character.
    /// Returns whether a file was written.
    pub(crate) fn flush(&self) -> bool {
        let Some(job) = self.take_write() else {
            return false;
        };
        match write_character(&job) {
            Ok(()) => {
                self.writes.fetch_add(1, Ordering::AcqRel);
                true
            }
            Err(WriteError::Unreadable) => {
                warn!(path = %job.path.display(), "affect_full: the file does not read, so Vosh keeps the fulls in memory");
                self.lock().unreadable = true;
                false
            }
            Err(WriteError::Io(e)) => {
                warn!(error = %e, path = %job.path.display(), "affect_full: write failed");
                // What the file holds is not known now, so the next flush
                // tries again.
                let mut inner = self.lock();
                if inner.character.as_deref() == Some(job.character.as_str()) {
                    inner.stored = None;
                }
                false
            }
        }
    }

    fn take_write(&self) -> Option<WriteJob> {
        let mut inner = self.lock();
        if inner.unreadable {
            return None;
        }
        let path = inner.path.clone()?;
        let character = inner.character.clone()?;
        let full = inner.full.clone();
        if inner.stored.as_ref() == Some(&full) {
            return None;
        }
        inner.stored = Some(full.clone());
        Some(WriteJob {
            path,
            character,
            full,
        })
    }

    /// The map the file keeps for `key`, empty when it keeps none or the
    /// file does not read. A file that does not read is left alone for
    /// the rest of the session.
    fn read_saved(&self, key: &str) -> FullMap {
        let Some(path) = self.lock().path.clone() else {
            return FullMap::new();
        };
        match read_file(&path) {
            Ok(Some(table)) => characters_of(&table)
                .and_then(|characters| characters.get(key))
                .map(map_of)
                .unwrap_or_default(),
            Ok(None) => FullMap::new(),
            Err(()) => {
                warn!(path = %path.display(), "affect_full: the file does not read, so Vosh keeps the fulls in memory");
                self.lock().unreadable = true;
                FullMap::new()
            }
        }
    }
}

/// The file as a table. None when there is no file, and an error when
/// it does not read as TOML.
fn read_file(path: &Path) -> Result<Option<toml::Table>, ()> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(()),
    };
    text.parse::<toml::Table>().map(Some).map_err(|_| ())
}

fn characters_of(table: &toml::Table) -> Option<&toml::Table> {
    table.get("characters").and_then(toml::Value::as_table)
}

/// A character's map from its table. Anything that is not a whole
/// number of hours is skipped.
fn map_of(value: &toml::Value) -> FullMap {
    value
        .as_table()
        .map(|t| {
            t.iter()
                .filter_map(|(k, v)| v.as_integer().map(|n| (k.clone(), n)))
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug)]
enum WriteError {
    Unreadable,
    Io(std::io::Error),
}

/// Read the file, change the one character, and write the whole file
/// through a temporary file and a rename, so the file always holds the
/// old maps or the new ones. Unknown fields and other characters stay as
/// they are. An empty map drops the character.
fn write_character(job: &WriteJob) -> Result<(), WriteError> {
    let mut table = match read_file(&job.path) {
        Ok(Some(table)) => table,
        Ok(None) => toml::Table::new(),
        Err(()) => return Err(WriteError::Unreadable),
    };
    table
        .entry("version")
        .or_insert(toml::Value::Integer(FILE_VERSION));
    let characters = table
        .entry("characters")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    let Some(characters) = characters.as_table_mut() else {
        return Err(WriteError::Unreadable);
    };
    if job.full.is_empty() {
        characters.remove(&job.character);
    } else {
        let map: toml::Table = job
            .full
            .iter()
            .map(|(k, &v)| (k.clone(), toml::Value::Integer(v)))
            .collect();
        characters.insert(job.character.clone(), toml::Value::Table(map));
    }
    let text = toml::to_string(&table).map_err(|e| WriteError::Io(std::io::Error::other(e)))?;
    if let Some(parent) = job.path.parent() {
        std::fs::create_dir_all(parent).map_err(WriteError::Io)?;
    }
    let mut tmp = job.path.clone().into_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let written = std::fs::write(&tmp, text).and_then(|()| std::fs::rename(&tmp, &job.path));
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(WriteError::Io(e));
    }
    Ok(())
}

/// Tell every window the map changed, and write it once the burst
/// settles.
pub(crate) fn changed<R: tauri::Runtime>(app: &AppHandle<R>, map: &FullMap) {
    crate::commands::broadcast(app, AFFECT_FULL_CHANGED_EVENT, map);
    schedule_write(app);
}

/// Write the map once no change has come for [`WRITE_DEBOUNCE`].
pub(crate) fn schedule_write<R: tauri::Runtime>(app: &AppHandle<R>) {
    let state: SharedState = app.state::<SharedState>().inner().clone();
    let ticket = state.affect_full.schedule();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(WRITE_DEBOUNCE).await;
        if !state.affect_full.write_due(ticket) {
            return;
        }
        let _ = tauri::async_runtime::spawn_blocking(move || state.affect_full.flush()).await;
    });
}

/// Take in a GMCP packet on the session loop: a Char.Affects list that
/// changes the map goes out to every window. The caller sends the list
/// itself after this, so the windows never draw it against the old map.
pub(crate) fn observe<R: tauri::Runtime>(app: &AppHandle<R>, package: &str, data: &Value) {
    let state = app.state::<SharedState>();
    if let Some(map) = state.affect_full.observe(package, data) {
        changed(app, &map);
    }
}

/// The game named the character on the live connection.
pub(crate) fn character_known<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    character: &str,
) {
    let Some((host, port)) = state.current_connection.lock().ok().and_then(|g| g.clone()) else {
        return;
    };
    let key = character_key(&host, port, character);
    if let Some(map) = state.affect_full.character_known(key) {
        changed(app, &map);
    } else {
        // A map built before the name came belongs to it now.
        schedule_write(app);
    }
}

/// A new connection: nothing shows until its first list.
pub(crate) fn connect<R: tauri::Runtime>(app: &AppHandle<R>, state: &SharedState) {
    if state.affect_full.connect() {
        crate::commands::broadcast(app, AFFECT_FULL_CHANGED_EVENT, &FullMap::new());
    }
}

/// The connection ended: write the map, then clear it everywhere.
pub(crate) fn disconnect<R: tauri::Runtime>(app: &AppHandle<R>, state: &SharedState) {
    if state.affect_full.disconnect() {
        crate::commands::broadcast(app, AFFECT_FULL_CHANGED_EVENT, &FullMap::new());
    }
}

/// The live map, hours at full by affect key.
#[tauri::command]
pub(crate) async fn affect_full_get(state: State<'_, SharedState>) -> Result<FullMap, String> {
    Ok(state.affect_full.map())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn list(affects: &[(&str, i64)]) -> Value {
        json!({
            "affects": affects
                .iter()
                .map(|(name, duration)| json!({ "kind": "spell", "name": name, "duration": duration }))
                .collect::<Vec<_>>()
        })
    }

    fn seen(store: &AffectFull, affects: &[(&str, i64)]) -> Option<FullMap> {
        store.observe(AFFECTS_PACKAGE, &list(affects))
    }

    fn map(entries: &[(&str, i64)]) -> FullMap {
        entries
            .iter()
            .map(|(k, v)| ((*k).to_string(), *v))
            .collect()
    }

    /// A store over its own temporary folder, never app data.
    fn store_in(dir: &tempfile::TempDir) -> AffectFull {
        let store = AffectFull::default();
        store.set_path(dir.path().join(FILE_NAME));
        store
    }

    const ERELEI: &str = "aabahran.com:4000 erelei";

    #[test]
    fn an_affect_first_seen_starts_full_at_its_hours() {
        let store = AffectFull::default();
        assert_eq!(
            seen(&store, &[("armor", 31), ("sanctuary", 9)]),
            Some(map(&[("armor", 31), ("sanctuary", 9)]))
        );
    }

    #[test]
    fn falling_hours_keep_full_and_a_tick_that_changes_nothing_sends_nothing() {
        let store = AffectFull::default();
        seen(&store, &[("armor", 48)]);
        assert_eq!(seen(&store, &[("armor", 47)]), None);
        assert_eq!(seen(&store, &[("armor", 46)]), None);
        assert_eq!(store.map(), map(&[("armor", 48)]));
    }

    #[test]
    fn a_rise_is_a_recast_and_starts_full_over() {
        let store = AffectFull::default();
        seen(&store, &[("sanctuary", 10)]);
        seen(&store, &[("sanctuary", 1)]);
        assert_eq!(seen(&store, &[("sanctuary", 10)]), None, "the same full");
        seen(&store, &[("armor", 48)]);
        seen(&store, &[("armor", 48)]);
        // Recast at a lower level: 48 down to 40 would keep 48 without
        // the rise rule, but 30 rising to 40 is a recast.
        seen(&store, &[("armor", 30)]);
        assert_eq!(seen(&store, &[("armor", 40)]), Some(map(&[("armor", 40)])));
    }

    #[test]
    fn an_ended_affect_is_forgotten_and_its_next_cast_starts_fresh() {
        let store = AffectFull::default();
        seen(&store, &[("bless", 48), ("armor", 20)]);
        assert_eq!(seen(&store, &[("armor", 19)]), Some(map(&[("armor", 20)])));
        assert_eq!(
            seen(&store, &[("armor", 18), ("bless", 40)]),
            Some(map(&[("armor", 20), ("bless", 40)]))
        );
    }

    #[test]
    fn permanent_and_unknown_affects_have_no_full() {
        let store = AffectFull::default();
        let data = json!({ "affects": [
            { "name": "mounted", "duration": -1 },
            { "name": "detect magic" },
            { "name": "haste", "duration": "14" },
        ]});
        assert_eq!(
            store.observe(AFFECTS_PACKAGE, &data),
            Some(map(&[("haste", 14)]))
        );
        // A timed affect that turns permanent loses its full.
        seen(&store, &[("haste", -1)]);
        assert!(store.map().is_empty());
    }

    #[test]
    fn a_hidden_list_changes_nothing_and_a_recast_under_it_reads_as_a_rise() {
        let store = AffectFull::default();
        seen(&store, &[("fly", 53)]);
        seen(&store, &[("fly", 3)]);
        let hidden = json!({ "affects": [], "hidden": true });
        assert_eq!(store.observe(AFFECTS_PACKAGE, &hidden), None);
        assert_eq!(store.map(), map(&[("fly", 53)]));
        // You recast fly under lamented tears at a lower level.
        assert_eq!(seen(&store, &[("fly", 40)]), Some(map(&[("fly", 40)])));
    }

    #[test]
    fn rows_that_share_a_name_keep_the_longest() {
        let store = AffectFull::default();
        let data = json!({ "affects": [
            { "name": "Bless", "duration": 6, "location": "hitroll" },
            { "name": "bless", "duration": 9, "location": "saves" },
            { "name": "giant  strength", "duration": 3 },
            { "name": "giant strength", "duration": -1 },
        ]});
        assert_eq!(
            store.observe(AFFECTS_PACKAGE, &data),
            Some(map(&[("bless", 9)]))
        );
    }

    #[test]
    fn other_packages_leave_it_alone() {
        let store = AffectFull::default();
        assert_eq!(store.observe("Char.Vitals", &json!({ "hp": 100 })), None);
        assert_eq!(store.observe(AFFECTS_PACKAGE, &json!({ "nope": 1 })), None);
        assert!(store.map().is_empty());
    }

    #[test]
    fn the_saved_map_merges_by_max_whether_the_name_comes_before_or_after_the_first_list() {
        let saved = map(&[("armor", 48), ("sanctuary", 5), ("fly", 53)]);
        // The name first, then the list.
        let before = AffectFull::default();
        assert_eq!(
            before.character_known_with(ERELEI.into(), saved.clone()),
            None
        );
        let first = seen(&before, &[("armor", 31), ("sanctuary", 9)]);
        // The list first, then the name.
        let after = AffectFull::default();
        seen(&after, &[("armor", 31), ("sanctuary", 9)]);
        let merged = after.character_known_with(ERELEI.into(), saved);
        // Armor continues the cast you left at 48. Sanctuary at 9 is
        // more than the 5 saved, a newer cast. Fly is not on you and
        // drops.
        let want = map(&[("armor", 48), ("sanctuary", 9)]);
        assert_eq!(first, Some(want.clone()));
        assert_eq!(merged, Some(want.clone()));
        assert_eq!(before.map(), want);
        assert_eq!(after.map(), want);
    }

    #[test]
    fn the_saved_map_counts_only_for_the_first_list() {
        let store = AffectFull::default();
        store.character_known_with(ERELEI.into(), map(&[("fly", 53)]));
        seen(&store, &[("armor", 31)]);
        // Fly cast later in the session starts at its own hours.
        assert_eq!(
            seen(&store, &[("armor", 30), ("fly", 50)]),
            Some(map(&[("armor", 31), ("fly", 50)]))
        );
    }

    #[test]
    fn nothing_is_written_before_the_character_is_known() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        seen(&store, &[("armor", 48)]);
        assert!(!store.flush());
        assert!(!dir.path().join(FILE_NAME).exists());
        // Once the game names the character, the map built so far is its.
        store.character_known(ERELEI.into());
        assert!(store.flush());
        assert_eq!(store.writes(), 1);
        let text = std::fs::read_to_string(dir.path().join(FILE_NAME)).unwrap();
        assert!(text.contains("version = 1"), "{text}");
        assert!(text.contains("armor = 48"), "{text}");
    }

    #[test]
    fn a_tick_with_no_change_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        store.character_known(ERELEI.into());
        seen(&store, &[("armor", 48)]);
        assert!(store.flush());
        for hours in (40..48).rev() {
            seen(&store, &[("armor", hours)]);
            assert!(!store.flush(), "a tick at {hours} writes nothing");
        }
        assert_eq!(store.writes(), 1);
    }

    #[test]
    fn a_login_that_changes_no_full_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        let text = "version = 1\n\n[characters.\"aabahran.com:4000 erelei\"]\narmor = 48\n";
        // The name first, then the list, and the list first, then the name.
        for name_first in [true, false] {
            std::fs::write(&path, text).unwrap();
            let store = store_in(&dir);
            if name_first {
                store.character_known(ERELEI.into());
                seen(&store, &[("armor", 31)]);
            } else {
                seen(&store, &[("armor", 31)]);
                store.character_known(ERELEI.into());
            }
            assert_eq!(store.map(), map(&[("armor", 48)]));
            assert!(!store.flush(), "the file holds these fulls already");
            assert!(store.disconnect(), "a map was showing");
            assert_eq!(store.writes(), 0, "nor does logging out write");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        }
    }

    #[test]
    fn a_login_drops_the_saved_fulls_of_affects_not_on_you() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        std::fs::write(
            &path,
            "version = 1\n\n[characters.\"aabahran.com:4000 erelei\"]\narmor = 48\nfly = 53\n",
        )
        .unwrap();
        let store = store_in(&dir);
        store.character_known(ERELEI.into());
        seen(&store, &[("armor", 31)]);
        assert!(store.flush());
        let table: toml::Table = std::fs::read_to_string(&path).unwrap().parse().unwrap();
        assert_eq!(map_of(&table["characters"][ERELEI]), map(&[("armor", 48)]));
        // A login with nothing on you drops the character.
        let next = store_in(&dir);
        next.character_known(ERELEI.into());
        assert_eq!(seen(&next, &[]), None, "the pane had nothing to show");
        assert!(next.flush());
        let table: toml::Table = std::fs::read_to_string(&path).unwrap().parse().unwrap();
        assert!(!table["characters"].as_table().unwrap().contains_key(ERELEI));
    }

    #[test]
    fn a_round_of_buffs_inside_the_wait_writes_once() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        store.character_known(ERELEI.into());
        let mut tickets = Vec::new();
        for (i, name) in ["armor", "shield", "bless", "sanctuary"].iter().enumerate() {
            let mut affects: Vec<(&str, i64)> = ["armor", "shield", "bless", "sanctuary"][..=i]
                .iter()
                .map(|n| (*n, 24))
                .collect();
            affects.sort_unstable();
            assert!(seen(&store, &affects).is_some(), "{name} changes the map");
            tickets.push(store.schedule());
        }
        let due: Vec<bool> = tickets.iter().map(|&t| store.write_due(t)).collect();
        assert_eq!(due, [false, false, false, true]);
        for &t in &tickets {
            if store.write_due(t) {
                store.flush();
            }
        }
        assert_eq!(store.writes(), 1);
    }

    #[test]
    fn disconnect_writes_then_empties() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        store.character_known(ERELEI.into());
        seen(&store, &[("armor", 48)]);
        assert!(store.disconnect(), "a map was showing");
        assert!(store.map().is_empty());
        assert_eq!(store.writes(), 1);
        // The next login reads it back.
        let next = store_in(&dir);
        next.character_known(ERELEI.into());
        assert_eq!(seen(&next, &[("armor", 31)]), Some(map(&[("armor", 48)])));
        // Another character on the same world keeps its own.
        let other = store_in(&dir);
        other.character_known("aabahran.com:4000 vanek".into());
        assert_eq!(seen(&other, &[("armor", 31)]), Some(map(&[("armor", 31)])));
    }

    #[test]
    fn the_file_keeps_other_characters_and_fields_it_does_not_know() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        std::fs::write(
            &path,
            "version = 1\nnote = \"kept\"\n\n[characters.\"aabahran.com:4000 vanek\"]\nhaste = 26\n\n[characters.\"aabahran.com:4000 erelei\"]\narmor = 12\n\"stone skin\" = \"fifty\"\n",
        )
        .unwrap();
        let store = store_in(&dir);
        store.character_known(ERELEI.into());
        // The saved armor is 12, less than the 31 on you: a newer cast.
        assert_eq!(
            seen(&store, &[("armor", 31), ("stone skin", 50)]),
            Some(map(&[("armor", 31), ("stone skin", 50)]))
        );
        assert!(store.flush());
        let table: toml::Table = std::fs::read_to_string(&path).unwrap().parse().unwrap();
        assert_eq!(table["note"].as_str(), Some("kept"));
        let characters = table["characters"].as_table().unwrap();
        assert_eq!(
            characters["aabahran.com:4000 vanek"]["haste"].as_integer(),
            Some(26)
        );
        assert_eq!(
            map_of(&characters[ERELEI]),
            map(&[("armor", 31), ("stone skin", 50)])
        );
        // An empty map drops the character.
        seen(&store, &[]);
        assert!(store.flush());
        let table: toml::Table = std::fs::read_to_string(&path).unwrap().parse().unwrap();
        assert!(!table["characters"].as_table().unwrap().contains_key(ERELEI));
    }

    #[test]
    fn a_file_that_does_not_read_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        std::fs::write(&path, "this is [not toml").unwrap();
        let store = store_in(&dir);
        store.character_known(ERELEI.into());
        assert_eq!(seen(&store, &[("armor", 31)]), Some(map(&[("armor", 31)])));
        assert!(!store.flush());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "this is [not toml");
        // The store still works in memory.
        assert_eq!(store.map(), map(&[("armor", 31)]));
    }

    #[test]
    fn another_character_on_the_same_connection_writes_the_first_and_starts_over() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        store.character_known(ERELEI.into());
        seen(&store, &[("armor", 48)]);
        store.character_known("aabahran.com:4000 vanek".into());
        assert_eq!(store.writes(), 1, "Erelei's fulls are written first");
        assert!(store.map().is_empty());
        assert_eq!(seen(&store, &[("armor", 20)]), Some(map(&[("armor", 20)])));
        let text = std::fs::read_to_string(dir.path().join(FILE_NAME)).unwrap();
        assert!(text.contains("armor = 48"), "{text}");
    }

    #[test]
    fn a_new_connection_forgets_the_old_one() {
        let store = AffectFull::default();
        store.character_known_with(ERELEI.into(), FullMap::new());
        seen(&store, &[("armor", 48)]);
        assert!(store.connect());
        assert!(store.map().is_empty());
        assert!(!store.connect(), "nothing was showing");
        // The first list after it is first seen again.
        assert_eq!(seen(&store, &[("armor", 12)]), Some(map(&[("armor", 12)])));
    }

    #[test]
    fn keys_match_the_pane() {
        assert_eq!(affect_key("  Stone   Skin "), "stone skin");
        assert_eq!(
            character_key("aabahran.com", 4000, "Erelei"),
            "aabahran.com:4000 erelei"
        );
    }
}
