//! The `json` table: `json.encode`, `json.decode`, `json.null` and
//! `json.array`, which every script and plugin reads like `string`.
//!
//! JSON has two things a Lua table cannot tell apart on its own, null
//! and an empty array. `json.null` is a value of its own that stands for
//! null both ways, so an array that holds one keeps its length. An array
//! `json.decode` builds carries a mark, and so does a table you pass to
//! `json.array`, so an empty one encodes back as `[]`. Any other table
//! with the keys 1 to n is an array, and every other table an object.
//!
//! Rust does the work and hands Lua the result. Each walk looks at the
//! clock as the hook would, and the text each side takes or gives has a
//! size limit of [`JSON_BYTES`], since Lua's memory limit never sees what
//! Rust builds.

use std::sync::Arc;

use mlua::{Lua, Table, Value};
use serde_json::{Map, Number, Value as Json};

use crate::limits::{self, Limits, INTERNAL_CHUNK};

/// The most text `json.decode` takes or `json.encode` gives.
pub(crate) const JSON_BYTES: usize = 1024 * 1024;

/// How deep tables may nest, either way.
const JSON_DEPTH: usize = 100;

/// How many values a walk handles between two looks at the clock.
const CLOCK_EVERY: usize = 1000;

/// The registry keys of `json.null` and the mark arrays carry.
const NULL_KEY: &str = "vosh.json.null";
const ARRAY_KEY: &str = "vosh.json.array";

/// The Lua that builds the `json` table. It takes the Rust functions,
/// `json.null` and the array mark, and raises each error at the line
/// that called the function.
const JSON: &str = r#"
local encode, decode, null, array_mark = ...
local error, getmetatable, setmetatable, type = error, getmetatable, setmetatable, type

local function found(ok, ...)
  if ok then
    return ...
  end
  error((...), 2)
end

json = {
  null = null,
  encode = function(...)
    return found(encode(...))
  end,
  decode = function(...)
    return found(decode(...))
  end,
  array = function(t)
    if t == nil then
      t = {}
    elseif type(t) ~= "table" then
      error("bad argument #1 to 'array' (table expected, got " .. type(t) .. ")", 2)
    elseif getmetatable(t) ~= nil and getmetatable(t) ~= array_mark then
      error("json.array cannot mark a table that has a metatable", 2)
    end
    return setmetatable(t, array_mark)
  end,
}
"#;

/// Put the `json` table in the globals.
pub(crate) fn install(lua: &Lua, limits: &Arc<Limits>) -> mlua::Result<()> {
    let null = lua.create_table()?;
    let null_meta = lua.create_table()?;
    null_meta.set("__name", "json.null")?;
    null_meta.set("__tostring", lua.create_function(|_, _: Value| Ok("null"))?)?;
    null_meta.set(
        "__newindex",
        lua.create_function(|_, _: mlua::MultiValue| -> mlua::Result<()> {
            Err(mlua::Error::RuntimeError(
                "json.null cannot be changed".into(),
            ))
        })?,
    )?;
    null_meta.set("__metatable", false)?;
    null.set_metatable(Some(null_meta));
    let array_mark = lua.create_table()?;
    array_mark.set("__name", "json.array")?;
    lua.set_named_registry_value(NULL_KEY, null.clone())?;
    lua.set_named_registry_value(ARRAY_KEY, array_mark.clone())?;

    let encode = {
        let limits = Arc::clone(limits);
        lua.create_function(move |lua, value: Option<Value>| {
            let result = encode(lua, &limits, value.unwrap_or(Value::Nil));
            answer(lua, result.map(|v| mlua::MultiValue::from_iter([v])))
        })?
    };
    let decode = {
        let limits = Arc::clone(limits);
        lua.create_function(move |lua, text: Value| {
            let result = match text {
                Value::String(s) => decode(lua, &limits, &s.as_bytes()),
                other => Err(Failure::Says(format!(
                    "bad argument #1 to 'decode' (string expected, got {})",
                    other.type_name()
                ))),
            };
            answer(lua, result.map(|v| mlua::MultiValue::from_iter([v])))
        })?
    };
    lua.load(JSON)
        .set_name(INTERNAL_CHUNK)
        .call::<()>((encode, decode, null, array_mark))
}

/// Why a call failed: an error for the script, a stop, or a Lua error
/// such as a failed allocation, which goes up as it is.
pub(crate) enum Failure {
    Says(String),
    Stopped,
    Lua(mlua::Error),
}

impl From<mlua::Error> for Failure {
    fn from(e: mlua::Error) -> Self {
        Self::Lua(e)
    }
}

/// What a Rust function hands the Lua wrapper: `true` and the result,
/// or `false` and the error to raise at the caller's line. A stop or a
/// Lua error, such as a failed allocation, ends the call instead.
pub(crate) fn answer(
    lua: &Lua,
    result: Result<mlua::MultiValue, Failure>,
) -> mlua::Result<mlua::MultiValue> {
    match result {
        Ok(mut values) => {
            values.push_front(Value::Boolean(true));
            Ok(values)
        }
        Err(Failure::Says(text)) => Ok(mlua::MultiValue::from_iter([
            Value::Boolean(false),
            Value::String(lua.create_string(text)?),
        ])),
        Err(Failure::Stopped) => Err(limits::stopped()),
        Err(Failure::Lua(e)) => Err(e),
    }
}

/// The `json.null` sentinel and the array mark.
fn markers(lua: &Lua) -> mlua::Result<(Table, Table)> {
    Ok((
        lua.named_registry_value::<Table>(NULL_KEY)?,
        lua.named_registry_value::<Table>(ARRAY_KEY)?,
    ))
}

/// A walk's look at the clock, every [`CLOCK_EVERY`] values.
pub(crate) struct Clock<'a> {
    pub(crate) limits: &'a Limits,
    pub(crate) seen: usize,
}

impl Clock<'_> {
    pub(crate) fn tick(&mut self, lua: &Lua) -> Result<(), Failure> {
        self.seen += 1;
        if self.seen % CLOCK_EVERY == 0 && self.limits.out_of_time(lua) {
            return Err(Failure::Stopped);
        }
        Ok(())
    }
}

fn encode(lua: &Lua, limits: &Limits, value: Value) -> Result<Value, Failure> {
    let (null, array_mark) = markers(lua)?;
    let mut walk = Encoder {
        null,
        array_mark,
        clock: Clock { limits, seen: 0 },
        open: Vec::new(),
    };
    let json = walk.value(lua, &value, 0)?;
    let text = serde_json::to_string(&json).map_err(|e| Failure::Says(e.to_string()))?;
    if text.len() > JSON_BYTES {
        return Err(Failure::Says(format!(
            "json.encode gives at most {} bytes, and this is {}",
            JSON_BYTES,
            text.len()
        )));
    }
    Ok(Value::String(lua.create_string(text)?))
}

struct Encoder<'a> {
    null: Table,
    array_mark: Table,
    clock: Clock<'a>,
    /// The tables the walk is inside now, to find one inside itself.
    open: Vec<*const std::ffi::c_void>,
}

impl Encoder<'_> {
    fn value(&mut self, lua: &Lua, value: &Value, depth: usize) -> Result<Json, Failure> {
        self.clock.tick(lua)?;
        Ok(match value {
            Value::Nil => Json::Null,
            Value::Boolean(b) => Json::Bool(*b),
            Value::Integer(n) => Json::Number(Number::from(*n)),
            Value::Number(n) => match Number::from_f64(*n) {
                Some(n) => Json::Number(n),
                None => {
                    return Err(Failure::Says(format!(
                        "json.encode cannot write the number {n}"
                    )))
                }
            },
            Value::String(s) => Json::String(s.to_string_lossy()),
            Value::Table(t) if t.to_pointer() == self.null.to_pointer() => Json::Null,
            Value::Table(t) => self.table(lua, t, depth)?,
            other => {
                return Err(Failure::Says(format!(
                    "json.encode cannot write a {}",
                    other.type_name()
                )))
            }
        })
    }

    fn table(&mut self, lua: &Lua, t: &Table, depth: usize) -> Result<Json, Failure> {
        if depth >= JSON_DEPTH {
            return Err(Failure::Says(format!(
                "json.encode goes at most {JSON_DEPTH} tables deep"
            )));
        }
        let at = t.to_pointer();
        if self.open.contains(&at) {
            return Err(Failure::Says(
                "json.encode found a table inside itself".into(),
            ));
        }
        self.open.push(at);
        let marked = t
            .metatable()
            .is_some_and(|m| m.to_pointer() == self.array_mark.to_pointer());
        let mut pairs = Vec::new();
        for pair in t.pairs::<Value, Value>() {
            pairs.push(pair?);
        }
        let len = t.raw_len();
        let sequence = pairs.len() == len
            && pairs.iter().all(|(k, _)| match k {
                Value::Integer(i) => *i >= 1 && usize::try_from(*i).is_ok_and(|i| i <= len),
                _ => false,
            });
        let json = if marked || (sequence && len > 0) {
            if !sequence {
                return Err(Failure::Says(
                    "json.encode found a key in an array that is not 1 to n".into(),
                ));
            }
            let mut items = Vec::with_capacity(len);
            for i in 1..=len {
                let item: Value = t.raw_get(i)?;
                items.push(self.value(lua, &item, depth + 1)?);
            }
            Json::Array(items)
        } else {
            let mut object = Map::new();
            for (key, item) in &pairs {
                let key = match key {
                    Value::String(s) => s.to_string_lossy(),
                    Value::Integer(n) => n.to_string(),
                    Value::Number(n) => n.to_string(),
                    other => {
                        return Err(Failure::Says(format!(
                            "json.encode cannot write a key of type {}",
                            other.type_name()
                        )))
                    }
                };
                let item = self.value(lua, item, depth + 1)?;
                object.insert(key, item);
            }
            Json::Object(object)
        };
        self.open.pop();
        Ok(json)
    }
}

fn decode(lua: &Lua, limits: &Limits, text: &[u8]) -> Result<Value, Failure> {
    if text.len() > JSON_BYTES {
        return Err(Failure::Says(format!(
            "json.decode takes at most {} bytes, and this is {}",
            JSON_BYTES,
            text.len()
        )));
    }
    let json: Json = serde_json::from_slice(text)
        .map_err(|e| Failure::Says(format!("json.decode could not read the text: {e}")))?;
    let (null, array_mark) = markers(lua)?;
    let mut clock = Clock { limits, seen: 0 };
    to_lua(lua, &json, &null, &array_mark, &mut clock, 0)
}

fn to_lua(
    lua: &Lua,
    json: &Json,
    null: &Table,
    array_mark: &Table,
    clock: &mut Clock<'_>,
    depth: usize,
) -> Result<Value, Failure> {
    clock.tick(lua)?;
    if depth >= JSON_DEPTH {
        return Err(Failure::Says(format!(
            "json.decode goes at most {JSON_DEPTH} levels deep"
        )));
    }
    Ok(match json {
        Json::Null => Value::Table(null.clone()),
        Json::Bool(b) => Value::Boolean(*b),
        Json::Number(n) => match n.as_i64() {
            Some(i) => Value::Integer(i),
            None => Value::Number(n.as_f64().unwrap_or(f64::NAN)),
        },
        Json::String(s) => Value::String(lua.create_string(s)?),
        Json::Array(items) => {
            let t = lua.create_table_with_capacity(items.len(), 0)?;
            for (i, item) in items.iter().enumerate() {
                t.raw_set(
                    i + 1,
                    to_lua(lua, item, null, array_mark, clock, depth + 1)?,
                )?;
            }
            t.set_metatable(Some(array_mark.clone()));
            Value::Table(t)
        }
        Json::Object(map) => {
            let t = lua.create_table_with_capacity(0, map.len())?;
            for (key, item) in map {
                t.raw_set(
                    lua.create_string(key)?,
                    to_lua(lua, item, null, array_mark, clock, depth + 1)?,
                )?;
            }
            Value::Table(t)
        }
    })
}

#[cfg(test)]
mod tests {
    use crate::test_support::{error_lines, typed_in_time};
    use crate::Action;

    /// What `code`, run as a `#lua` line, logged with `mud.log`.
    fn logged(code: &str) -> Vec<String> {
        typed_in_time(code)
            .actions
            .iter()
            .filter_map(|a| match a {
                Action::Log { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn json_comes_back_as_it_went_in() {
        let text = r#"{"a":[1,2,null,{"b":true}],"e":[],"n":-1.5,"o":{},"s":"Orla"}"#;
        assert_eq!(
            logged(&format!("mud.log(json.encode(json.decode('{text}')))")),
            [text]
        );
    }

    #[test]
    fn a_table_with_the_keys_1_to_n_is_an_array_and_any_other_an_object() {
        assert_eq!(
            logged(
                "mud.log(json.encode({1, 2, 3})) \
                 mud.log(json.encode({})) \
                 mud.log(json.encode(json.array())) \
                 mud.log(json.encode(json.array({'a'}))) \
                 mud.log(json.encode({x = 1, [1] = 2})) \
                 mud.log(json.encode({1, nil, 3}))"
            ),
            [
                "[1,2,3]",
                "{}",
                "[]",
                r#"["a"]"#,
                r#"{"1":2,"x":1}"#,
                r#"{"1":1,"3":3}"#,
            ]
        );
    }

    #[test]
    fn null_is_a_value_of_its_own() {
        assert_eq!(
            logged(
                "mud.log(tostring(json.null)) \
                 mud.log(tostring(json.decode('null') == json.null)) \
                 local t = json.decode('[1,null,3]') \
                 mud.log(tostring(#t)) \
                 mud.log(json.encode({json.null}))"
            ),
            ["null", "true", "3", "[null]"]
        );
    }

    #[test]
    fn what_json_cannot_hold_raises_an_error_at_your_line() {
        for (code, says) in [
            (
                "json.encode({f = function() end})",
                "t:1: json.encode cannot write a function",
            ),
            (
                "local t = {} t.t = t json.encode(t)",
                "t:1: json.encode found a table inside itself",
            ),
            (
                "json.encode(0/0)",
                "t:1: json.encode cannot write the number NaN",
            ),
            (
                "json.encode({[true] = 1})",
                "t:1: json.encode cannot write a key of type boolean",
            ),
            (
                "json.decode('{')",
                "t:1: json.decode could not read the text",
            ),
            (
                "json.decode(5)",
                "t:1: bad argument #1 to 'decode' (string expected, got integer)",
            ),
        ] {
            let lines = error_lines(&typed_in_time(code));
            assert!(lines.iter().any(|l| l.contains(says)), "{code}: {lines:?}");
        }
    }

    #[test]
    fn json_null_cannot_change() {
        let lines = error_lines(&typed_in_time("json.null.x = 1"));
        assert!(
            lines
                .iter()
                .any(|l| l.contains("json.null cannot be changed")),
            "{lines:?}"
        );
    }
}
