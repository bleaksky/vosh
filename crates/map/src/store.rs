//! SQLite-backed storage for rooms and exits.
//!
//! Schema is intentionally narrow for Phase 7: rooms hold identity,
//! coordinates, and a free-form note plus an avoid flag; exits hold
//! directed edges between rooms with a string direction. Phase 11 will
//! add tags, color hints, and per-area metadata as the editor UI grows.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MapError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Room {
    pub id: i64,
    pub area: String,
    pub name: String,
    pub terrain: String,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub notes: String,
    pub avoid: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exit {
    pub from_room: i64,
    pub direction: String,
    pub to_room: i64,
}

pub struct MapStore {
    conn: Connection,
}

impl std::fmt::Debug for MapStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MapStore").finish()
    }
}

impl MapStore {
    pub fn open(path: &Path) -> Result<Self, MapError> {
        let conn = Connection::open(path)?;
        Self::with_connection(conn)
    }

    pub fn open_in_memory() -> Result<Self, MapError> {
        let conn = Connection::open_in_memory()?;
        Self::with_connection(conn)
    }

    fn with_connection(conn: Connection) -> Result<Self, MapError> {
        // Match the log store's Phase 2 fsync-cost reduction: WAL +
        // synchronous=NORMAL turns the per-room-upsert fsync into a
        // per-checkpoint cost. Room.Info pushes fire 1-10/sec while
        // the player is exploring, each doing an `upsert_room` and a
        // `set_exits` transaction; the old default forced a full
        // fsync per statement. WAL also produces `<db>-wal` and
        // `<db>-shm` sidecar files alongside the main `.db` — SQLite
        // manages them transparently and folds them back at clean
        // shutdown. Existing map databases open in WAL without any
        // migration. In-memory databases silently report `memory`
        // for `journal_mode`; the call still succeeds.
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS rooms (
                id INTEGER PRIMARY KEY,
                area TEXT NOT NULL DEFAULT '',
                name TEXT NOT NULL DEFAULT '',
                terrain TEXT NOT NULL DEFAULT '',
                x INTEGER NOT NULL DEFAULT 0,
                y INTEGER NOT NULL DEFAULT 0,
                z INTEGER NOT NULL DEFAULT 0,
                notes TEXT NOT NULL DEFAULT '',
                avoid INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS rooms_area_idx ON rooms (area);

            CREATE TABLE IF NOT EXISTS exits (
                from_room INTEGER NOT NULL,
                direction TEXT NOT NULL,
                to_room INTEGER NOT NULL,
                PRIMARY KEY (from_room, direction)
            );
            CREATE INDEX IF NOT EXISTS exits_to_idx ON exits (to_room);",
        )?;
        Ok(Self { conn })
    }

    /// Insert or update a room. The position fields are honored exactly.
    /// Caller is responsible for choosing coordinates when adding a room
    /// for the first time.
    pub fn upsert_room(&mut self, room: &Room) -> Result<(), MapError> {
        self.conn.execute(
            "INSERT INTO rooms (id, area, name, terrain, x, y, z, notes, avoid)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                 area = excluded.area,
                 name = excluded.name,
                 terrain = excluded.terrain,
                 notes = excluded.notes,
                 avoid = excluded.avoid",
            params![
                room.id,
                room.area,
                room.name,
                room.terrain,
                room.x,
                room.y,
                room.z,
                room.notes,
                i64::from(room.avoid),
            ],
        )?;
        Ok(())
    }

    /// Set or move a room's position. Used when the auto-positioner picks
    /// new coordinates for a room we have not seen before.
    pub fn set_position(&mut self, id: i64, x: i32, y: i32, z: i32) -> Result<(), MapError> {
        self.conn.execute(
            "UPDATE rooms SET x = ?2, y = ?3, z = ?4 WHERE id = ?1",
            params![id, x, y, z],
        )?;
        Ok(())
    }

    pub fn get_room(&self, id: i64) -> Result<Option<Room>, MapError> {
        let row = self
            .conn
            .query_row(
                "SELECT id, area, name, terrain, x, y, z, notes, avoid
                 FROM rooms WHERE id = ?1",
                params![id],
                row_to_room,
            )
            .optional()?;
        Ok(row)
    }

    pub fn list_area(&self, area: &str) -> Result<Vec<Room>, MapError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, area, name, terrain, x, y, z, notes, avoid
             FROM rooms WHERE area = ?1 ORDER BY id",
        )?;
        let rooms = stmt
            .query_map(params![area], row_to_room)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rooms)
    }

    /// Replace the exit set leaving `from_room`. Easier than diffing on each
    /// Room.Info push and the cost is negligible.
    pub fn set_exits(&mut self, from_room: i64, exits: &[(String, i64)]) -> Result<(), MapError> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM exits WHERE from_room = ?1", params![from_room])?;
        {
            let mut stmt = tx
                .prepare("INSERT INTO exits (from_room, direction, to_room) VALUES (?1, ?2, ?3)")?;
            for (dir, to) in exits {
                stmt.execute(params![from_room, dir, to])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn exits_from(&self, from_room: i64) -> Result<Vec<Exit>, MapError> {
        let mut stmt = self
            .conn
            .prepare("SELECT from_room, direction, to_room FROM exits WHERE from_room = ?1")?;
        let rows = stmt
            .query_map(params![from_room], |r| {
                Ok(Exit {
                    from_room: r.get(0)?,
                    direction: r.get(1)?,
                    to_room: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn exits_in_area(&self, area: &str) -> Result<Vec<Exit>, MapError> {
        let mut stmt = self.conn.prepare(
            "SELECT e.from_room, e.direction, e.to_room
             FROM exits e
             JOIN rooms r ON r.id = e.from_room
             WHERE r.area = ?1",
        )?;
        let rows = stmt
            .query_map(params![area], |r| {
                Ok(Exit {
                    from_room: r.get(0)?,
                    direction: r.get(1)?,
                    to_room: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn set_note(&mut self, id: i64, notes: &str) -> Result<(), MapError> {
        self.conn.execute(
            "UPDATE rooms SET notes = ?2 WHERE id = ?1",
            params![id, notes],
        )?;
        Ok(())
    }

    pub fn set_avoid(&mut self, id: i64, avoid: bool) -> Result<(), MapError> {
        self.conn.execute(
            "UPDATE rooms SET avoid = ?2 WHERE id = ?1",
            params![id, i64::from(avoid)],
        )?;
        Ok(())
    }

    /// Find a path of directions from `from` to `to`. BFS over the exit
    /// graph; rooms with `avoid` set are skipped unless they are the start
    /// or end. Returns `None` when no path exists.
    pub fn find_path(&self, from: i64, to: i64) -> Result<Option<Vec<String>>, MapError> {
        if from == to {
            return Ok(Some(Vec::new()));
        }
        let avoid_rooms: HashSet<i64> = self
            .conn
            .prepare("SELECT id FROM rooms WHERE avoid = 1")?
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;

        // parents[room] = (previous_room, direction_taken)
        let mut parents: HashMap<i64, (i64, String)> = HashMap::new();
        let mut queue: VecDeque<i64> = VecDeque::new();
        let mut visited: HashSet<i64> = HashSet::new();
        queue.push_back(from);
        visited.insert(from);

        while let Some(current) = queue.pop_front() {
            for exit in self.exits_from(current)? {
                if visited.contains(&exit.to_room) {
                    continue;
                }
                if avoid_rooms.contains(&exit.to_room) && exit.to_room != to {
                    continue;
                }
                visited.insert(exit.to_room);
                parents.insert(exit.to_room, (current, exit.direction.clone()));
                if exit.to_room == to {
                    return Ok(Some(reconstruct_path(&parents, from, to)));
                }
                queue.push_back(exit.to_room);
            }
        }
        Ok(None)
    }

    pub fn room_count(&self) -> Result<i64, MapError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM rooms", [], |r| r.get(0))?)
    }
}

fn row_to_room(row: &rusqlite::Row<'_>) -> rusqlite::Result<Room> {
    Ok(Room {
        id: row.get(0)?,
        area: row.get(1)?,
        name: row.get(2)?,
        terrain: row.get(3)?,
        x: row.get(4)?,
        y: row.get(5)?,
        z: row.get(6)?,
        notes: row.get(7)?,
        avoid: row.get::<_, i64>(8)? != 0,
    })
}

fn reconstruct_path(parents: &HashMap<i64, (i64, String)>, from: i64, to: i64) -> Vec<String> {
    let mut steps = Vec::new();
    let mut current = to;
    while current != from {
        let (prev, dir) = parents.get(&current).expect("parent must be set");
        steps.push(dir.clone());
        current = *prev;
    }
    steps.reverse();
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(id: i64, area: &str, name: &str, x: i32, y: i32) -> Room {
        Room {
            id,
            area: area.into(),
            name: name.into(),
            terrain: String::new(),
            x,
            y,
            z: 0,
            notes: String::new(),
            avoid: false,
        }
    }

    #[test]
    fn low_fsync_pragmas_are_applied_on_open() {
        // Regression guard for the Phase 3 perf fix. Mirrors the same
        // assertion in `vosh-log` — if the pragmas are dropped,
        // per-Room.Info fsync pressure returns and exploration stalls.
        let s = MapStore::open_in_memory().unwrap();
        let sync: i64 = s
            .conn
            .query_row("PRAGMA synchronous", [], |r| r.get(0))
            .unwrap();
        // 0=OFF, 1=NORMAL, 2=FULL, 3=EXTRA.
        assert_eq!(sync, 1, "synchronous should be NORMAL, got {sync}");
        let mode: String = s
            .conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert!(
            matches!(mode.as_str(), "memory" | "wal"),
            "journal_mode should be WAL on disk (or memory in-memory), got {mode}"
        );
    }

    #[test]
    fn upsert_and_get() {
        let mut s = MapStore::open_in_memory().unwrap();
        s.upsert_room(&r(1, "Town", "Square", 0, 0)).unwrap();
        let room = s.get_room(1).unwrap().unwrap();
        assert_eq!(room.name, "Square");
    }

    #[test]
    fn list_area_returns_only_matching() {
        let mut s = MapStore::open_in_memory().unwrap();
        s.upsert_room(&r(1, "Town", "Square", 0, 0)).unwrap();
        s.upsert_room(&r(2, "Town", "Inn", 1, 0)).unwrap();
        s.upsert_room(&r(3, "Forest", "Glade", 0, 0)).unwrap();
        let town = s.list_area("Town").unwrap();
        assert_eq!(town.len(), 2);
    }

    #[test]
    fn exits_round_trip() {
        let mut s = MapStore::open_in_memory().unwrap();
        s.upsert_room(&r(1, "Town", "Square", 0, 0)).unwrap();
        s.set_exits(1, &[("north".into(), 2), ("east".into(), 3)])
            .unwrap();
        let exits = s.exits_from(1).unwrap();
        assert_eq!(exits.len(), 2);
    }

    #[test]
    fn set_exits_replaces_old() {
        let mut s = MapStore::open_in_memory().unwrap();
        s.upsert_room(&r(1, "Town", "Square", 0, 0)).unwrap();
        s.set_exits(1, &[("north".into(), 2)]).unwrap();
        s.set_exits(1, &[("south".into(), 5)]).unwrap();
        let exits = s.exits_from(1).unwrap();
        assert_eq!(exits.len(), 1);
        assert_eq!(exits[0].direction, "south");
        assert_eq!(exits[0].to_room, 5);
    }

    #[test]
    fn pathfind_simple_chain() {
        let mut s = MapStore::open_in_memory().unwrap();
        for id in 1..=4 {
            s.upsert_room(&r(id, "Town", &format!("R{id}"), id as i32, 0))
                .unwrap();
        }
        s.set_exits(1, &[("east".into(), 2)]).unwrap();
        s.set_exits(2, &[("east".into(), 3)]).unwrap();
        s.set_exits(3, &[("east".into(), 4)]).unwrap();
        let path = s.find_path(1, 4).unwrap().unwrap();
        assert_eq!(path, vec!["east", "east", "east"]);
    }

    #[test]
    fn pathfind_picks_shortest() {
        let mut s = MapStore::open_in_memory().unwrap();
        for id in 1..=4 {
            s.upsert_room(&r(id, "Town", &format!("R{id}"), 0, 0))
                .unwrap();
        }
        s.set_exits(1, &[("east".into(), 2)]).unwrap();
        s.set_exits(2, &[("east".into(), 3)]).unwrap();
        s.set_exits(3, &[("east".into(), 4)]).unwrap();
        s.set_exits(1, &[("east".into(), 2), ("north".into(), 4)])
            .unwrap();
        let path = s.find_path(1, 4).unwrap().unwrap();
        assert_eq!(path, vec!["north"]);
    }

    #[test]
    fn pathfind_skips_avoid_rooms() {
        let mut s = MapStore::open_in_memory().unwrap();
        for id in 1..=4 {
            s.upsert_room(&r(id, "Town", &format!("R{id}"), 0, 0))
                .unwrap();
        }
        s.set_exits(1, &[("east".into(), 2)]).unwrap();
        s.set_exits(2, &[("east".into(), 3)]).unwrap();
        s.set_exits(3, &[("east".into(), 4)]).unwrap();
        s.set_exits(1, &[("east".into(), 2), ("south".into(), 4)])
            .unwrap();
        s.set_avoid(4, true).unwrap();
        // Even with avoid set on the destination we still allow it because
        // it is the explicit target.
        let path = s.find_path(1, 4).unwrap().unwrap();
        assert!(!path.is_empty());
        // But avoid rooms in the middle get skipped.
        s.set_avoid(2, true).unwrap();
        s.set_avoid(4, false).unwrap();
        s.set_exits(1, &[("east".into(), 2), ("south".into(), 3)])
            .unwrap();
        let path = s.find_path(1, 4).unwrap().unwrap();
        assert_eq!(path[0], "south");
    }

    #[test]
    fn pathfind_returns_none_when_disconnected() {
        let mut s = MapStore::open_in_memory().unwrap();
        s.upsert_room(&r(1, "Town", "A", 0, 0)).unwrap();
        s.upsert_room(&r(2, "Town", "B", 0, 0)).unwrap();
        assert!(s.find_path(1, 2).unwrap().is_none());
    }

    #[test]
    fn note_and_avoid_round_trip() {
        let mut s = MapStore::open_in_memory().unwrap();
        s.upsert_room(&r(1, "Town", "Square", 0, 0)).unwrap();
        s.set_note(1, "fountain").unwrap();
        s.set_avoid(1, true).unwrap();
        let room = s.get_room(1).unwrap().unwrap();
        assert_eq!(room.notes, "fountain");
        assert!(room.avoid);
    }
}
