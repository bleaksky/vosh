//! The environment each plugin runs in. A plugin's globals are its own,
//! and so is its `mud` table, whose functions register for the plugin.
//! Two plugins that both define `draw` never overwrite each other, and
//! no plugin can replace `mud.send` for the rest. The standard libraries
//! reach a plugin read only.
//!
//! Your `#lua` lines, the bodies of your triggers and aliases, and loose
//! scripts share the global environment, as they always have. They
//! reach a plugin's globals through `plugins.<name>`, a read only view
//! that follows the plugin across a reload.

use mlua::{Function, Lua, Table, Value};

use crate::api;
use crate::limits::INTERNAL_CHUNK;
use crate::owner::Owner;

/// Lua that makes a plugin's environment. It takes the standard
/// library as it stood once the sandbox and the limits were in place,
/// and returns the function that makes one environment from a plugin's
/// `mud` table.
///
/// Each environment gets its own read only stand in for each library
/// table, so a `rawset` on one reaches that plugin alone. The string
/// methods read the string library through the metatable every string
/// shares, so a plugin's own `getmetatable` hides that metatable, and
/// your own Lua still reads it as it always has.
const ENV: &str = r#"
local std = ...
local error, next, setmetatable, type = error, next, setmetatable, type
local raw_getmetatable = getmetatable

local function plugin_getmetatable(value)
  if type(value) == "string" then
    return false
  end
  return raw_getmetatable(value)
end

local function read_only(lib, label)
  local proxy = {}
  return setmetatable(proxy, {
    __index = lib,
    __newindex = function()
      error(label .. " is read only in a plugin", 2)
    end,
    __pairs = function()
      return function(_, key) return next(lib, key) end, proxy, nil
    end,
    __len = function() return #lib end,
    __metatable = false,
  })
end

return function(mud)
  local view = {}
  for key, value in next, std do
    if type(value) == "table" then
      view[key] = read_only(value, key)
    else
      view[key] = value
    end
  end
  view.getmetatable = plugin_getmetatable
  local env = { mud = mud }
  env._G = env
  return setmetatable(env, { __index = view, __metatable = false })
end
"#;

/// Lua that puts `plugins` in the global environment. It takes the
/// table of environments by plugin name. `plugins.<name>` is a view of
/// that plugin's globals that reads the environment it has now, or
/// nothing while it is off, and refuses a change. It leaves out `_G`,
/// which is the environment itself, and the plugin's `mud` table, so
/// the view hands out nothing that writes into the plugin or registers
/// for it.
const PLUGINS: &str = r#"
local by_name = ...
local error, next, setmetatable = error, next, setmetatable
local views = {}
local hidden = { _G = true, mud = true }

local function view_of(name)
  local view = views[name]
  if view ~= nil then
    return view
  end
  local label = "plugins." .. name
  view = {}
  setmetatable(view, {
    __index = function(_, key)
      local env = by_name[name]
      if env ~= nil and not hidden[key] then
        return env[key]
      end
    end,
    __newindex = function()
      error(label .. " is read only", 2)
    end,
    __pairs = function()
      local env = by_name[name] or {}
      local function step(_, key)
        local after, value = next(env, key)
        while after ~= nil and hidden[after] do
          after, value = next(env, after)
        end
        return after, value
      end
      return step, view, nil
    end,
    __metatable = false,
  })
  views[name] = view
  return view
end

local plugins = {}
plugins = setmetatable(plugins, {
  __index = function(_, name)
    if by_name[name] ~= nil then
      return view_of(name)
    end
  end,
  __newindex = function()
    error("plugins is read only", 2)
  end,
  __pairs = function()
    return function(_, name)
      local after = next(by_name, name)
      if after ~= nil then
        return after, view_of(after)
      end
    end, plugins, nil
  end,
  __metatable = false,
})
return plugins
"#;

/// The environments of the plugins that run.
pub(crate) struct Envs {
    /// Each running plugin's environment, by plugin name.
    by_name: Table,
    /// Makes one environment from a plugin's `mud` table.
    make: Function,
}

impl Envs {
    /// Take the standard library as it stands now, after the sandbox and
    /// the limits and before the `mud` table, for every plugin to read,
    /// then put `plugins` in the global environment.
    pub(crate) fn install(lua: &Lua) -> mlua::Result<Self> {
        let std = lua.create_table()?;
        for pair in lua.globals().pairs::<Value, Value>() {
            let (key, value) = pair?;
            if !matches!(&key, Value::String(name) if name.as_bytes() == b"_G") {
                std.raw_set(key, value)?;
            }
        }
        let make = lua.load(ENV).set_name(INTERNAL_CHUNK).call(std)?;
        let by_name = lua.create_table()?;
        let plugins: Table = lua
            .load(PLUGINS)
            .set_name(INTERNAL_CHUNK)
            .call(by_name.clone())?;
        lua.globals().set("plugins", plugins)?;
        Ok(Self { by_name, make })
    }

    /// A new environment for the plugin `name`, with a `mud` table of
    /// its own.
    pub(crate) fn create(&self, lua: &Lua, name: &str) -> mlua::Result<Table> {
        let mud = api::mud_table(lua, Some(Owner::Plugin(name.to_string())))?;
        self.make.call(mud)
    }

    /// The environment of the plugin `name`, while it runs.
    pub(crate) fn get(&self, name: &str) -> Option<Table> {
        self.by_name.raw_get::<Option<Table>>(name).ok().flatten()
    }

    /// Make `env` the environment of the running plugin `name`, or with
    /// None forget the one it had.
    pub(crate) fn set(&self, name: &str, env: Option<Table>) {
        let _ = self.by_name.raw_set(name, env);
    }
}
