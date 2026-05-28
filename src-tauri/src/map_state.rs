//! Map storage state for the active session.
//!
//! Wraps the disk-backed [`MapStore`] and tracks the current and previous
//! room ids so the session loop can position newly-seen rooms relative to
//! where the player just walked from.

use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use tracing::warn;
use vosh_gmcp::Message;
use vosh_map::{direction, MapError, MapStore, Room};

#[derive(Debug)]
pub(crate) struct MapState {
    pub(crate) store: MapStore,
    pub(crate) current_room_id: Option<i64>,
    pub(crate) previous_room_id: Option<i64>,
}

impl MapState {
    pub(crate) fn new(store: MapStore) -> Self {
        Self {
            store,
            current_room_id: None,
            previous_room_id: None,
        }
    }
}

/// Per-app shared map state. `None` until the Tauri setup callback opens
/// the `SQLite` store.
pub(crate) type SharedMap = Arc<Mutex<Option<MapState>>>;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MapPayload {
    pub current_room_id: Option<i64>,
    pub area: Option<String>,
}

/// Process a GMCP `Room.Info` message into the map store. Returns the
/// payload that should be emitted on `session://map`, if any.
///
/// Aabahran's Room.Info object uses `name`, `area`, `num`, `terrain`, and
/// `exits` fields; this routine reads what is present and stores defaults
/// for the rest.
pub(crate) async fn handle_room_info(
    app: &AppHandle,
    map: &SharedMap,
    msg: &Message,
) -> Result<(), MapError> {
    if msg.package != "Room.Info" {
        return Ok(());
    }
    let Some(obj) = msg.data.as_object() else {
        return Ok(());
    };
    let Some(id) = obj
        .get("num")
        .or_else(|| obj.get("id"))
        .and_then(value_as_i64)
    else {
        return Ok(());
    };
    let area = obj
        .get("area")
        .and_then(value_as_string)
        .unwrap_or_default();
    let name = obj
        .get("name")
        .and_then(value_as_string)
        .unwrap_or_default();
    let terrain = obj
        .get("terrain")
        .and_then(value_as_string)
        .unwrap_or_default();
    let exits = obj
        .get("exits")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| value_as_i64(v).map(|to| (k.clone(), to)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut guard = map.lock().await;
    let Some(state) = guard.as_mut() else {
        return Ok(());
    };

    let existing = state.store.get_room(id)?;
    let (x, y, z) = if let Some(room) = &existing {
        (room.x, room.y, room.z)
    } else {
        infer_position(state, id)?
    };

    state.store.upsert_room(&Room {
        id,
        area: area.clone(),
        name,
        terrain,
        x,
        y,
        z,
        notes: existing
            .as_ref()
            .map(|r| r.notes.clone())
            .unwrap_or_default(),
        avoid: existing.as_ref().is_some_and(|r| r.avoid),
    })?;
    state.store.set_exits(id, &exits)?;

    if state.current_room_id != Some(id) {
        state.previous_room_id = state.current_room_id;
        state.current_room_id = Some(id);
    }

    let payload = MapPayload {
        current_room_id: Some(id),
        area: Some(area),
    };
    drop(guard);
    if let Err(e) = app.emit("session://map", payload) {
        warn!(error = %e, "failed to emit map payload");
    }
    Ok(())
}

fn infer_position(state: &MapState, new_id: i64) -> Result<(i32, i32, i32), MapError> {
    let Some(prev_id) = state.current_room_id else {
        return Ok((0, 0, 0));
    };
    let Some(prev_room) = state.store.get_room(prev_id)? else {
        return Ok((0, 0, 0));
    };
    for exit in state.store.exits_from(prev_id)? {
        if exit.to_room != new_id {
            continue;
        }
        if let Some((_, offset)) = direction::classify(&exit.direction) {
            return Ok((
                prev_room.x + offset.dx,
                prev_room.y + offset.dy,
                prev_room.z + offset.dz,
            ));
        }
    }
    Ok((prev_room.x, prev_room.y, prev_room.z))
}

fn value_as_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn value_as_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}
