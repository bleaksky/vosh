//! The lines Vosh prints about the Lua it runs: an error with its file
//! and line where Lua knows them, a stop, and the action cap.

use crate::limits::{
    Stop, StopReason, ACTIONS_PER_CALL, CALL_MEMORY, ECHO_BYTES, MB, STATE_MEMORY, TIME_BUDGET,
};
use crate::owner::{Owner, Site};

/// The line for a Lua error: the place Lua names, then what went
/// wrong, without the stack traceback mlua adds. A message longer than
/// an echo may be ends where an echo would.
pub(crate) fn describe(err: &mlua::Error) -> String {
    let mut line = match parts(err) {
        (Some(at), message) => format!("{at}: {message}"),
        (None, message) => message,
    };
    if line.len() > ECHO_BYTES {
        let mut end = ECHO_BYTES;
        while !line.is_char_boundary(end) {
            end -= 1;
        }
        line.truncate(end);
    }
    line
}

/// Where an error happened, when Lua's message does not say so itself,
/// and what went wrong.
fn parts(err: &mlua::Error) -> (Option<String>, String) {
    match err {
        mlua::Error::RuntimeError(message) | mlua::Error::SyntaxError { message, .. } => {
            (None, without_traceback(message))
        }
        mlua::Error::CallbackError { traceback, cause } => {
            let (at, message) = parts(cause);
            (at.or_else(|| first_frame(traceback)), message)
        }
        mlua::Error::WithContext { context, cause } => {
            let (at, message) = parts(cause);
            (at, format!("{context}: {message}"))
        }
        other => (None, other.to_string()),
    }
}

fn without_traceback(message: &str) -> String {
    message
        .split("\nstack traceback:")
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_string()
}

/// The first place in your Lua a traceback names, like `#lua:1`, past
/// the C functions and Vosh's own Lua.
fn first_frame(traceback: &str) -> Option<String> {
    traceback
        .lines()
        .filter_map(|line| line.trim().split_once(": in "))
        .map(|(at, _)| at)
        .find(|at| *at != "[C]" && !at.starts_with("[vosh]") && *at != "?")
        .map(str::to_string)
}

/// The lines for a stop of `owner`, who ran `site` when Vosh stopped it.
pub(crate) fn stop_lines(owner: &Owner, site: &Site, stop: &Stop) -> Vec<String> {
    let subject = subject(owner, site);
    // Only the stop of a plugin names the file and line, the way the
    // Scripts design writes its lines.
    let at = match (owner, &stop.at) {
        (Owner::Plugin(name), Some(at)) => {
            format!(" at {} line {}", file_in(name, &at.source), at.line)
        }
        _ => String::new(),
    };
    let what = match stop.reason {
        StopReason::Time => format!(
            "Vosh stopped {subject}{at} after {} ms.",
            TIME_BUDGET.as_millis()
        ),
        StopReason::CallMemory => format!(
            "Vosh stopped {subject}{at}. One call used more than {} MB.",
            CALL_MEMORY / MB
        ),
        StopReason::StateMemory => format!(
            "Vosh stopped {subject}. Your scripts hold more than {} MB.",
            STATE_MEMORY / MB
        ),
    };
    match (owner, site) {
        (Owner::Plugin(name), _) => vec![
            what,
            format!(
                "{name} stays off until you save it under Scripts in Settings or restart Vosh."
            ),
        ],
        (Owner::Trigger(name) | Owner::Alias(name), _) => {
            vec![format!(
                "{what} {name} stays off until you save it or restart Vosh."
            )]
        }
        (Owner::Script(_), _) => vec![format!("{what} It stays off until #script reload.")],
        (Owner::Typed, Site::LuaTrigger { .. } | Site::Gmcp { .. }) => {
            vec![format!("{what} Vosh removed it.")]
        }
        (Owner::Typed, Site::Entry | Site::Timer { .. }) => vec![what],
    }
}

/// The line for a call that queued more actions than one call may.
pub(crate) fn cap_line(owner: &Owner, site: &Site) -> String {
    let subject = subject(owner, site);
    // A phrase starts the sentence with a capital, and a name stays as
    // you wrote it.
    let subject = match owner {
        Owner::Plugin(_) | Owner::Script(_) => subject,
        _ => capitalized(&subject),
    };
    format!(
        "{subject} queued more than {ACTIONS_PER_CALL} actions in one call. Vosh dropped the rest."
    )
}

/// The line for a call that queued a piece of text past its size limit,
/// or more text in all than one call may.
pub(crate) fn text_cap_line(owner: &Owner, site: &Site) -> String {
    let subject = subject(owner, site);
    let subject = match owner {
        Owner::Plugin(_) | Owner::Script(_) => subject,
        _ => capitalized(&subject),
    };
    format!("{subject} queued more text than one call may. Vosh dropped what went past the limit.")
}

/// How the lines name the Lua that ran.
fn subject(owner: &Owner, site: &Site) -> String {
    match owner {
        Owner::Plugin(name) | Owner::Script(name) => name.clone(),
        Owner::Trigger(name) => format!("the Lua in trigger {name}"),
        Owner::Alias(name) => format!("the Lua in alias {name}"),
        Owner::Typed => match site {
            Site::Entry => "your #lua line".to_string(),
            Site::LuaTrigger { name, .. } => format!("the Lua trigger {name}"),
            Site::Gmcp { package, .. } => format!("a {package} handler"),
            Site::Timer { .. } => "a timer".to_string(),
        },
    }
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The file of plugin `name` a chunk name points at, like `main.lua`
/// for `@vitals_alert/main.lua`.
fn file_in(name: &str, source: &str) -> String {
    let path = source
        .strip_prefix('@')
        .or_else(|| source.strip_prefix('='))
        .unwrap_or(source);
    path.strip_prefix(name)
        .and_then(|rest| rest.strip_prefix('/'))
        .unwrap_or(path)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::At;

    fn stop(reason: StopReason, at: Option<(&str, u32)>) -> Stop {
        Stop {
            reason,
            at: at.map(|(source, line)| At {
                source: source.into(),
                line,
            }),
        }
    }

    #[test]
    fn a_plugin_stop_reads_as_the_board_writes_it() {
        let owner = Owner::Plugin("wait_full".into());
        let at = Some(("@wait_full/main.lua", 5));
        assert_eq!(
            stop_lines(&owner, &Site::Entry, &stop(StopReason::Time, at)),
            [
                "Vosh stopped wait_full at main.lua line 5 after 100 ms.",
                "wait_full stays off until you save it under Scripts in Settings or restart Vosh.",
            ]
        );
        assert_eq!(
            stop_lines(&owner, &Site::Entry, &stop(StopReason::CallMemory, at))[0],
            "Vosh stopped wait_full at main.lua line 5. One call used more than 32 MB."
        );
        assert_eq!(
            stop_lines(&owner, &Site::Entry, &stop(StopReason::StateMemory, at))[0],
            "Vosh stopped wait_full. Your scripts hold more than 128 MB."
        );
        assert_eq!(
            stop_lines(&owner, &Site::Entry, &stop(StopReason::Time, None))[0],
            "Vosh stopped wait_full after 100 ms."
        );
        assert_eq!(
            cap_line(&owner, &Site::Entry),
            "wait_full queued more than 100 actions in one call. Vosh dropped the rest."
        );
    }

    #[test]
    fn the_other_stops_read_as_the_board_writes_them() {
        let time = stop(StopReason::Time, Some(("=trigger tells", 1)));
        assert_eq!(
            stop_lines(&Owner::Trigger("tells".into()), &Site::Entry, &time),
            ["Vosh stopped the Lua in trigger tells after 100 ms. tells stays off until you save it or restart Vosh."]
        );
        assert_eq!(
            stop_lines(&Owner::Alias("heal".into()), &Site::Entry, &time),
            ["Vosh stopped the Lua in alias heal after 100 ms. heal stays off until you save it or restart Vosh."]
        );
        assert_eq!(
            stop_lines(&Owner::Script("combat.lua".into()), &Site::Entry, &time),
            ["Vosh stopped combat.lua after 100 ms. It stays off until #script reload."]
        );
        assert_eq!(
            stop_lines(&Owner::Typed, &Site::Entry, &time),
            ["Vosh stopped your #lua line after 100 ms."]
        );
        let handler = Site::Gmcp {
            package: "Char.Vitals".into(),
            callback_id: 1,
        };
        assert_eq!(
            stop_lines(&Owner::Typed, &handler, &time),
            ["Vosh stopped a Char.Vitals handler after 100 ms. Vosh removed it."]
        );
        assert_eq!(
            cap_line(&Owner::Trigger("tells".into()), &Site::Entry),
            "The Lua in trigger tells queued more than 100 actions in one call. Vosh dropped the rest."
        );
        assert_eq!(
            cap_line(&Owner::Typed, &Site::Entry),
            "Your #lua line queued more than 100 actions in one call. Vosh dropped the rest."
        );
    }

    #[test]
    fn an_error_names_its_place_without_the_traceback() {
        let runtime = mlua::Error::RuntimeError(
            "vitals_alert/main.lua:22: attempt to concatenate a nil value (field 'hp_pct')\nstack traceback:\n\t[C]: in ?"
                .into(),
        );
        assert_eq!(
            describe(&runtime),
            "vitals_alert/main.lua:22: attempt to concatenate a nil value (field 'hp_pct')"
        );
        let callback = mlua::Error::CallbackError {
            traceback: "stack traceback:\n\t[C]: in function 'mud.send'\n\t[vosh]:7: in function 'pcall'\n\t#lua:1: in main chunk\n\t[C]: in ?".into(),
            cause: std::sync::Arc::new(mlua::Error::RuntimeError("script engine state missing".into())),
        };
        assert_eq!(describe(&callback), "#lua:1: script engine state missing");
    }
}
