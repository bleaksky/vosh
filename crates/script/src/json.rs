//! The `json` table: `json.encode`, `json.decode`, `json.null` and
//! `json.array`, which every script and plugin reads like `string`.
//!
//! JSON has two things a Lua table cannot tell apart on its own, null
//! and an empty array. `json.null` is a value of its own that stands for
//! null both ways, so an array that holds one keeps its length. It is a
//! light userdata, which has no fields and no metatable of its own, so no
//! script can change it. An array
//! `json.decode` builds carries a mark, and so does a table you pass to
//! `json.array`, so an empty one encodes back as `[]`. The mark's
//! metatable is locked too, since every decoded array shares it. Any
//! other table with the keys 1 to n is an array, and every other table an
//! object.
//!
//! Rust does the work and hands Lua the result. Each walk looks at the
//! clock every [`CLOCK_EVERY`] values as the hook would, and the text
//! each side takes or gives has a size limit of [`JSON_BYTES`], which an
//! encode counts as it goes, since Lua's memory limit never sees what
//! Rust builds.

use std::sync::Arc;

use mlua::{LightUserData, Lua, Table, Value};
use serde_json::{Map, Number, Value as Json};

use crate::limits::{self, Limits, INTERNAL_CHUNK};

/// The most text `json.decode` takes or `json.encode` gives.
pub(crate) const JSON_BYTES: usize = 1024 * 1024;

/// How deep tables may nest, either way.
const JSON_DEPTH: usize = 100;

/// How many values a walk handles between two looks at the clock.
pub(crate) const CLOCK_EVERY: usize = 100;

/// The registry key of the mark arrays carry.
const ARRAY_KEY: &str = "vosh.json.array";

/// The byte whose address `json.null` holds.
static NULL_MARK: u8 = 0;

/// `json.null`.
fn null() -> LightUserData {
    LightUserData(std::ptr::addr_of!(NULL_MARK).cast_mut().cast())
}

/// What `getmetatable` gives for a marked array.
const ARRAY_NAME: &str = "json.array";

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
    end
    local mark = getmetatable(t)
    if mark == "json.array" then
      return t
    elseif mark ~= nil then
      error("json.array cannot mark a table that has a metatable", 2)
    end
    return setmetatable(t, array_mark)
  end,
}
"#;

/// Put the `json` table in the globals.
pub(crate) fn install(lua: &Lua, limits: &Arc<Limits>) -> mlua::Result<()> {
    let array_mark = lua.create_table()?;
    array_mark.set("__name", ARRAY_NAME)?;
    array_mark.set("__metatable", ARRAY_NAME)?;
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
    lua.load(JSON).set_name(INTERNAL_CHUNK).call::<()>((
        encode,
        decode,
        Value::LightUserData(null()),
        array_mark,
    ))
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

/// The array mark.
struct Markers {
    array: Table,
}

impl Markers {
    fn of(lua: &Lua) -> mlua::Result<Self> {
        Ok(Self {
            array: lua.named_registry_value(ARRAY_KEY)?,
        })
    }
}

/// Run `walk` over each pair of `t`, keeping the first failure it gives
/// whole, since a callback can only end the iteration with a Lua error.
fn each_pair(
    t: &Table,
    mut walk: impl FnMut(Value, Value) -> Result<(), Failure>,
) -> Result<(), Failure> {
    let mut failed = None;
    let iterated = t.for_each::<Value, Value>(|key, value| {
        walk(key, value).map_err(|e| {
            failed = Some(e);
            mlua::Error::RuntimeError(String::new())
        })
    });
    match (iterated, failed) {
        (_, Some(e)) => Err(e),
        (Err(e), None) => Err(Failure::Lua(e)),
        (Ok(()), None) => Ok(()),
    }
}

fn encode(lua: &Lua, limits: &Limits, value: Value) -> Result<Value, Failure> {
    let mut walk = Encoder {
        markers: Markers::of(lua)?,
        clock: Clock { limits, seen: 0 },
        open: Vec::new(),
        bytes: 0,
    };
    let json = walk.value(lua, &value, 0)?;
    let text = serde_json::to_string(&json).map_err(|e| Failure::Says(e.to_string()))?;
    if text.len() > JSON_BYTES {
        return Err(too_long());
    }
    Ok(Value::String(lua.create_string(text)?))
}

fn too_long() -> Failure {
    Failure::Says(format!("json.encode gives at most {JSON_BYTES} bytes"))
}

struct Encoder<'a> {
    markers: Markers,
    clock: Clock<'a>,
    /// The tables the walk is inside now, to find one inside itself.
    open: Vec<*const std::ffi::c_void>,
    /// About how many bytes of text the walk built so far, so a large
    /// table stops at the limit before Rust holds much more than it.
    bytes: usize,
}

impl Encoder<'_> {
    /// Count `more` bytes of output, and stop past the limit.
    fn grow(&mut self, more: usize) -> Result<(), Failure> {
        self.bytes = self.bytes.saturating_add(more);
        if self.bytes > JSON_BYTES {
            return Err(too_long());
        }
        Ok(())
    }

    fn value(&mut self, lua: &Lua, value: &Value, depth: usize) -> Result<Json, Failure> {
        self.clock.tick(lua)?;
        Ok(match value {
            Value::Nil => {
                self.grow(4)?;
                Json::Null
            }
            Value::Boolean(b) => {
                self.grow(5)?;
                Json::Bool(*b)
            }
            Value::Integer(n) => {
                self.grow(20)?;
                Json::Number(Number::from(*n))
            }
            Value::Number(n) => match Number::from_f64(*n) {
                Some(n) => {
                    self.grow(24)?;
                    Json::Number(n)
                }
                None => {
                    return Err(Failure::Says(format!(
                        "json.encode cannot write the number {n}"
                    )))
                }
            },
            Value::String(s) => {
                self.grow(s.as_bytes().len() + 2)?;
                Json::String(s.to_string_lossy())
            }
            Value::LightUserData(u) if *u == null() => {
                self.grow(4)?;
                Json::Null
            }
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
            .is_some_and(|m| m.to_pointer() == self.markers.array.to_pointer());
        // The first pass only counts, so the walk holds no more than the
        // pair it looks at.
        let len = t.raw_len();
        let mut count = 0_usize;
        let mut sequence = true;
        each_pair(t, |key, _| {
            self.clock.tick(lua)?;
            count += 1;
            sequence &= matches!(key, Value::Integer(i)
                if i >= 1 && usize::try_from(i).is_ok_and(|i| i <= len));
            Ok(())
        })?;
        let sequence = sequence && count == len;
        let json = if marked || (sequence && len > 0) {
            if !sequence {
                return Err(Failure::Says(
                    "json.encode found a key in an array that is not 1 to n".into(),
                ));
            }
            self.grow(len + 2)?;
            let mut items = Vec::with_capacity(len);
            for i in 1..=len {
                let item: Value = t.raw_get(i)?;
                items.push(self.value(lua, &item, depth + 1)?);
            }
            Json::Array(items)
        } else {
            self.grow(2)?;
            let mut object = Map::new();
            each_pair(t, |key, item| {
                let name = match &key {
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
                self.grow(name.len() + 4)?;
                let item = self.value(lua, &item, depth + 1)?;
                if object.insert(name.clone(), item).is_some() {
                    return Err(Failure::Says(format!(
                        "json.encode found two keys that both read as {name}"
                    )));
                }
                Ok(())
            })?;
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
    let markers = Markers::of(lua)?;
    let mut clock = Clock { limits, seen: 0 };
    to_lua(lua, &json, &markers, &mut clock, 0)
}

fn to_lua(
    lua: &Lua,
    json: &Json,
    markers: &Markers,
    clock: &mut Clock<'_>,
    depth: usize,
) -> Result<Value, Failure> {
    clock.tick(lua)?;
    let deep = || Failure::Says(format!("json.decode goes at most {JSON_DEPTH} levels deep"));
    Ok(match json {
        Json::Null => Value::LightUserData(null()),
        Json::Bool(b) => Value::Boolean(*b),
        Json::Number(n) => match n.as_i64() {
            Some(i) => Value::Integer(i),
            None => Value::Number(n.as_f64().unwrap_or(f64::NAN)),
        },
        Json::String(s) => Value::String(lua.create_string(s)?),
        Json::Array(items) => {
            if depth >= JSON_DEPTH {
                return Err(deep());
            }
            let t = lua.create_table_with_capacity(items.len(), 0)?;
            for (i, item) in items.iter().enumerate() {
                t.raw_set(i + 1, to_lua(lua, item, markers, clock, depth + 1)?)?;
            }
            t.set_metatable(Some(markers.array.clone()));
            Value::Table(t)
        }
        Json::Object(map) => {
            if depth >= JSON_DEPTH {
                return Err(deep());
            }
            let t = lua.create_table_with_capacity(0, map.len())?;
            for (key, item) in map {
                t.raw_set(
                    lua.create_string(key)?,
                    to_lua(lua, item, markers, clock, depth + 1)?,
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
                "mud.log(type(json.null)) \
                 mud.log(tostring(json.decode('null') == json.null)) \
                 local t = json.decode('[1,null,3]') \
                 mud.log(tostring(#t)) \
                 mud.log(json.encode({json.null})) \
                 mud.log(tostring(getmetatable(json.null)))"
            ),
            ["userdata", "true", "3", "[null]", "nil"]
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
    fn neither_null_nor_the_array_mark_can_change() {
        for code in [
            "json.null.x = 1",
            "rawset(json.null, 'x', 1)",
            "setmetatable(json.decode('[]'), {})",
            "getmetatable(json.array()).__index = function() return 'x' end",
        ] {
            assert_ne!(
                error_lines(&typed_in_time(code)),
                Vec::<String>::new(),
                "{code}"
            );
        }
        assert_eq!(
            logged(
                "mud.log(getmetatable(json.array())) \
                 mud.log(getmetatable(json.decode('[]'))) \
                 local t = json.array({1}) \
                 mud.log(tostring(json.array(t) == t))"
            ),
            ["json.array", "json.array", "true"]
        );
    }

    #[test]
    fn two_keys_that_read_the_same_are_an_error() {
        let lines = error_lines(&typed_in_time(
            "json.encode({[1] = 'a', ['1'] = 'b', x = 1})",
        ));
        assert!(
            lines
                .iter()
                .any(|l| l.contains("json.encode found two keys that both read as 1")),
            "{lines:?}"
        );
    }

    #[test]
    fn a_large_table_stops_at_the_size_limit_before_it_is_built() {
        let outcome = typed_in_time(
            "local s = string.rep('x', 4000000) local t = {} \
             for i = 1, 100 do t[i] = s end \
             local ok, e = pcall(json.encode, t) mud.log(tostring(ok) .. ' ' .. e)",
        );
        let lines: Vec<String> = outcome
            .actions
            .iter()
            .filter_map(|a| match a {
                Action::Log { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(lines.len(), 1, "{:?}", outcome.actions);
        assert!(
            lines[0].starts_with("false json.encode gives at most"),
            "{lines:?}"
        );
    }

    #[test]
    fn what_encode_writes_100_deep_decode_reads_back() {
        assert_eq!(
            logged(
                "local t = {} local inner = t \
                 for i = 1, 99 do inner[1] = {} inner = inner[1] end \
                 local text = json.encode(t) \
                 mud.log(tostring(json.encode(json.decode(text)) == text))"
            ),
            ["true"]
        );
    }
}
