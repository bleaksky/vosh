//! The `regex` table: `regex.match`, `regex.find`, `regex.gmatch`,
//! `regex.gsub` and `regex.split`, which every script and plugin reads
//! like `string`. Each takes a pattern in the syntax triggers use, the
//! Rust regex crate's, which matches in time that grows with the text
//! alone, so no pattern can hang a script.
//!
//! Positions count bytes from 1, as `string.find` does. A pattern with
//! no groups hands back the whole match, and one with groups each group,
//! nil for a group that took no part. Vosh keeps the last
//! [`CACHE_SIZE`] patterns compiled and turns down one that would
//! compile larger than [`PATTERN_BYTES`].

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};

use mlua::{Function, Lua, MultiValue, Value};
use regex::bytes::{Captures, Regex, RegexBuilder};

use crate::json::{answer, Clock, Failure};
use crate::limits::{Limits, INTERNAL_CHUNK};

/// How many compiled patterns each engine keeps.
const CACHE_SIZE: usize = 64;

/// The most memory one compiled pattern may take.
const PATTERN_BYTES: usize = 1024 * 1024;

/// The Lua that builds the `regex` table. It takes the Rust functions
/// and raises each error at the line that called the function.
const REGEX: &str = r"
local find, match, next_match, gsub, split = ...
local error = error
local pack, unpack = table.pack, table.unpack

local function found(ok, ...)
  if ok then
    return ...
  end
  error((...), 2)
end

regex = {
  find = function(...)
    return found(find(...))
  end,
  match = function(...)
    return found(match(...))
  end,
  gmatch = function(text, pattern)
    -- A bad argument or pattern errors here, not at the first step.
    local ok, e = next_match(text, pattern, 1, 0, true)
    if not ok then
      error(e, 2)
    end
    local at, last = 1, 0
    return function()
      if at == nil then
        return nil
      end
      local r = pack(next_match(text, pattern, at, last, false))
      if not r[1] then
        error(r[2], 2)
      end
      if r[2] == nil then
        at = nil
        return nil
      end
      at, last = r[2], r[3]
      return unpack(r, 4, r.n)
    end
  end,
  gsub = function(...)
    return found(gsub(...))
  end,
  split = function(...)
    return found(split(...))
  end,
}
";

/// The patterns an engine compiled last, newest at the back.
#[derive(Default)]
struct Cache {
    compiled: HashMap<String, Arc<Regex>>,
    order: VecDeque<String>,
}

impl Cache {
    fn get(&mut self, pattern: &str) -> Result<Arc<Regex>, Failure> {
        if let Some(found) = self.compiled.get(pattern) {
            let found = Arc::clone(found);
            if let Some(at) = self.order.iter().position(|p| p == pattern) {
                let key = self.order.remove(at).unwrap_or_default();
                self.order.push_back(key);
            }
            return Ok(found);
        }
        let compiled = RegexBuilder::new(pattern)
            .size_limit(PATTERN_BYTES)
            .build()
            .map_err(|e| Failure::Says(format!("regex could not read the pattern: {e}")))?;
        let compiled = Arc::new(compiled);
        if self.order.len() >= CACHE_SIZE {
            if let Some(oldest) = self.order.pop_front() {
                self.compiled.remove(&oldest);
            }
        }
        self.compiled
            .insert(pattern.to_string(), Arc::clone(&compiled));
        self.order.push_back(pattern.to_string());
        Ok(compiled)
    }
}

type Shared = Arc<Mutex<Cache>>;

/// Put the `regex` table in the globals.
pub(crate) fn install(lua: &Lua, limits: &Arc<Limits>) -> mlua::Result<()> {
    let cache: Shared = Arc::default();
    let find = {
        let cache = Arc::clone(&cache);
        lua.create_function(move |lua, args: MultiValue| {
            answer(lua, find_or_match(lua, &cache, &args, "find", true))
        })?
    };
    let matcher = {
        let cache = Arc::clone(&cache);
        lua.create_function(move |lua, args: MultiValue| {
            answer(lua, find_or_match(lua, &cache, &args, "match", false))
        })?
    };
    let next_match = {
        let cache = Arc::clone(&cache);
        lua.create_function(move |lua, args: MultiValue| {
            answer(lua, next_match(lua, &cache, &args))
        })?
    };
    let gsub = {
        let cache = Arc::clone(&cache);
        let limits = Arc::clone(limits);
        lua.create_function(move |lua, args: MultiValue| {
            answer(lua, gsub(lua, &cache, &limits, &args))
        })?
    };
    let split = {
        let cache = Arc::clone(&cache);
        let limits = Arc::clone(limits);
        lua.create_function(move |lua, args: MultiValue| {
            answer(lua, split(lua, &cache, &limits, &args))
        })?
    };
    lua.load(REGEX)
        .set_name(INTERNAL_CHUNK)
        .call::<()>((find, matcher, next_match, gsub, split))
}

/// Argument `arg` as a string, a number turned into one, as the string
/// library reads it.
fn text_arg(
    lua: &Lua,
    args: &MultiValue,
    arg: usize,
    fname: &str,
) -> Result<mlua::String, Failure> {
    let value = args.get(arg - 1).cloned().unwrap_or(Value::Nil);
    let kind = value.type_name();
    match lua.coerce_string(value)? {
        Some(s) => Ok(s),
        None => Err(Failure::Says(format!(
            "bad argument #{arg} to '{fname}' (string expected, got {kind})"
        ))),
    }
}

/// The compiled pattern of argument 2.
fn pattern_arg(
    lua: &Lua,
    cache: &Shared,
    args: &MultiValue,
    fname: &str,
) -> Result<Arc<Regex>, Failure> {
    let pattern = text_arg(lua, args, 2, fname)?;
    let pattern = pattern.to_str().map_err(|_| {
        Failure::Says(format!(
            "bad argument #2 to '{fname}' (pattern is not UTF-8)"
        ))
    })?;
    cache
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&pattern)
}

/// Argument `arg` as a whole number, or `default` when it is nil.
fn int_arg(args: &MultiValue, arg: usize, fname: &str, default: i64) -> Result<i64, Failure> {
    match args.get(arg - 1) {
        None | Some(Value::Nil) => Ok(default),
        Some(Value::Integer(n)) => Ok(*n),
        #[allow(clippy::cast_possible_truncation)]
        Some(Value::Number(n)) if n.fract() == 0.0 && n.is_finite() => Ok(*n as i64),
        Some(other) => Err(Failure::Says(format!(
            "bad argument #{arg} to '{fname}' (number expected, got {})",
            other.type_name()
        ))),
    }
}

/// Where a search starts, from `init` counted as `string.find` counts
/// it, or None when it starts past the end.
fn start_at(init: i64, len: usize) -> Option<usize> {
    let len_i = i64::try_from(len).unwrap_or(i64::MAX);
    let from = if init < 0 {
        (len_i + init + 1).max(1)
    } else {
        init.max(1)
    };
    let from = usize::try_from(from - 1).ok()?;
    (from <= len).then_some(from)
}

/// The groups of `caps`, or the whole match when the pattern has none.
fn groups(lua: &Lua, caps: &Captures<'_>, out: &mut Vec<Value>) -> mlua::Result<()> {
    if caps.len() == 1 {
        out.push(Value::String(
            lua.create_string(caps.get(0).map_or(&[][..], |m| m.as_bytes()))?,
        ));
        return Ok(());
    }
    for group in caps.iter().skip(1) {
        out.push(match group {
            Some(m) => Value::String(lua.create_string(m.as_bytes())?),
            None => Value::Nil,
        });
    }
    Ok(())
}

fn position(at: usize) -> Value {
    Value::Integer(i64::try_from(at).unwrap_or(i64::MAX))
}

fn find_or_match(
    lua: &Lua,
    cache: &Shared,
    args: &MultiValue,
    fname: &str,
    find: bool,
) -> Result<MultiValue, Failure> {
    let text = text_arg(lua, args, 1, fname)?;
    let re = pattern_arg(lua, cache, args, fname)?;
    let init = int_arg(args, 3, fname, 1)?;
    let bytes = text.as_bytes();
    let bytes: &[u8] = &bytes;
    let Some(from) = start_at(init, bytes.len()) else {
        return Ok(MultiValue::from_iter([Value::Nil]));
    };
    let Some(caps) = re.captures_at(bytes, from) else {
        return Ok(MultiValue::from_iter([Value::Nil]));
    };
    let mut out = Vec::new();
    if find {
        let whole = caps.get(0).map_or(from..from, |m| m.range());
        out.push(position(whole.start + 1));
        out.push(position(whole.end));
        if caps.len() > 1 {
            groups(lua, &caps, &mut out)?;
        }
    } else {
        groups(lua, &caps, &mut out)?;
    }
    Ok(MultiValue::from_iter(out))
}

/// One step of `regex.gmatch`: the next match from byte `at`, counted
/// from 1, that is not an empty match where the last match ended,
/// `last` counted the same way. Hands back where the next step starts,
/// where this match ended and its groups, or nil once none is left.
/// With `check` it only reads the arguments.
fn next_match(lua: &Lua, cache: &Shared, args: &MultiValue) -> Result<MultiValue, Failure> {
    let text = text_arg(lua, args, 1, "gmatch")?;
    let re = pattern_arg(lua, cache, args, "gmatch")?;
    if matches!(args.get(4), Some(Value::Boolean(true))) {
        return Ok(MultiValue::new());
    }
    let at = usize::try_from(int_arg(args, 3, "gmatch", 1)?.max(1) - 1).unwrap_or(0);
    let last = int_arg(args, 4, "gmatch", 0)?;
    let bytes = text.as_bytes();
    let bytes: &[u8] = &bytes;
    let mut from = at;
    while from <= bytes.len() {
        let Some(caps) = re.captures_at(bytes, from) else {
            break;
        };
        let Some(whole) = caps.get(0) else {
            break;
        };
        if whole.is_empty() && i64::try_from(whole.start()).ok() == Some(last) && last > 0 {
            from = whole.start() + char_len(bytes, whole.start());
            continue;
        }
        let next = if whole.is_empty() {
            whole.end() + char_len(bytes, whole.end())
        } else {
            whole.end()
        };
        let mut out = vec![position(next + 1), position(whole.end())];
        groups(lua, &caps, &mut out)?;
        return Ok(MultiValue::from_iter(out));
    }
    Ok(MultiValue::from_iter([Value::Nil]))
}

/// The length in bytes of the character at `at`, 1 for a byte that is
/// not UTF-8 and at the end.
fn char_len(bytes: &[u8], at: usize) -> usize {
    let lead = bytes.get(at).copied().unwrap_or(0);
    let want = match lead {
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1,
    };
    if at + want <= bytes.len() && std::str::from_utf8(&bytes[at..at + want]).is_ok() {
        want
    } else {
        1
    }
}

fn gsub(
    lua: &Lua,
    cache: &Shared,
    limits: &Limits,
    args: &MultiValue,
) -> Result<MultiValue, Failure> {
    let text = text_arg(lua, args, 1, "gsub")?;
    let re = pattern_arg(lua, cache, args, "gsub")?;
    let repl = args.get(2).cloned().unwrap_or(Value::Nil);
    let most = int_arg(args, 4, "gsub", i64::MAX)?;
    let most = usize::try_from(most.max(0)).unwrap_or(usize::MAX);
    let template = match &repl {
        Value::String(s) => Some(s.as_bytes().to_vec()),
        Value::Integer(_) | Value::Number(_) => lua
            .coerce_string(repl.clone())?
            .map(|s| s.as_bytes().to_vec()),
        Value::Function(_) => None,
        other => {
            return Err(Failure::Says(format!(
                "bad argument #3 to 'gsub' (string or function expected, got {})",
                other.type_name()
            )))
        }
    };
    let call: Option<Function> = match &repl {
        Value::Function(f) => Some(f.clone()),
        _ => None,
    };
    let bytes = text.as_bytes();
    let bytes: &[u8] = &bytes;
    let mut out = Vec::with_capacity(bytes.len());
    let mut last = 0;
    let mut count: i64 = 0;
    let mut clock = Clock { limits, seen: 0 };
    for caps in re.captures_iter(bytes).take(most) {
        clock.tick(lua)?;
        let Some(whole) = caps.get(0) else { continue };
        out.extend_from_slice(&bytes[last..whole.start()]);
        if let Some(template) = &template {
            caps.expand(template, &mut out);
        } else if let Some(call) = &call {
            let mut pieces = Vec::new();
            groups(lua, &caps, &mut pieces)?;
            let back: Value = call.call(MultiValue::from_iter(pieces))?;
            match back {
                Value::Nil | Value::Boolean(false) => out.extend_from_slice(whole.as_bytes()),
                other => match lua.coerce_string(other.clone())? {
                    Some(s) => out.extend_from_slice(&s.as_bytes()),
                    None => {
                        return Err(Failure::Says(format!(
                            "invalid replacement value (a {})",
                            other.type_name()
                        )))
                    }
                },
            }
        }
        last = whole.end();
        count += 1;
        if out.len() > limits.memory_room(lua) {
            return Err(Failure::Stopped);
        }
    }
    out.extend_from_slice(&bytes[last..]);
    Ok(MultiValue::from_iter([
        Value::String(lua.create_string(&out)?),
        Value::Integer(count),
    ]))
}

fn split(
    lua: &Lua,
    cache: &Shared,
    limits: &Limits,
    args: &MultiValue,
) -> Result<MultiValue, Failure> {
    let text = text_arg(lua, args, 1, "split")?;
    let re = pattern_arg(lua, cache, args, "split")?;
    let bytes = text.as_bytes();
    let bytes: &[u8] = &bytes;
    let pieces = lua.create_table()?;
    let mut clock = Clock { limits, seen: 0 };
    for (i, piece) in re.split(bytes).enumerate() {
        clock.tick(lua)?;
        pieces.raw_set(i + 1, lua.create_string(piece)?)?;
    }
    Ok(MultiValue::from_iter([Value::Table(pieces)]))
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
    fn match_and_find_give_groups_and_byte_positions() {
        assert_eq!(
            logged(
                r"mud.log(table.concat({regex.match('Maren: 12 hp', [[^(\w+): (\d+)]])}, ','))
                   mud.log(regex.match('a1b22', [[\d+]]))
                   mud.log(table.concat({regex.find('hello world', 'o w')}, ','))
                   mud.log(table.concat({regex.find('hello', 'l', -2)}, ','))
                   mud.log(table.concat({regex.find('key = value', [[(\w+)\s*=\s*(\w+)]])}, ','))
                   mud.log(tostring(regex.find('hello', 'z')))
                   mud.log(regex.match('café', [[\w+$]]))
                   mud.log(tostring(select(2, regex.match('ab', '(a)(x)?'))))"
            ),
            [
                "Maren,12",
                "1",
                "5,7",
                "4,4",
                "1,11,key,value",
                "nil",
                "café",
                "nil"
            ]
        );
    }

    #[test]
    fn gmatch_steps_through_each_match_as_string_gmatch_does() {
        assert_eq!(
            logged(
                r"local all = {}
                   for n in regex.gmatch('a1b22c333', [[\d+]]) do all[#all + 1] = n end
                   mud.log(table.concat(all, ','))
                   local empty = 0
                   for _ in regex.gmatch('abc', 'x*') do empty = empty + 1 end
                   mud.log(tostring(empty))
                   local runs = {}
                   for r in regex.gmatch('aab', 'a*') do runs[#runs + 1] = '<' .. r .. '>' end
                   mud.log(table.concat(runs))
                   for k, v in regex.gmatch('hp=12 mv=40', [[(\w+)=(\d+)]]) do mud.log(k .. ':' .. v) end"
            ),
            ["1,22,333", "4", "<aa><>", "hp:12", "mv:40"]
        );
    }

    #[test]
    fn gsub_takes_a_template_or_a_function_and_a_count() {
        assert_eq!(
            logged(
                r"mud.log(table.concat({regex.gsub('hp 120/300', [[(\d+)/(\d+)]], '$2 of $1')}, ','))
                   mud.log(table.concat({regex.gsub('a b c', [[\w]], function(c) return c:upper() end)}, ','))
                   mud.log(table.concat({regex.gsub('a b c', [[\w]], function(c) if c == 'b' then return nil end return 'x' end)}, ','))
                   mud.log(table.concat({regex.gsub('aaa', 'a', 'b', 2)}, ','))
                   mud.log(table.concat({regex.gsub('cost $5', 'cost', '$$')}, ','))"
            ),
            ["hp 300 of 120,1", "A B C,3", "x b x,3", "bba,2", "$ $5,1"]
        );
    }

    #[test]
    fn split_hands_back_the_pieces_between_matches() {
        assert_eq!(
            logged(r"mud.log(table.concat(regex.split('Orla, Maren,Tolliver', [[,\s*]]), '|'))"),
            ["Orla|Maren|Tolliver"]
        );
    }

    #[test]
    fn a_bad_pattern_raises_an_error_at_your_line() {
        for code in [
            "regex.find('x', '(')",
            "regex.gmatch('x', '(')",
            "regex.gsub('x', '[', 'y')",
        ] {
            let lines = error_lines(&typed_in_time(code));
            assert!(
                lines
                    .iter()
                    .any(|l| l.contains("t:1: regex could not read the pattern")),
                "{code}: {lines:?}"
            );
        }
        let lines = error_lines(&typed_in_time("regex.gsub('x', 'x', {})"));
        assert!(
            lines.iter().any(|l| l
                .contains("bad argument #3 to 'gsub' (string or function expected, got table)")),
            "{lines:?}"
        );
    }

    #[test]
    fn an_error_in_a_replacement_function_reaches_the_script() {
        let lines = error_lines(&typed_in_time(
            "regex.gsub('a', 'a', function() error('nope') end)",
        ));
        assert!(lines.iter().any(|l| l.contains("nope")), "{lines:?}");
    }
}
