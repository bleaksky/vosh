//! What the app's tests use to drive the engine.

use std::time::Duration;

use crate::ScriptEngine;

/// A new engine whose `os.nap(ms)` sleeps for `ms`, so a slow handler
/// takes that long on the wall clock however busy other tests keep the
/// processor. `os.clock` counts the processor time of every thread, so a
/// loop on it ends early while other tests run.
pub fn engine_with_nap() -> ScriptEngine {
    let engine = ScriptEngine::default();
    let nap = engine
        .lua
        .create_function(|_, ms: u64| {
            std::thread::sleep(Duration::from_millis(ms));
            Ok(())
        })
        .expect("Lua makes a function");
    let os: mlua::Table = engine.lua.globals().get("os").expect("the os table");
    os.set("nap", nap).expect("os takes a function");
    engine
}
