//! `string.find`, `string.match`, `string.gmatch` and `string.gsub`,
//! matched by [`crate::pattern`] under the call's limits. They go in the
//! real `string` table, which the metatable every string shares reads,
//! so `s:find(p)` reaches them too.
//!
//! Rust does the matching and hands Lua the results. Lua calls a
//! function or reads a table that `string.gsub` takes as its
//! replacement, so an error such a function raises reaches the script as
//! it was raised. Each keeps the results and the error lines of the C
//! function, with the exception `crate::library` names for a call made
//! as a `return`.

use std::sync::Arc;

use mlua::{IntoLua, Lua, MultiValue, Value};

use crate::limits::{self, Limits, INTERNAL_CHUNK};
use crate::pattern::{no_specials, start_position, Capture, Fail, Matcher};

/// The Lua that puts the wrapped functions in the string table. It takes
/// the Rust functions that match.
const STRINGS: &str = r#"
local find, match, gmatch_start, gmatch_next, gsub_text, gsub_scan = ...
local error, type = error, type
local format, sub = string.format, string.sub
local concat, unpack = table.concat, table.unpack
local str = string

-- Hand back what a Rust function found, or raise the error it names at
-- the line that called the string function.
local function found(ok, ...)
  if ok then
    return ...
  end
  error((...), 2)
end

function str.find(...)
  return found(find(...))
end

function str.match(...)
  return found(match(...))
end

local function gmatch_found(state, ok, e, ...)
  if not ok then
    error(e, 2)
  end
  if e ~= nil then
    state[3], state[4] = e, e
    return ...
  end
end

function str.gmatch(...)
  local ok, s, p, at = gmatch_start(...)
  if not ok then
    error(s, 2)
  end
  local state = { s, p, at, -1 }
  return function()
    return gmatch_found(state, gmatch_next(state[1], state[2], state[3], state[4]))
  end
end

-- gsub with a function or a table, whose calls and reads run here.
local function gsub_call(s, p, repl, max_s)
  local is_function = type(repl) == "function"
  local parts, np = {}, 0
  local at, last, n, changed = 0, -1, 0, false
  while n < max_s do
    local q, e, caps = gsub_scan(s, p, at, last)
    if q == false then
      error(e, 2)
    end
    if q == nil then
      break
    end
    np = np + 1
    parts[np] = sub(s, at + 1, q)
    n = n + 1
    local v
    if is_function then
      v = repl(unpack(caps, 1, caps.n))
    else
      v = repl[caps[1]]
    end
    np = np + 1
    if not v then
      parts[np] = sub(s, q + 1, e)
    elseif type(v) == "string" or type(v) == "number" then
      parts[np] = v
      changed = true
    else
      error(format("invalid replacement value (a %s)", type(v)), 2)
    end
    at, last = e, e
    if caps.anchor then
      break
    end
  end
  if not changed then
    return s, n
  end
  np = np + 1
  parts[np] = sub(s, at + 1)
  return concat(parts), n
end

function str.gsub(...)
  local ok, result, n, p, max_s = gsub_text(...)
  if not ok then
    error(result, 2)
  end
  if p == nil then
    return result, n
  end
  return gsub_call(result, p, (select(3, ...)), max_s)
end
"#;

/// Put the wrapped functions in the string table.
pub(crate) fn install(lua: &Lua, limits: &Arc<Limits>) -> mlua::Result<()> {
    let find = {
        let limits = Arc::clone(limits);
        lua.create_function(move |lua, args: MultiValue| find_or_match(lua, &limits, &args, true))?
    };
    let matcher = {
        let limits = Arc::clone(limits);
        lua.create_function(move |lua, args: MultiValue| find_or_match(lua, &limits, &args, false))?
    };
    let gmatch_start = lua.create_function(|lua, args: MultiValue| gmatch_start(lua, &args))?;
    let gmatch_next = {
        let limits = Arc::clone(limits);
        lua.create_function(move |lua, args: (mlua::String, mlua::String, i64, i64)| {
            gmatch_next(lua, &limits, args)
        })?
    };
    let gsub_text = {
        let limits = Arc::clone(limits);
        lua.create_function(move |lua, args: MultiValue| gsub_text(lua, &limits, &args))?
    };
    let gsub_scan = {
        let limits = Arc::clone(limits);
        lua.create_function(move |lua, args: (mlua::String, mlua::String, i64, i64)| {
            gsub_scan(lua, &limits, args)
        })?
    };
    lua.load(STRINGS).set_name(INTERNAL_CHUNK).call::<()>((
        find,
        matcher,
        gmatch_start,
        gmatch_next,
        gsub_text,
        gsub_scan,
    ))
}

/// The error a bad argument raises, as the C library words it.
fn bad(arg: usize, fname: &str, extra: &str) -> String {
    format!("bad argument #{arg} to '{fname}' ({extra})")
}

/// How the C library names the type of `value`, as `luaL_typeerror`
/// does, a table's `__name` included.
fn type_name(value: &Value) -> String {
    match value {
        Value::Nil => "nil".into(),
        Value::Boolean(_) => "boolean".into(),
        Value::LightUserData(_) => "light userdata".into(),
        Value::Integer(_) | Value::Number(_) => "number".into(),
        Value::String(_) => "string".into(),
        Value::Table(t) => t
            .metatable()
            .and_then(|mt| mt.raw_get::<Option<mlua::String>>("__name").ok().flatten())
            .map_or_else(|| "table".into(), |name| name.to_string_lossy()),
        Value::Function(_) => "function".into(),
        Value::Thread(_) => "thread".into(),
        _ => "userdata".into(),
    }
}

/// Argument `arg` of `args` as a string, as `luaL_checklstring` reads
/// it, a number turned into one.
fn string_arg(
    lua: &Lua,
    args: &MultiValue,
    arg: usize,
    fname: &str,
) -> Result<mlua::String, String> {
    let Some(value) = args.get(arg - 1) else {
        return Err(bad(arg, fname, "string expected, got no value"));
    };
    match lua.coerce_string(value.clone()) {
        Ok(Some(s)) => Ok(s),
        _ => Err(bad(
            arg,
            fname,
            &format!("string expected, got {}", type_name(value)),
        )),
    }
}

/// Argument `arg` of `args` as an integer, as `luaL_optinteger` reads
/// it, or None when it is absent or nil.
fn opt_integer(
    lua: &Lua,
    args: &MultiValue,
    arg: usize,
    fname: &str,
) -> Result<Option<i64>, String> {
    let value = match args.get(arg - 1) {
        None | Some(Value::Nil) => return Ok(None),
        Some(value) => value,
    };
    if let Ok(Some(i)) = lua.coerce_integer(value.clone()) {
        return Ok(Some(i));
    }
    if let Ok(Some(_)) = lua.coerce_number(value.clone()) {
        return Err(bad(arg, fname, "number has no integer representation"));
    }
    Err(bad(
        arg,
        fname,
        &format!("number expected, got {}", type_name(value)),
    ))
}

/// What a Rust function hands the Lua that wraps it: true and the
/// results, or false and the error.
fn answer(lua: &Lua, result: Result<Vec<Value>, String>) -> mlua::Result<MultiValue> {
    match result {
        Ok(values) => Ok(std::iter::once(Value::Boolean(true))
            .chain(values)
            .collect()),
        Err(text) => Ok(MultiValue::from_vec(vec![
            Value::Boolean(false),
            text.into_lua(lua)?,
        ])),
    }
}

/// Turn a matcher's failure into the error line it gives, or raise the
/// stop when the call ran out of time.
fn failed(fail: Fail) -> mlua::Result<String> {
    match fail {
        Fail::Error(text) => Ok(text),
        Fail::Stopped => Err(limits::stopped()),
    }
}

/// The Lua values of `captures` of a match in `src`.
fn capture_values(lua: &Lua, src: &[u8], captures: &[Capture]) -> mlua::Result<Vec<Value>> {
    captures
        .iter()
        .map(|capture| match *capture {
            Capture::Text(from, to) => Ok(Value::String(lua.create_string(&src[from..to])?)),
            Capture::Position(at) => Ok(Value::Integer(i64::try_from(at).unwrap_or(i64::MAX))),
        })
        .collect()
}

fn index(at: usize) -> Value {
    Value::Integer(i64::try_from(at).unwrap_or(i64::MAX))
}

/// `string.find`, or with `find` false `string.match`.
fn find_or_match(
    lua: &Lua,
    limits: &Limits,
    args: &MultiValue,
    find: bool,
) -> mlua::Result<MultiValue> {
    let fname = if find { "find" } else { "match" };
    let parsed = (|| {
        let s = string_arg(lua, args, 1, fname)?;
        let p = string_arg(lua, args, 2, fname)?;
        let init = opt_integer(lua, args, 3, fname)?.unwrap_or(1);
        Ok((s, p, init))
    })();
    let (s, p, init) = match parsed {
        Ok(parsed) => parsed,
        Err(text) => return answer(lua, Err(text)),
    };
    let src = s.as_bytes();
    let pat = p.as_bytes();
    let init = start_position(init, src.len()) - 1;
    if init > src.len() {
        return answer(lua, Ok(vec![Value::Nil]));
    }
    let plain = find
        && args
            .get(3)
            .is_some_and(|v| !matches!(v, Value::Nil | Value::Boolean(false)));
    if find && (plain || no_specials(&pat)) {
        let found = memchr::memmem::find(&src[init..], &pat).map(|at| init + at);
        return answer(
            lua,
            Ok(match found {
                Some(at) => vec![index(at + 1), index(at + pat.len())],
                None => vec![Value::Nil],
            }),
        );
    }
    let anchor = pat.first() == Some(&b'^');
    let pat = &pat[usize::from(anchor)..];
    let mut out_of_time = || limits.out_of_time(lua);
    let mut m = Matcher::new(&src, pat, &mut out_of_time);
    let mut start = init;
    let found = loop {
        match m.match_at(start, 0) {
            // find hands back where the match is and its captures, and
            // match the captures, or the whole match with none.
            Ok(Some(end)) => break m.captures(start, end, !find).map(|caps| Some((end, caps))),
            Ok(None) => {}
            Err(fail) => break Err(fail),
        }
        start += 1;
        if anchor || start > src.len() {
            break Ok(None);
        }
        if let Err(fail) = m.step() {
            break Err(fail);
        }
    };
    match found {
        Ok(Some((end, caps))) => {
            let mut values = Vec::with_capacity(caps.len() + 2);
            if find {
                values.extend([index(start + 1), index(end)]);
            }
            values.extend(capture_values(lua, &src, &caps)?);
            answer(lua, Ok(values))
        }
        Ok(None) => answer(lua, Ok(vec![Value::Nil])),
        Err(fail) => answer(lua, Err(failed(fail)?)),
    }
}

/// The start of `string.gmatch`: its subject and pattern as strings and
/// the index its search starts from, 0 for the first byte.
fn gmatch_start(lua: &Lua, args: &MultiValue) -> mlua::Result<MultiValue> {
    let parsed = (|| {
        let s = string_arg(lua, args, 1, "gmatch")?;
        let p = string_arg(lua, args, 2, "gmatch")?;
        let init = opt_integer(lua, args, 3, "gmatch")?.unwrap_or(1);
        Ok((s, p, init))
    })();
    match parsed {
        Ok((s, p, init)) => {
            let len = s.as_bytes().len();
            // A start past the end finds nothing.
            let at = (start_position(init, len) - 1).min(len + 1);
            answer(lua, Ok(vec![Value::String(s), Value::String(p), index(at)]))
        }
        Err(text) => answer(lua, Err(text)),
    }
}

/// The next match of a `string.gmatch` loop from the index `at`, never
/// one that ends where the one before ended at `last`: true, the index
/// it ends at and its captures, or true alone when no match is left.
fn gmatch_next(
    lua: &Lua,
    limits: &Limits,
    (s, p, at, last): (mlua::String, mlua::String, i64, i64),
) -> mlua::Result<MultiValue> {
    let src = s.as_bytes();
    let pat = p.as_bytes();
    let mut out_of_time = || limits.out_of_time(lua);
    let mut m = Matcher::new(&src, &pat, &mut out_of_time);
    let mut start = usize::try_from(at).unwrap_or(0);
    let found = loop {
        if start > src.len() {
            break Ok(None);
        }
        match m.match_at(start, 0) {
            Ok(Some(end)) if i64::try_from(end).ok() != Some(last) => {
                break m.captures(start, end, true).map(|caps| Some((end, caps)));
            }
            Ok(_) => {}
            Err(fail) => break Err(fail),
        }
        start += 1;
        if let Err(fail) = m.step() {
            break Err(fail);
        }
    };
    match found {
        Ok(Some((end, caps))) => {
            let mut values = vec![index(end)];
            values.extend(capture_values(lua, &src, &caps)?);
            answer(lua, Ok(values))
        }
        Ok(None) => answer(lua, Ok(Vec::new())),
        Err(fail) => answer(lua, Err(failed(fail)?)),
    }
}

/// `string.gsub` with a string or a number as its replacement, all of
/// it here: true, the new string and the count. With a function or a
/// table as its replacement: true, the subject, nil, the pattern and the
/// most replacements to make, for the Lua that calls or reads it.
fn gsub_text(lua: &Lua, limits: &Limits, args: &MultiValue) -> mlua::Result<MultiValue> {
    let parsed = (|| {
        let s = string_arg(lua, args, 1, "gsub")?;
        let p = string_arg(lua, args, 2, "gsub")?;
        let repl = args.get(2).cloned();
        let max_s = opt_integer(lua, args, 4, "gsub")?;
        let kind = match &repl {
            Some(Value::Integer(_) | Value::Number(_) | Value::String(_)) => Ok(true),
            Some(Value::Function(_) | Value::Table(_)) => Ok(false),
            Some(other) => Err(type_name(other)),
            None => Err("no value".to_string()),
        };
        match kind {
            Ok(text) => Ok((s, p, repl, max_s, text)),
            Err(got) => Err(bad(
                3,
                "gsub",
                &format!("string/function/table expected, got {got}"),
            )),
        }
    })();
    let (s, p, repl, max_s, text) = match parsed {
        Ok(parsed) => parsed,
        Err(text) => return answer(lua, Err(text)),
    };
    let len = s.as_bytes().len();
    let max_s = max_s.unwrap_or_else(|| i64::try_from(len).unwrap_or(i64::MAX).saturating_add(1));
    if !text {
        return answer(
            lua,
            Ok(vec![
                Value::String(s),
                Value::Nil,
                Value::String(p),
                Value::Integer(max_s),
            ]),
        );
    }
    let Some(Ok(Some(repl))) = repl.map(|v| lua.coerce_string(v)) else {
        return answer(lua, Err(bad(3, "gsub", "string expected")));
    };
    let src = s.as_bytes();
    let pat = p.as_bytes();
    let repl_bytes = repl.as_bytes();
    let anchor = pat.first() == Some(&b'^');
    let pat = &pat[usize::from(anchor)..];
    let room = limits.memory_room(lua);
    let mut out_of_time = || limits.out_of_time(lua);
    let mut m = Matcher::new(&src, pat, &mut out_of_time);
    let mut out = Vec::new();
    let mut at = 0;
    let mut last = None;
    let mut n: i64 = 0;
    let result = (|| {
        while n < max_s {
            m.step()?;
            match m.match_at(at, 0)? {
                Some(end) if last != Some(end) => {
                    n += 1;
                    m.add_replacement(&mut out, &repl_bytes, at, end)?;
                    at = end;
                    last = Some(end);
                }
                _ if at < src.len() => {
                    out.push(src[at]);
                    at += 1;
                }
                _ => break,
            }
            if out.len() > room {
                return Ok(false);
            }
            if anchor {
                break;
            }
        }
        Ok(true)
    })();
    match result {
        Ok(true) => {}
        // What Rust built would take the call past its memory, as the
        // buffer the C function builds in Lua would.
        Ok(false) => return Err(mlua::Error::MemoryError("not enough memory".into())),
        Err(fail) => return answer(lua, Err(failed(fail)?)),
    }
    if n == 0 {
        return answer(lua, Ok(vec![Value::String(s.clone()), Value::Integer(0)]));
    }
    out.extend_from_slice(&src[at..]);
    let result = lua.create_string(&out)?;
    answer(lua, Ok(vec![Value::String(result), Value::Integer(n)]))
}

/// The next match a `string.gsub` with a function or a table makes,
/// from the index `at`, never an empty one where the one before ended
/// at `last`: its start, its end and a table of its captures, nil when
/// none is left, or false and the error.
fn gsub_scan(
    lua: &Lua,
    limits: &Limits,
    (s, p, at, last): (mlua::String, mlua::String, i64, i64),
) -> mlua::Result<MultiValue> {
    let src = s.as_bytes();
    let pat = p.as_bytes();
    let anchor = pat.first() == Some(&b'^');
    let pat = &pat[usize::from(anchor)..];
    let mut out_of_time = || limits.out_of_time(lua);
    let mut m = Matcher::new(&src, pat, &mut out_of_time);
    let mut start = usize::try_from(at).unwrap_or(0);
    let found = loop {
        match m.match_at(start, 0) {
            Ok(Some(end)) if i64::try_from(end).ok() != Some(last) => {
                break m.captures(start, end, true).map(|caps| Some((end, caps)));
            }
            Ok(_) => {}
            Err(fail) => break Err(fail),
        }
        if anchor || start >= src.len() {
            break Ok(None);
        }
        start += 1;
        if let Err(fail) = m.step() {
            break Err(fail);
        }
    };
    match found {
        Ok(Some((end, caps))) => {
            let values = capture_values(lua, &src, &caps)?;
            let table = lua.create_sequence_from(values)?;
            table.raw_set("n", caps.len())?;
            if anchor {
                table.raw_set("anchor", true)?;
            }
            Ok(MultiValue::from_vec(vec![
                index(start),
                index(end),
                Value::Table(table),
            ]))
        }
        Ok(None) => Ok(MultiValue::new()),
        Err(fail) => Ok(MultiValue::from_vec(vec![
            Value::Boolean(false),
            failed(fail)?.into_lua(lua)?,
        ])),
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::{same_as_stock, typed_in_time};
    use crate::Owner;

    #[test]
    fn find_answers_as_the_c_one_does() {
        same_as_stock(&[
            "return table.concat({string.find('hello world', 'o w')}, ',')",
            "return table.concat({string.find('hello world', 'o', 6)}, ',')",
            "return table.concat({string.find('hello world', 'l+')}, ',')",
            "return tostring(string.find('hello', 'xyz'))",
            "return table.concat({string.find('a.b', '.', 1, true)}, ',')",
            "return table.concat({string.find('a.b', '.', nil, true)}, ',')",
            "return table.concat({string.find('a.b', '%.')}, ',')",
            "return table.concat({string.find('hello', '')}, ',')",
            "return tostring(string.find('hello', '', 10))",
            "return table.concat({string.find('hello', '', 6)}, ',')",
            "return table.concat({string.find('hello', 'l', -2)}, ',')",
            "return table.concat({string.find('hello', 'h', -10)}, ',')",
            "return table.concat({string.find('key = value', '(%w+)%s*=%s*(%w+)')}, ',')",
            "return table.concat({string.find('  x', '^%s*()')}, ',')",
            "return table.concat({string.find('THE (quick) fox', '%((%a+)%)')}, ',')",
            "return table.concat({string.find('f(a(b)c)d', '%b()')}, ',')",
            "return table.concat({string.find('THE quick brown', '%f[%a]%a+', 4)}, ',')",
            "return table.concat({string.find('abcabc', '(a)(b)(c)%1')}, ',')",
            "return table.concat({string.find(12345, 34)}, ',')",
            "local s = ('x'):rep(10) return s:find('x', 3)",
            "return select(2, pcall(function() string.find('a', '[a') end))",
            "return select(2, pcall(function() string.find('a', '%') end))",
            "return select(2, pcall(function() string.find('a', '(()') end))",
            "return select(2, pcall(function() string.find('a', '%1') end))",
            "return select(2, pcall(function() string.match('a', 'a)') end))",
            "return tostring(string.find('a)', 'a)'))",
            "return select(2, pcall(function() string.find('a', '%f') end))",
            "return select(2, pcall(function() string.find('a', '%b') end))",
            "return select(2, pcall(function() string.find() end))",
            "return select(2, pcall(function() string.find('a', {}) end))",
            "return select(2, pcall(function() string.find('a', setmetatable({}, {__name = 'Thing'})) end))",
            "return select(2, pcall(function() string.find('a', 'a', 1.5) end))",
            "return select(2, pcall(function() string.find('a', 'a', 'x') end))",
            "return select(2, pcall(function() string.find(string.rep('a', 40), string.rep('(a)', 33)) end))",
            "return select(2, pcall(function() string.find(string.rep('a', 300), string.rep('a?', 300) .. string.rep('a', 300)) end))",
        ]);
    }

    #[test]
    fn match_answers_as_the_c_one_does() {
        same_as_stock(&[
            "return string.match('hello 123 world', '%d+')",
            "return table.concat({string.match('key=val', '(%w+)=(%w+)')}, ',')",
            "return table.concat({string.match('hello', '()ll()')}, ',')",
            "return string.match('  trim  ', '^%s*(.-)%s*$')",
            "return tostring(string.match('abc', '^b'))",
            "return string.match('[[x]]', '%[(%b[])%]')",
            "return string.match('hello', '.-', 2) .. '|'",
            "return string.match('a\\0b', 'a%zb') ~= nil and 'z' or 'no'",
            "return string.match('x1y2', '%a%d', 3)",
            "return tostring(string.match('', '.*'))",
            "return string.match('Ab9_ \\t', '[%w_]+')",
            "return string.match('a-b', '[a%-b]+')",
            "return string.match('abc]', '[]a-c]+')",
            "return string.match('x^y', '[%^x]+')",
            "return string.match('abc', '[^b]+')",
            "return string.match('A1 b2', '%u%d')",
            "return string.match('tab\\there', '%c')",
            "return string.match('ff 0x1F zz', '%x+', 3)",
            "return string.match('a,b;c', '%p')",
            "return string.match('\\v x', '%s')",
            "return string.match('caf\\195\\169', '%a+$')",
            "return tostring(string.match('abc', 'abcd?$'))",
        ]);
    }

    #[test]
    fn gmatch_answers_as_the_c_one_does() {
        same_as_stock(&[
            "local t = {} for w in string.gmatch('one two  three', '%a+') do t[#t + 1] = w end return table.concat(t, ',')",
            "local t = {} for k, v in string.gmatch('a=1, b=2', '(%w+)=(%w+)') do t[#t + 1] = k .. v end return table.concat(t, ',')",
            "local t = {} for p in string.gmatch('abc', '()') do t[#t + 1] = p end return table.concat(t, ',')",
            "local t = {} for w in string.gmatch('abc', 'x*') do t[#t + 1] = '[' .. w .. ']' end return table.concat(t)",
            "local t = {} for w in string.gmatch('hello world', '%a+', 3) do t[#t + 1] = w end return table.concat(t, ',')",
            "local t = {} for w in string.gmatch('hello world', '%a+', -5) do t[#t + 1] = w end return table.concat(t, ',')",
            "local t = {} for w in string.gmatch('^a^b', '^%a') do t[#t + 1] = w end return table.concat(t, ',')",
            "local t = {} for w in ('a,b,,c'):gmatch('([^,]*)') do t[#t + 1] = '<' .. w .. '>' end return table.concat(t)",
            "return select(2, pcall(function() for w in string.gmatch('a', '[') do end end))",
            "return select(2, pcall(function() string.gmatch() end))",
            "local it = string.gmatch('ab', '.') it() it() return tostring((it()))",
            "local it = string.gmatch('abc', '.', 10) return tostring((it()))",
        ]);
    }

    #[test]
    fn gsub_answers_as_the_c_one_does() {
        same_as_stock(&[
            "return table.concat({string.gsub('hello world', 'o', '0')}, ',')",
            "return table.concat({string.gsub('hello world', 'o', '0', 1)}, ',')",
            "return table.concat({string.gsub('hello', '', '-')}, ',')",
            "return table.concat({string.gsub('abc', '%w', '%0%0')}, ',')",
            "return table.concat({string.gsub('hello world', '(%w+)', '<%1>')}, ',')",
            "return table.concat({string.gsub('hello world', '(%w+) (%w+)', '%2 %1')}, ',')",
            "return table.concat({string.gsub('abc', '()', '%1')}, ',')",
            "return table.concat({string.gsub('abc', 'b', '%%')}, ',')",
            "return table.concat({string.gsub('abc', '%w', '%1')}, ',')",
            "return table.concat({string.gsub('abc', 'b', 5)}, ',')",
            "return table.concat({string.gsub(123, 2, 9)}, ',')",
            "return table.concat({string.gsub('abc', '^a', 'x')}, ',')",
            "return table.concat({string.gsub('abc', '^b', 'x')}, ',')",
            "return table.concat({string.gsub('abc', 'x*', '-')}, ',')",
            "return table.concat({string.gsub('abc', 'b', 'x', 0)}, ',')",
            "return table.concat({string.gsub('abc', 'b', 'x', -1)}, ',')",
            "return table.concat({string.gsub('abc', '(', 'x')}, ',')",
            "return table.concat({('x y'):gsub('%s', '_')}, ',')",
            "return select(2, pcall(function() string.gsub('abc', 'b', '%2') end))",
            "return select(2, pcall(function() string.gsub('abc', 'b', '%x') end))",
            "return select(2, pcall(function() string.gsub('abc', 'b', '%') end))",
            "return select(2, pcall(function() string.gsub('abc', '.') end))",
            "return select(2, pcall(function() string.gsub('abc', '.', true) end))",
            "return select(2, pcall(function() string.gsub('abc', '.', 'x', 'y') end))",
            "return select(2, pcall(function() string.gsub('abc', '[', 'x') end))",
        ]);
    }

    #[test]
    fn gsub_calls_a_function_and_reads_a_table_as_the_c_one_does() {
        same_as_stock(&[
            "return table.concat({string.gsub('hello world', '%w+', function(w) return w:upper() end)}, ',')",
            "return table.concat({string.gsub('hello world', '%w+', function(w) if w == 'world' then return false end return '[' .. w .. ']' end)}, ',')",
            "return table.concat({string.gsub('$name is $age', '%$(%w+)', {name = 'Orla', age = 40})}, ',')",
            "return table.concat({string.gsub('$name is $x', '%$(%w+)', {name = 'Orla'})}, ',')",
            "return table.concat({string.gsub('abc', '()', function(p) return p end)}, ',')",
            "return table.concat({string.gsub('abc', '', function() return '-' end)}, ',')",
            "return table.concat({string.gsub('abc', '.', function(c) return c:byte() end)}, ',')",
            "return table.concat({string.gsub('abc', '^.', function(c) return c:upper() end)}, ',')",
            "return table.concat({string.gsub('a b c', '(%w)', function(c) return c .. c end, 2)}, ',')",
            "return table.concat({string.gsub('abc', '.', function() end)}, ',')",
            "return select(2, pcall(function() string.gsub('abc', '.', {b = true}) end))",
            "return select(2, pcall(function() string.gsub('abc', '.', function() return {} end) end))",
            "local ok, e = pcall(string.gsub, 'abc', '.', function() error({code = 7}) end) return type(e) .. tostring(e.code)",
            "return select(2, pcall(function() string.gsub('abc', '.', function() error('inner') end) end))",
        ]);
    }

    #[test]
    fn a_pattern_that_backtracks_stops_on_the_time_limit() {
        for code in [
            "string.rep('a', 3000):find('.-.-.-.-.-.-b')",
            "local s = string.rep('a', 3000) s:match('.-.-.-.-.-.-b')",
            "for w in string.rep('a', 3000):gmatch('.-.-.-.-.-.-b') do end",
            "string.gsub(string.rep('a', 3000), '.-.-.-.-.-.-b', 'x')",
            "string.gsub(string.rep('a', 3000), '.-.-.-.-.-.-b', function() end)",
            "string.find(string.rep('a', 3000), string.rep('(.-)', 6) .. 'b%1')",
        ] {
            let outcome = typed_in_time(code);
            assert_eq!(outcome.stopped, [Owner::Typed], "{code}");
        }
        // A plugin calls the same functions through its read only view.
        let outcome = crate::test_support::returns_in_time(|| {
            let mut e = crate::ScriptEngine::new().unwrap();
            e.load_script(
                Owner::Plugin("pat".into()),
                "@pat/main.lua",
                "local s = string.rep('a', 3000) local r = s:find('.-.-.-.-.-.-b')",
            )
        });
        assert_eq!(outcome.stopped, [Owner::Plugin("pat".into())]);
    }

    #[test]
    fn a_plain_search_takes_time_in_line_with_its_subject() {
        let outcome = typed_in_time(
            "local hay = string.rep('a', 4 * 1024 * 1024) \
             local needle = string.rep('a', 2 * 1024 * 1024) .. 'b' \
             mud.echo(tostring(hay:find(needle, 1, true)) .. tostring(hay:find(needle)))",
        );
        assert_eq!(
            outcome.actions,
            [crate::Action::Echo("nilnil".into())],
            "{:?}",
            outcome.actions
        );
    }

    #[test]
    fn a_replacement_past_the_memory_limit_stops_the_call() {
        // Each %0 copies the whole 256 KB match, so the 160 of them build
        // 40 MB in a few ms. A plain replacement over many matches gets
        // there too, but takes most of the 100 ms in a debug build and
        // runs out of time first while other tests load the machine.
        let outcome = typed_in_time(
            "local line = string.rep('a', 256 * 1024) \
             string.gsub(line, '^.*', string.rep('%0', 160))",
        );
        assert_eq!(outcome.stopped, [Owner::Typed]);
        assert_eq!(
            crate::test_support::error_lines(&outcome),
            ["Vosh stopped your #lua line. One call used more than 32 MB."]
        );
    }
}
