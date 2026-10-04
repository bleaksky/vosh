//! The standard library functions that would loop inside C, where the
//! hook cannot look. A `__len` that names a huge length, or arguments
//! that name a huge range, kept `table.insert`, `table.remove`,
//! `table.move`, `table.sort` and `table.concat` busy inside one C call
//! for as long as they liked, with metamethods written in C that use no
//! memory. Each one here does that work in Lua, where the hook sees
//! every step. `table.sort` hands a small plain table to the C sort, and
//! `table.concat` hands C the values it read, since C then has a known
//! amount to do.
//!
//! Each keeps the results and the error lines of the C function, but for
//! two things. A value that is not a table never stands in for one, and
//! an error in a call made as `return table.insert(...)` names the line
//! of the function that made it, since Lua forgets the frame of a Lua
//! function that returns a call.

use mlua::Lua;

use crate::limits::INTERNAL_CHUNK;

/// The largest plain table the C `table.sort` sorts, well inside the
/// time limit. A larger one sorts in Lua.
const C_SORT_MAX: u32 = 100_000;

/// The Lua that wraps the table functions. It takes the size limit of
/// the C sort.
const TABLES: &str = r##"
local C_SORT_MAX = ...
local error, getmetatable, select, tonumber, type = error, getmetatable, select, tonumber, type
local format = string.format
local tointeger, ult, maxinteger = math.tointeger, math.ult, math.maxinteger
local floor = math.floor
local clock, time = os.clock, os.time
local tbl = table
local C = {
  sort = tbl.sort,
  concat = tbl.concat,
}

-- The line the C library gives a bad argument.
local function bad(arg, fname, extra)
  return format("bad argument #%d to '%s' (%s)", arg, fname, extra)
end

-- The type of argument `arg` of `count` arguments, as the C library
-- names it.
local function typename(v, arg, count)
  if arg > count then
    return "no value"
  end
  return type(v)
end

-- The length of the table `t`, for the function `fname` that took it
-- as its first argument of `count`. An error names the line that called
-- it.
local function getn(t, fname, count)
  if type(t) ~= "table" then
    error(bad(1, fname, "table expected, got " .. typename(t, 1, count)), 3)
  end
  local n = tointeger(#t)
  if n == nil then
    error("object length is not an integer", 3)
  end
  return n
end

-- `v`, argument `arg` of `count` given to `fname`, as an integer.
local function checkint(v, arg, fname, count)
  local i = tointeger(v)
  if i == nil then
    if tonumber(v) ~= nil then
      error(bad(arg, fname, "number has no integer representation"), 3)
    end
    error(bad(arg, fname, "number expected, got " .. typename(v, arg, count)), 3)
  end
  return i
end

function tbl.insert(...)
  local count = select("#", ...)
  local t, pos, v = ...
  local e = getn(t, "insert", count) + 1
  if count == 2 then
    t[e] = pos
    return
  end
  if count ~= 3 then
    error("wrong number of arguments to 'insert'", 2)
  end
  pos = checkint(pos, 2, "insert", count)
  if not ult(pos - 1, e) then
    error(bad(2, "insert", "position out of bounds"), 2)
  end
  local i = e
  while i > pos do
    t[i] = t[i - 1]
    i = i - 1
  end
  t[pos] = v
end

function tbl.remove(...)
  local count = select("#", ...)
  local t, pos = ...
  local size = getn(t, "remove", count)
  if pos == nil then
    pos = size
  else
    pos = checkint(pos, 2, "remove", count)
  end
  if pos ~= size and ult(size, pos - 1) then
    error(bad(2, "remove", "position out of bounds"), 2)
  end
  local result = t[pos]
  while pos < size do
    t[pos] = t[pos + 1]
    pos = pos + 1
  end
  t[pos] = nil
  return result
end

function tbl.move(...)
  local count = select("#", ...)
  local a1, f, e, t, a2 = ...
  f = checkint(f, 2, "move", count)
  e = checkint(e, 3, "move", count)
  t = checkint(t, 4, "move", count)
  local tt = a2
  if tt == nil then
    tt = a1
  end
  if type(a1) ~= "table" then
    error(bad(1, "move", "table expected, got " .. typename(a1, 1, count)), 2)
  end
  if type(tt) ~= "table" then
    error(bad(a2 == nil and 1 or 5, "move", "table expected, got " .. type(tt)), 2)
  end
  if e >= f then
    if not (f > 0 or e < maxinteger + f) then
      error(bad(3, "move", "too many elements to move"), 2)
    end
    local n = e - f + 1
    if not (t <= maxinteger - n + 1) then
      error(bad(4, "move", "destination wrap around"), 2)
    end
    if t > e or t <= f or (a2 ~= nil and a1 ~= tt) then
      for i = 0, n - 1 do
        tt[t + i] = a1[f + i]
      end
    else
      for i = n - 1, 0, -1 do
        tt[t + i] = a1[f + i]
      end
    end
  end
  return tt
end

function tbl.concat(...)
  local count = select("#", ...)
  local t, sep, i, j = ...
  local last = getn(t, "concat", count)
  if sep == nil then
    sep = ""
  elseif type(sep) ~= "string" and type(sep) ~= "number" then
    error(bad(2, "concat", "string expected, got " .. type(sep)), 2)
  end
  if i == nil then
    i = 1
  else
    i = checkint(i, 3, "concat", count)
  end
  if j ~= nil then
    last = checkint(j, 4, "concat", count)
  end
  -- Read each value once in Lua, then let C join them. A plain table
  -- holds the values it handed over, so C joins it as it is.
  local plain = getmetatable(t) == nil
  local first = i
  local parts, n = {}, 0
  while i <= last do
    local v = t[i]
    if type(v) ~= "string" and type(v) ~= "number" then
      error(format("invalid value (%s) at index %d in table for 'concat'", type(v), i), 2)
    end
    if not plain then
      n = n + 1
      parts[n] = v
    end
    if i == last then
      break
    end
    i = i + 1
  end
  if plain then
    return C.concat(t, sep, first, last)
  end
  return C.concat(parts, sep)
end

-- What the C sort does, step for step, from ltablib.c.
local RANLIMIT = 100

local function lt(comp, a, b)
  if comp == nil then
    return a < b
  end
  if comp(a, b) then
    return true
  end
  return false
end

local function randomize()
  return (floor(clock() * 1000000) + time()) % 4294967296
end

local function choose_pivot(lo, up, rnd)
  local r4 = (up - lo) // 4
  return rnd % (r4 * 2) + (lo + r4)
end

-- Nil when the order function is not a valid order.
local function partition(t, lo, up, pivot, comp)
  local i, j = lo, up - 1
  while true do
    i = i + 1
    local ai = t[i]
    while lt(comp, ai, pivot) do
      if i == up - 1 then
        return nil
      end
      i = i + 1
      ai = t[i]
    end
    j = j - 1
    local aj = t[j]
    while lt(comp, pivot, aj) do
      if j < i then
        return nil
      end
      j = j - 1
      aj = t[j]
    end
    if j < i then
      t[up - 1] = ai
      t[i] = pivot
      return i
    end
    t[i] = aj
    t[j] = ai
  end
end

-- False when the order function is not a valid order.
local function auxsort(t, lo, up, rnd, comp)
  while lo < up do
    local a, b = t[lo], t[up]
    if lt(comp, b, a) then
      t[lo] = b
      t[up] = a
    end
    if up - lo == 1 then
      break
    end
    local p
    if up - lo < RANLIMIT or rnd == 0 then
      p = (lo + up) // 2
    else
      p = choose_pivot(lo, up, rnd)
    end
    a, b = t[p], t[lo]
    if lt(comp, a, b) then
      t[p] = b
      t[lo] = a
    else
      b = t[up]
      if lt(comp, b, a) then
        t[p] = b
        t[up] = a
      end
    end
    if up - lo == 2 then
      break
    end
    local pivot = t[p]
    t[p] = t[up - 1]
    t[up - 1] = pivot
    p = partition(t, lo, up, pivot, comp)
    if p == nil then
      return false
    end
    local n
    if p - lo < up - p then
      if not auxsort(t, lo, p - 1, rnd, comp) then
        return false
      end
      n = p - lo
      lo = p + 1
    else
      if not auxsort(t, p + 1, up, rnd, comp) then
        return false
      end
      n = up - p
      up = p - 1
    end
    if (up - lo) // 128 > n then
      rnd = randomize()
    end
  end
  return true
end

function tbl.sort(...)
  local count = select("#", ...)
  local t, comp = ...
  local n = getn(t, "sort", count)
  if n > 1 then
    if n >= 2147483647 then
      error(bad(1, "sort", "array too big"), 2)
    end
    if comp ~= nil and type(comp) ~= "function" then
      error(bad(2, "sort", "function expected, got " .. type(comp)), 2)
    end
    -- C sorts a small plain table quickly with no order function to
    -- call, which could make it raise from here.
    if comp == nil and n <= C_SORT_MAX and getmetatable(t) == nil then
      return C.sort(t)
    end
    if not auxsort(t, 1, n, 0, comp) then
      error("invalid order function for sorting", 2)
    end
  end
end
"##;

/// Put the wrapped table functions in the `table` library.
pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    lua.load(TABLES)
        .set_name(INTERNAL_CHUNK)
        .call::<()>(C_SORT_MAX)
}

#[cfg(test)]
mod tests {
    use crate::test_support::{same_as_stock, typed_in_time};
    use crate::Owner;

    #[test]
    fn the_table_functions_answer_as_the_c_ones_do() {
        same_as_stock(&[
            "local t = {1, 2, 3} table.insert(t, 'x') return table.concat(t, ',')",
            "local t = {1, 2, 3} table.insert(t, 1, 'x') return table.concat(t, ',')",
            "local t = {1, 2, 3} table.insert(t, 4, 'x') return table.concat(t, ',')",
            "local t = {1, 2, 3} table.insert(t, '2', 'x') return table.concat(t, ',')",
            "local t = {1, 2, 3} return select(2, pcall(function() table.insert(t, 5, 'x') end))",
            "return select(2, pcall(function() table.insert({}, 0, 'x') end))",
            "return select(2, pcall(function() table.insert({}, 1, 2, 3) end))",
            "return select(2, pcall(function() table.insert({}) end))",
            "return select(2, pcall(function() table.insert(nil, 1) end))",
            "return select(2, pcall(function() table.insert({}, 1.5, 1) end))",
            "return select(2, pcall(function() table.insert({}, 'x', 1) end))",
            "local t = {1, 2, 3} local r = table.remove(t) return r .. ':' .. table.concat(t, ',')",
            "local t = {1, 2, 3} local r = table.remove(t, 1) return r .. ':' .. table.concat(t, ',')",
            "local t = {} return tostring(table.remove(t)) .. tostring(table.remove(t, 1)) .. #t",
            "local t = {1, 2, 3} return tostring(table.remove(t, 4)) .. table.concat(t, ',')",
            "return select(2, pcall(function() table.remove({1, 2, 3}, 5) end))",
            "return table.concat(table.move({1, 2, 3, 4, 5}, 2, 4, 1), ',')",
            "return table.concat(table.move({1, 2, 3, 4, 5}, 1, 3, 3), ',')",
            "local t = table.move({1, 2, 3}, 1, 3, 2, {}) return tostring(t[1]) .. table.concat(t, ',', 2)",
            "return select(2, pcall(function() table.move({}, 1, math.maxinteger, 2) end))",
            "return select(2, pcall(function() table.move({}, -1, math.maxinteger, 1) end))",
            "return select(2, pcall(function() table.move({}, 1, 2) end))",
            "return select(2, pcall(function() table.move(nil, 1, 2, 3) end))",
            "return select(2, pcall(function() table.move({}, 1, 2, 3, 'x') end))",
            "local s = {} local t = setmetatable({}, {__newindex = function(_, k, v) s[#s + 1] = k .. '=' .. tostring(v) end, __index = function(_, k) return k * 10 end}) table.move(t, 1, 3, 2) return table.concat(s, ' ')",
            "return table.concat({1, 2, 'a'}, '-')",
            "return table.concat({1, 2, 3}, ', ', 2)",
            "return table.concat({1, 2, 3}, 1, 2, 3)",
            "return table.concat({}, 'x') .. '|'",
            "return select(2, pcall(function() table.concat({1, {}, 3}) end))",
            "return select(2, pcall(function() table.concat({}, {}) end))",
            "local t = setmetatable({}, {__index = function(_, i) return 'v' .. i end, __len = function() return 3 end}) return table.concat(t, ',')",
            "local t = setmetatable({}, {__index = function(_, i) return i == 2 or 'v' end, __len = function() return 3 end}) return select(2, pcall(function() table.concat(t) end))",
            "local t = setmetatable({}, {__len = function() return 1.5 end}) return select(2, pcall(function() table.concat(t) end))",
        ]);
    }

    #[test]
    fn sort_orders_as_the_c_sort_does_ties_included() {
        // Records with many ties, so a different algorithm would leave
        // the tied ones in another order. Under 100 elements the C sort
        // never picks a random pivot.
        let records = "local seed, t = 7, {} \
                       for i = 1, 90 do seed = (seed * 75 + 74) % 65537 t[i] = {id = i, key = seed % 5} end";
        let read = "local ids = {} for i, r in ipairs(t) do ids[i] = r.id end return table.concat(ids, ',')";
        same_as_stock(&[
            "local t = {5, 2, 8, 1, 9, 3} table.sort(t) return table.concat(t, ',')",
            "local t = {5, 2, 8, 1, 9, 3} table.sort(t, function(a, b) return a > b end) return table.concat(t, ',')",
            &format!(
                "{records} setmetatable(t, {{}}) table.sort(t, function(a, b) return a.key < b.key end) {read}"
            ),
            &format!(
                "{records} table.sort(t, function(a, b) return a.key < b.key end) {read}"
            ),
            "return select(2, pcall(function() table.sort({3, 1, 2}, 1) end))",
            "return select(2, pcall(function() table.sort(setmetatable({3, 1, 2, 5, 4}, {}), function() return true end) end))",
            "return select(2, pcall(function() table.sort({1, 'x', 2}) end))",
        ]);
        // A large table sorts in Lua, to the same order.
        let mut e = crate::ScriptEngine::new().unwrap();
        let sorted = e.eval(
            "local seed, t = 3, setmetatable({}, {}) \
                 for i = 1, 3000 do seed = (seed * 75 + 74) % 65537 t[i] = seed end \
                 table.sort(t) local ok = true \
                 for i = 2, #t do ok = ok and t[i - 1] <= t[i] end \
                 mud.echo(tostring(ok) .. ' ' .. #t)",
            "=t",
        );
        assert_eq!(sorted.actions, [crate::Action::Echo("true 3000".into())]);
    }

    #[test]
    fn a_huge_range_or_length_stops_on_the_time_limit() {
        for code in [
            "table.move({}, 1, math.maxinteger, 1, {})",
            "local t = setmetatable({}, {__len = function() return math.maxinteger - 1 end}) \
             table.insert(t, 1, 'x')",
            "local t = setmetatable({}, {__len = function() return math.maxinteger end}) \
             table.remove(t, 1)",
            // Metamethods written in C use no memory and run no Lua.
            "local t = setmetatable({}, {__len = function() return 2147483646 end, \
               __index = rawlen, __newindex = rawequal}) table.sort(t)",
            "local t = setmetatable({}, {__len = function() return math.maxinteger end, \
               __index = rawlen}) table.concat(t)",
        ] {
            let outcome = typed_in_time(code);
            assert_eq!(outcome.stopped, [Owner::Typed], "{code}");
        }
    }
}
