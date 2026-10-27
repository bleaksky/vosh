//! The `regex` table: `regex.match`, `regex.find`, `regex.gmatch`,
//! `regex.gsub` and `regex.split`, which every script and plugin reads
//! like `string`. Each takes a pattern in the syntax triggers use, the
//! Rust regex crate's, whose time grows with the text and the pattern
//! alone, so no pattern backtracks without end.
//!
//! Positions count bytes from 1, as `string.find` does, and a search
//! from `init` anchors `^` there, as `string.find` does. A pattern with
//! no groups hands back the whole match, and one with groups each group,
//! nil for a group that took no part. An empty match never lands inside
//! a character, and one where the last match ended is skipped, as Lua
//! 5.4 does.
//!
//! One search cannot stop part way, so each call takes text of at most
//! [`TEXT_BYTES`] and a pattern of at most [`NAME_BYTES`], and looks at
//! the clock after each match and once more as it ends. Vosh keeps the
//! last [`CACHE_SIZE`] patterns compiled and turns down one that would
//! compile larger than [`PATTERN_BYTES`]. `regex.gsub` with a function
//! runs its loop in Lua, a step at a time, so an error the function
//! raises reaches the script as it was raised.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};

use mlua::{Lua, MultiValue, Value};
use regex::bytes::{Captures, Regex, RegexBuilder};

use crate::json::{answer, Failure};
use crate::limits::{Limits, INTERNAL_CHUNK, NAME_BYTES};

/// How many compiled patterns each engine keeps.
const CACHE_SIZE: usize = 64;

/// The most memory one compiled pattern may take.
const PATTERN_BYTES: usize = 512 * 1024;

/// The most text one call searches.
pub(crate) const TEXT_BYTES: usize = 256 * 1024;

/// The Lua that builds the `regex` table. It takes the Rust functions
/// and raises each error at the line that called the function.
const REGEX: &str = r#"
local find, match, step, gsub_text, split = ...
local error, tonumber, type = error, tonumber, type
local tointeger, huge = math.tointeger, math.huge
local pack, unpack, concat = table.pack, table.unpack, table.concat
local sub = string.sub

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
    local ok, e = step(text, pattern, 1, 0, true)
    if not ok then
      error(e, 2)
    end
    local at, last = 1, 0
    return function()
      if at == nil then
        return nil
      end
      local r = pack(step(text, pattern, at, last, false))
      if not r[1] then
        error(r[2], 2)
      end
      if r[2] == nil then
        at = nil
        return nil
      end
      at, last = r[2], r[4]
      return unpack(r, 5, r.n)
    end
  end,
  gsub = function(text, pattern, repl, n)
    if type(repl) ~= "function" then
      return found(gsub_text(text, pattern, repl, n))
    end
    local ok, e = step(text, pattern, 1, 0, true, "gsub")
    if not ok then
      error(e, 2)
    end
    local most = huge
    if n ~= nil then
      local number = tonumber(n)
      most = number and tointeger(number)
      if most == nil then
        if number == nil then
          error("bad argument #4 to 'gsub' (number expected, got " .. type(n) .. ")", 2)
        end
        error("bad argument #4 to 'gsub' (number has no integer representation)", 2)
      end
    end
    local out, count, at, last, from = {}, 0, 1, 0, 1
    while count < most do
      local r = pack(step(text, pattern, at, last, false))
      if not r[1] then
        error(r[2], 2)
      end
      if r[2] == nil then
        break
      end
      local start, stop = r[3], r[4]
      local v = repl(unpack(r, 5, r.n))
      if v == nil or v == false then
        v = sub(text, start, stop)
      elseif type(v) ~= "string" and type(v) ~= "number" then
        error("invalid replacement value (a " .. type(v) .. ")", 2)
      end
      out[#out + 1] = sub(text, from, start - 1)
      out[#out + 1] = v
      from = stop + 1
      count = count + 1
      at, last = r[2], stop
    end
    out[#out + 1] = sub(text, from)
    return concat(out), count
  end,
  split = function(...)
    return found(split(...))
  end,
}
"#;

/// The patterns an engine compiled last, newest at the back.
#[derive(Default)]
struct Cache {
    compiled: HashMap<Arc<str>, Arc<Regex>>,
    order: VecDeque<Arc<str>>,
}

impl Cache {
    fn get(&mut self, pattern: &str) -> Result<Arc<Regex>, Failure> {
        if let Some(found) = self.compiled.get(pattern) {
            let found = Arc::clone(found);
            if let Some(at) = self.order.iter().position(|p| &**p == pattern) {
                if let Some(key) = self.order.remove(at) {
                    self.order.push_back(key);
                }
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
        let key: Arc<str> = Arc::from(pattern);
        self.compiled
            .insert(Arc::clone(&key), Arc::clone(&compiled));
        self.order.push_back(key);
        Ok(compiled)
    }
}

type Shared = Arc<Mutex<Cache>>;

/// What a call reads from its first two arguments: the text and the
/// compiled pattern.
struct Search {
    text: mlua::String,
    re: Arc<Regex>,
}

/// Put the `regex` table in the globals.
pub(crate) fn install(lua: &Lua, limits: &Arc<Limits>) -> mlua::Result<()> {
    let cache: Shared = Arc::default();
    let call = |run: fn(&Lua, &Shared, &Limits, &MultiValue) -> Result<MultiValue, Failure>| {
        let cache = Arc::clone(&cache);
        let limits = Arc::clone(limits);
        lua.create_function(move |lua, args: MultiValue| {
            let mut result = run(lua, &cache, &limits, &args);
            // One search cannot stop part way, so the clock looks once
            // more as the call ends.
            if result.is_ok() && limits.out_of_time(lua) {
                result = Err(Failure::Stopped);
            }
            answer(lua, result)
        })
    };
    let find = call(|lua, cache, _, args| find_or_match(lua, cache, args, "find", true))?;
    let matcher = call(|lua, cache, _, args| find_or_match(lua, cache, args, "match", false))?;
    let step = call(step)?;
    let gsub_text = call(gsub_text)?;
    let split = call(split)?;
    lua.load(REGEX)
        .set_name(INTERNAL_CHUNK)
        .call::<()>((find, matcher, step, gsub_text, split))
}

/// The text and pattern of a call to `fname`, each within its limit.
fn search(lua: &Lua, cache: &Shared, args: &MultiValue, fname: &str) -> Result<Search, Failure> {
    let text = text_arg(lua, args, 1, fname)?;
    if text.as_bytes().len() > TEXT_BYTES {
        return Err(Failure::Says(format!(
            "bad argument #1 to '{fname}' (regex searches text of at most {TEXT_BYTES} bytes)"
        )));
    }
    let pattern = text_arg(lua, args, 2, fname)?;
    if pattern.as_bytes().len() > NAME_BYTES {
        return Err(Failure::Says(format!(
            "bad argument #2 to '{fname}' (a pattern holds at most {NAME_BYTES} bytes)"
        )));
    }
    let pattern = pattern.to_str().map_err(|_| {
        Failure::Says(format!(
            "bad argument #2 to '{fname}' (pattern is not UTF-8)"
        ))
    })?;
    let re = cache
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&pattern)?;
    Ok(Search { text, re })
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

/// Argument `arg` as a whole number, a numeric string turned into one,
/// or `default` when it is nil.
fn int_arg(
    lua: &Lua,
    args: &MultiValue,
    arg: usize,
    fname: &str,
    default: i64,
) -> Result<i64, Failure> {
    let value = args.get(arg - 1).cloned().unwrap_or(Value::Nil);
    if value.is_nil() {
        return Ok(default);
    }
    let kind = value.type_name();
    let number = lua.coerce_number(value.clone())?;
    match (number, lua.coerce_integer(value)?) {
        (_, Some(n)) => Ok(n),
        (Some(_), None) => Err(Failure::Says(format!(
            "bad argument #{arg} to '{fname}' (number has no integer representation)"
        ))),
        (None, None) => Err(Failure::Says(format!(
            "bad argument #{arg} to '{fname}' (number expected, got {kind})"
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
        let whole = caps.get(0).map_or(&[][..], |m| m.as_bytes());
        out.push(Value::String(lua.create_string(whole)?));
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
    let Search { text, re } = search(lua, cache, args, fname)?;
    let init = int_arg(lua, args, 3, fname, 1)?;
    let bytes = text.as_bytes();
    let Some(from) = start_at(init, bytes.len()) else {
        return Ok(MultiValue::from_iter([Value::Nil]));
    };
    let Some(caps) = re.captures(&bytes[from..]) else {
        return Ok(MultiValue::from_iter([Value::Nil]));
    };
    let mut out = Vec::new();
    if find {
        let whole = caps.get(0).map_or(0..0, |m| m.range());
        out.push(position(from + whole.start + 1));
        out.push(position(from + whole.end));
        if caps.len() > 1 {
            groups(lua, &caps, &mut out)?;
        }
    } else {
        groups(lua, &caps, &mut out)?;
    }
    Ok(MultiValue::from_iter(out))
}

/// The length in bytes of the character at `at`, 1 for a byte that is
/// not UTF-8 and at the end.
fn char_len(bytes: &[u8], at: usize) -> usize {
    let want = match bytes.get(at).copied().unwrap_or(0) {
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

/// The next match in `bytes` from the byte `from`, skipping an empty one
/// that starts where the last match ended, at `last`.
fn next_in<'h>(
    re: &Regex,
    bytes: &'h [u8],
    mut from: usize,
    last: Option<usize>,
) -> Option<Captures<'h>> {
    while from <= bytes.len() {
        let caps = re.captures_at(bytes, from)?;
        let whole = caps.get(0)?;
        if whole.is_empty() && Some(whole.start()) == last {
            from = whole.start() + char_len(bytes, whole.start());
            continue;
        }
        return Some(caps);
    }
    None
}

/// Where the search after a match from `start` to `end` goes on: at its
/// end, or past the character there when the match was empty.
fn after(bytes: &[u8], start: usize, end: usize) -> usize {
    if start == end {
        end + char_len(bytes, end)
    } else {
        end
    }
}

/// One step of `regex.gmatch` or of `regex.gsub` with a function: the
/// next match from the byte `at`, counted from 1, never an empty one
/// where the last match ended, `last`, 0 for none. Hands back where the
/// next step starts, where this match starts and ends and its groups,
/// or nil once none is left. With a true fifth argument it only reads
/// the text and the pattern. A sixth names the function for its errors.
fn step(lua: &Lua, cache: &Shared, _: &Limits, args: &MultiValue) -> Result<MultiValue, Failure> {
    let fname = match args.get(5) {
        Some(Value::String(name)) if name.as_bytes().as_ref() == b"gsub" => "gsub",
        _ => "gmatch",
    };
    let Search { text, re } = search(lua, cache, args, fname)?;
    if matches!(args.get(4), Some(Value::Boolean(true))) {
        return Ok(MultiValue::new());
    }
    let at = usize::try_from(int_arg(lua, args, 3, fname, 1)?.max(1) - 1).unwrap_or(0);
    let last = usize::try_from(int_arg(lua, args, 4, fname, 0)?)
        .ok()
        .filter(|&last| last > 0);
    let bytes = text.as_bytes();
    let Some(caps) = next_in(&re, &bytes, at, last) else {
        return Ok(MultiValue::from_iter([Value::Nil]));
    };
    let whole = caps.get(0).map_or(at..at, |m| m.range());
    let mut out = vec![
        position(after(&bytes, whole.start, whole.end) + 1),
        position(whole.start + 1),
        position(whole.end),
    ];
    groups(lua, &caps, &mut out)?;
    Ok(MultiValue::from_iter(out))
}

/// One piece of a `regex.gsub` template: text as it stands, or a group,
/// None for a name the pattern does not have, which gives nothing.
enum Piece {
    Text(Vec<u8>),
    Group(Option<usize>),
}

/// `template` in pieces, read as the regex crate reads a replacement:
/// `$name` takes the longest run of letters, digits and `_`, `${name}`
/// takes what the braces hold, and `$$` is a dollar sign. A name of
/// digits alone names a group by number.
fn pieces(template: &[u8], re: &Regex) -> Vec<Piece> {
    let group = |name: &[u8]| -> Option<usize> {
        let name = std::str::from_utf8(name).ok()?;
        if !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit()) {
            return name.parse().ok().filter(|&n| n < re.captures_len());
        }
        re.capture_names().position(|n| n == Some(name))
    };
    let mut out = Vec::new();
    let mut text = Vec::new();
    let mut i = 0;
    while i < template.len() {
        if template[i] != b'$' {
            text.push(template[i]);
            i += 1;
            continue;
        }
        let rest = &template[i + 1..];
        let (name, used) = match rest.first() {
            Some(b'$') => {
                text.push(b'$');
                i += 2;
                continue;
            }
            Some(b'{') => match rest.iter().position(|&b| b == b'}') {
                Some(close) => (&rest[1..close], close + 1),
                None => (&rest[..0], 0),
            },
            _ => {
                let run = rest
                    .iter()
                    .position(|&b| !(b.is_ascii_alphanumeric() || b == b'_'))
                    .unwrap_or(rest.len());
                (&rest[..run], run)
            }
        };
        if used == 0 {
            text.push(b'$');
            i += 1;
            continue;
        }
        if !text.is_empty() {
            out.push(Piece::Text(std::mem::take(&mut text)));
        }
        out.push(Piece::Group(group(name)));
        i += 1 + used;
    }
    if !text.is_empty() {
        out.push(Piece::Text(text));
    }
    out
}

/// How many bytes `pieces` give for the match `caps`.
fn expanded_len(pieces: &[Piece], caps: &Captures<'_>) -> usize {
    pieces
        .iter()
        .map(|piece| match piece {
            Piece::Text(text) => text.len(),
            Piece::Group(Some(n)) => caps.get(*n).map_or(0, |m| m.len()),
            Piece::Group(None) => 0,
        })
        .sum()
}

fn expand(pieces: &[Piece], caps: &Captures<'_>, out: &mut Vec<u8>) {
    for piece in pieces {
        match piece {
            Piece::Text(text) => out.extend_from_slice(text),
            Piece::Group(Some(n)) => {
                if let Some(m) = caps.get(*n) {
                    out.extend_from_slice(m.as_bytes());
                }
            }
            Piece::Group(None) => {}
        }
    }
}

/// The error a call raises when what Rust builds would take it past its
/// memory, as a buffer the C library builds in Lua would.
fn out_of_memory() -> Failure {
    Failure::Lua(mlua::Error::MemoryError("not enough memory".into()))
}

/// `regex.gsub` with a template: the text with each match, or the first
/// `n`, replaced, and the count.
fn gsub_text(
    lua: &Lua,
    cache: &Shared,
    limits: &Limits,
    args: &MultiValue,
) -> Result<MultiValue, Failure> {
    let Search { text, re } = search(lua, cache, args, "gsub")?;
    let repl = args.get(2).cloned().unwrap_or(Value::Nil);
    let template = match &repl {
        Value::String(_) | Value::Integer(_) | Value::Number(_) => lua
            .coerce_string(repl.clone())?
            .map(|s| s.as_bytes().to_vec())
            .unwrap_or_default(),
        other => {
            return Err(Failure::Says(format!(
                "bad argument #3 to 'gsub' (string or function expected, got {})",
                other.type_name()
            )))
        }
    };
    let most = int_arg(lua, args, 4, "gsub", i64::MAX)?;
    let pieces = pieces(&template, &re);
    let bytes = text.as_bytes();
    let room = limits.memory_room(lua);
    let mut out = Vec::new();
    let (mut from, mut at, mut last) = (0, 0, None);
    let mut count: i64 = 0;
    while count < most && at <= bytes.len() {
        if limits.out_of_time(lua) {
            return Err(Failure::Stopped);
        }
        let Some(caps) = next_in(&re, &bytes, at, last) else {
            break;
        };
        let whole = caps.get(0).map_or(at..at, |m| m.range());
        let grows = (whole.start - from) + expanded_len(&pieces, &caps);
        if out.len().saturating_add(grows) > room {
            return Err(out_of_memory());
        }
        out.extend_from_slice(&bytes[from..whole.start]);
        expand(&pieces, &caps, &mut out);
        from = whole.end;
        at = after(&bytes, whole.start, whole.end);
        last = Some(whole.end);
        count += 1;
    }
    if out.len().saturating_add(bytes.len() - from) > room {
        return Err(out_of_memory());
    }
    out.extend_from_slice(&bytes[from..]);
    Ok(MultiValue::from_iter([
        Value::String(lua.create_string(&out)?),
        Value::Integer(count),
    ]))
}

/// `regex.split`: the pieces between the matches.
fn split(
    lua: &Lua,
    cache: &Shared,
    limits: &Limits,
    args: &MultiValue,
) -> Result<MultiValue, Failure> {
    let Search { text, re } = search(lua, cache, args, "split")?;
    let bytes = text.as_bytes();
    let pieces = lua.create_table()?;
    let (mut from, mut at, mut last, mut n) = (0, 0, None, 0);
    while at <= bytes.len() {
        if limits.out_of_time(lua) {
            return Err(Failure::Stopped);
        }
        let Some(caps) = next_in(&re, &bytes, at, last) else {
            break;
        };
        let whole = caps.get(0).map_or(at..at, |m| m.range());
        n += 1;
        pieces.raw_set(n, lua.create_string(&bytes[from..whole.start])?)?;
        from = whole.end;
        at = after(&bytes, whole.start, whole.end);
        last = Some(whole.end);
    }
    pieces.raw_set(n + 1, lua.create_string(&bytes[from..])?)?;
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
        assert!(
            lines.iter().any(|l| l.starts_with("t:1: nope")),
            "{lines:?}"
        );
        assert!(!lines.iter().any(|l| l.contains("t:1: t:1:")), "{lines:?}");
        // A table it raises reaches a pcall as the table.
        assert_eq!(
            logged(
                "local ok, e = pcall(regex.gsub, 'a', 'a', function() error({code = 5}) end) \
                 mud.log(tostring(ok) .. ' ' .. tostring(e.code))"
            ),
            ["false 5"]
        );
    }

    #[test]
    fn a_replacement_past_the_memory_limit_stops_the_call() {
        let outcome = typed_in_time(
            "local ok = pcall(regex.gsub, string.rep('a', 100000), '(?s).+', string.rep('$0', 1000)) \
             mud.log('went on')",
        );
        assert!(
            !outcome
                .actions
                .iter()
                .any(|a| matches!(a, Action::Log { .. })),
            "{:?}",
            outcome.actions
        );
        assert_ne!(error_lines(&outcome), Vec::<String>::new());
    }

    #[test]
    fn text_and_patterns_have_a_size_limit() {
        for (code, says) in [
            (
                "regex.find(string.rep('a', 300000), 'a')",
                "regex searches text of at most 262144 bytes",
            ),
            (
                "regex.find('a', string.rep('a', 5000))",
                "a pattern holds at most 4096 bytes",
            ),
            (
                "regex.gsub('a', 'a', 'b', 2.5)",
                "bad argument #4 to 'gsub' (number has no integer representation)",
            ),
            (
                "regex.gsub('a', 'a', function() end, 2.5)",
                "bad argument #4 to 'gsub' (number has no integer representation)",
            ),
            (
                "regex.gsub(5, '(', function() end)",
                "regex could not read the pattern",
            ),
        ] {
            let lines = error_lines(&typed_in_time(code));
            assert!(lines.iter().any(|l| l.contains(says)), "{code}: {lines:?}");
        }
    }

    #[test]
    fn edges_follow_the_string_library() {
        assert_eq!(
            logged(
                r"mud.log(table.concat({regex.find('hello', '^l', 3)}, ','))
                   mud.log(table.concat({regex.gsub('é', '', '-')}, ','))
                   mud.log(table.concat({regex.gsub('120', [[(\d+)]], '${1}hp')}, ','))
                   mud.log(table.concat({regex.gsub('aaa', 'a', 'b', '2')}, ','))
                   local p = regex.split('aé', '')
                   mud.log(#p .. ':' .. p[3])"
            ),
            ["3,3", "-é-,2", "120hp,1", "bba,2", "4:é"]
        );
    }
}
