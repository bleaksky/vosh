//! The environment each plugin runs in. A plugin's globals are its own,
//! and so is its `mud` table, whose functions register for the plugin.
//! Two plugins that both define `draw` never overwrite each other, and
//! no plugin can replace `mud.send` for the rest. The standard libraries
//! reach a plugin read only.
//!
//! Your `#lua` lines, the bodies of your triggers and aliases, and loose
//! scripts share the global environment, as they always have.

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
/// shares, so that metatable hides itself from `getmetatable`.
const ENV: &str = r#"
local std = ...
local error, next, setmetatable, type = error, next, setmetatable, type

getmetatable("").__metatable = false

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
  local env = { mud = mud }
  env._G = env
  return setmetatable(env, { __index = view, __metatable = false })
end
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
    /// the limits and before the `mud` table, for every plugin to read.
    pub(crate) fn install(lua: &Lua) -> mlua::Result<Self> {
        let std = lua.create_table()?;
        for pair in lua.globals().pairs::<Value, Value>() {
            let (key, value) = pair?;
            if !matches!(&key, Value::String(name) if name.as_bytes() == b"_G") {
                std.raw_set(key, value)?;
            }
        }
        let make = lua.load(ENV).set_name(INTERNAL_CHUNK).call(std)?;
        Ok(Self {
            by_name: lua.create_table()?,
            make,
        })
    }

    /// A new environment for the plugin `name`, with a `mud` table of
    /// its own.
    pub(crate) fn create(&self, lua: &Lua, name: &str) -> mlua::Result<Table> {
        let mud = api::mud_table(lua, Some(Owner::Plugin(name.to_string())))?;
        self.make.call(mud)
    }

    /// Make `env` the environment of the running plugin `name`, or with
    /// None forget the one it had.
    pub(crate) fn set(&self, name: &str, env: Option<Table>) {
        let _ = self.by_name.raw_set(name, env);
    }
}
