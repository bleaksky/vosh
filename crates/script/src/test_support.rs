//! Helpers the tests of several modules share.

use std::time::Duration;

use crate::{Action, Owner, ScriptEngine, ScriptOutcome};

/// Run `f` on a thread of its own and hand back what it returns,
/// failing the test when it has not returned within ten seconds, so Lua
/// that never stops fails a test instead of hanging the suite.
pub(crate) fn returns_in_time<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(Duration::from_secs(10))
        .expect("the Lua never came back")
}

/// Run `code` as a `#lua` line in a fresh engine, on a thread of its
/// own, and hand back what it left.
pub(crate) fn typed_in_time(code: &str) -> ScriptOutcome {
    let code = code.to_string();
    returns_in_time(move || ScriptEngine::new().unwrap().eval(&code, "=t"))
}

/// What `body`, the body of a Lua function that returns one value, gives
/// back as a string in stock Lua and then in Vosh, each from a chunk
/// named `t`, so an error names the same place in both.
pub(crate) fn stock_and_vosh(body: &str) -> (String, String) {
    let stock = mlua::Lua::new()
        .load(format!("return tostring((function() {body} end)())"))
        .set_name("=t")
        .eval::<String>()
        .unwrap_or_else(|e| panic!("stock Lua failed on {body}: {e}"));
    let outcome = ScriptEngine::new().unwrap().eval(
        &format!("mud.log(tostring((function() {body} end)()))"),
        "=t",
    );
    let vosh = match outcome.actions.as_slice() {
        [Action::Log { text, .. }] => text.clone(),
        other => panic!("Vosh failed on {body}: {other:?}"),
    };
    (stock, vosh)
}

/// Assert that each body in `bodies` gives back in Vosh what it gives
/// back in stock Lua.
#[track_caller]
pub(crate) fn same_as_stock(bodies: &[&str]) {
    for body in bodies {
        let (stock, vosh) = stock_and_vosh(body);
        assert_eq!(vosh, stock, "{body}");
    }
}

/// The error lines Vosh printed about the Lua, errors and stops alike.
pub(crate) fn error_lines(outcome: &ScriptOutcome) -> Vec<String> {
    outcome
        .actions
        .iter()
        .filter_map(|action| match action {
            Action::Error { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// Each line Vosh printed about the Lua, an error, a note or a print,
/// with whose Lua it is about, in order.
pub(crate) fn said(outcome: &ScriptOutcome) -> Vec<(&'static str, Owner, String)> {
    outcome
        .actions
        .iter()
        .filter_map(|action| match action {
            Action::Error { owner, text, .. } => Some(("error", owner.clone(), text.clone())),
            Action::Note { owner, text } => Some(("note", owner.clone(), text.clone())),
            Action::Log { owner, text } => Some(("print", owner.clone(), text.clone())),
            _ => None,
        })
        .collect()
}
