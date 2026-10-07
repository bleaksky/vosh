//! What the app's tests use to drive the engine.

use std::time::Duration;

use crate::ScriptEngine;

/// The time the naps of the call running now stand for.
struct Napped(Duration);

/// A new engine whose `os.nap(ms)` counts as `ms` of the call that runs
/// it, with no sleep. A test of the budget then charges each slow handler
/// the same time on every machine, where a real sleep can run long on a
/// busy runner and leave a handler past the budget alone. `os.clock`
/// counts the processor time of every thread, so a loop on it ends early
/// while other tests run.
pub fn engine_with_nap() -> ScriptEngine {
    let engine = ScriptEngine::default();
    engine.lua.set_app_data(Napped(Duration::ZERO));
    let nap = engine
        .lua
        .create_function(|lua, ms: u64| {
            if let Some(mut napped) = lua.app_data_mut::<Napped>() {
                napped.0 += Duration::from_millis(ms);
            }
            Ok(())
        })
        .expect("Lua makes a function");
    let os: mlua::Table = engine.lua.globals().get("os").expect("the os table");
    os.set("nap", nap).expect("os takes a function");
    engine
}

/// The time the naps of the call that just ended stand for, which the
/// next call starts from nothing.
pub(crate) fn take_napped(lua: &mlua::Lua) -> Duration {
    lua.app_data_mut::<Napped>()
        .map(|mut napped| std::mem::take(&mut napped.0))
        .unwrap_or_default()
}
