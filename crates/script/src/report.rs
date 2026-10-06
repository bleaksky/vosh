//! The lines Vosh prints about the Lua it runs: an error with its file
//! and line where Lua knows them, a stop, the action cap, and the
//! handlers an event skipped.

use crate::actions::{Action, Place};
use crate::budget::{Event, EVENT_BUDGET};
use crate::limits::{
    At, Stop, StopReason, ACTIONS_PER_CALL, CALL_MEMORY, ECHO_BYTES, MB, PANE_BLOCKS, STATE_MEMORY,
    TIME_BUDGET,
};
use crate::owner::{Owner, Site};

/// The line for a Lua error of `owner`, with the place Lua names.
pub(crate) fn error(owner: &Owner, err: &mlua::Error) -> Action {
    let (at, message) = parts(err);
    let at = at
        .as_deref()
        .map_or_else(|| leading_place(&message), leading_place);
    Action::Error {
        owner: owner.clone(),
        text: describe(err),
        at,
    }
}

/// The place `source:line` at the start of `text`, the way Lua puts it
/// before an error's message, like `vitals_alert/main.lua:22`.
fn leading_place(text: &str) -> Option<Place> {
    text.match_indices(':').find_map(|(colon, _)| {
        let after = &text[colon + 1..];
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        let rest = &after[digits..];
        if digits == 0 || !(rest.is_empty() || rest.starts_with(':')) {
            return None;
        }
        Some(Place {
            source: text[..colon].to_string(),
            line: after[..digits].parse().ok()?,
        })
    })
}

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

/// The lines for a stop of `owner`, who ran `site` when Vosh stopped it:
/// the stop as an error, and for a plugin a note on how long it stays
/// off, which the board draws plain.
pub(crate) fn stop_lines(owner: &Owner, site: &Site, stop: &Stop) -> Vec<Action> {
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
    let error = |text: String| Action::Error {
        owner: owner.clone(),
        text,
        at: stop.at.as_ref().map(place),
    };
    match (owner, site) {
        (Owner::Plugin(name), _) => vec![
            error(what),
            Action::Note {
                owner: owner.clone(),
                text: format!(
                    "{name} stays off until you save it under Scripts in Settings or restart Vosh."
                ),
            },
        ],
        (Owner::Trigger(name) | Owner::Alias(name), _) => vec![error(format!(
            "{what} {name} stays off until you save it or restart Vosh."
        ))],
        (Owner::Script(_), _) => vec![error(format!("{what} It stays off until #script reload."))],
        (Owner::Typed, Site::LuaTrigger { .. } | Site::Gmcp { .. }) => {
            vec![error(format!("{what} Vosh removed it."))]
        }
        (Owner::Typed, Site::Entry | Site::Timer { .. }) => vec![error(what)],
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

/// The line for a call that gave a pane more blocks than one pane
/// shows.
pub(crate) fn pane_cap_line(owner: &Owner, site: &Site) -> String {
    let subject = subject(owner, site);
    let subject = match owner {
        Owner::Plugin(_) | Owner::Script(_) => subject,
        _ => capitalized(&subject),
    };
    format!(
        "{subject} gave a pane more than {PANE_BLOCKS} blocks. Vosh shows the first {PANE_BLOCKS}."
    )
}

/// The line for the first handler of `owner` that `event` skipped, once
/// `owner` used its time for the event.
pub(crate) fn budget_line(owner: &Owner, event: &Event) -> String {
    let subject = subject(owner, &Site::Entry);
    let used = format!("{subject} used its {} ms", EVENT_BUDGET.as_millis());
    match event {
        Event::Line => format!("{used} for this line, so Vosh skipped the rest of its handlers."),
        Event::Packet(package) => {
            format!("{used} for this {package} packet, so Vosh skipped the rest of its handlers.")
        }
        Event::Timers => format!(
            "{used} for this round of timers, so the rest of its timers wait for the next round."
        ),
        Event::Replay => format!(
            "{used} on the last packets, so the rest of its new handlers wait for the next packet."
        ),
    }
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
    let path = shown_source(source);
    path.strip_prefix(name)
        .and_then(|rest| rest.strip_prefix('/'))
        .unwrap_or(path)
        .to_string()
}

/// The place a stop happened, as Lua names it in a message.
fn place(at: &At) -> Place {
    Place {
        source: shown_source(&at.source).to_string(),
        line: at.line,
    }
}

/// A chunk name as Lua shows it in a message, without the `@` or `=`
/// it starts with.
fn shown_source(source: &str) -> &str {
    source
        .strip_prefix('@')
        .or_else(|| source.strip_prefix('='))
        .unwrap_or(source)
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

    /// The text of each line in `actions`.
    fn texts(actions: &[Action]) -> Vec<&str> {
        actions
            .iter()
            .map(|action| match action {
                Action::Error { text, .. } | Action::Note { text, .. } => text.as_str(),
                other => panic!("no line: {other:?}"),
            })
            .collect()
    }

    #[test]
    fn a_plugin_stop_reads_as_the_board_writes_it() {
        let owner = Owner::Plugin("wait_full".into());
        let at = Some(("@wait_full/main.lua", 5));
        // The stop is an error at its place, and the second line a note
        // the board draws plain.
        assert_eq!(
            stop_lines(&owner, &Site::Entry, &stop(StopReason::Time, at)),
            [
                Action::Error {
                    owner: owner.clone(),
                    text: "Vosh stopped wait_full at main.lua line 5 after 100 ms.".into(),
                    at: Some(Place {
                        source: "wait_full/main.lua".into(),
                        line: 5,
                    }),
                },
                Action::Note {
                    owner: owner.clone(),
                    text: "wait_full stays off until you save it under Scripts in Settings or restart Vosh."
                        .into(),
                },
            ]
        );
        assert_eq!(
            texts(&stop_lines(
                &owner,
                &Site::Entry,
                &stop(StopReason::CallMemory, at)
            ))[0],
            "Vosh stopped wait_full at main.lua line 5. One call used more than 32 MB."
        );
        assert_eq!(
            texts(&stop_lines(
                &owner,
                &Site::Entry,
                &stop(StopReason::StateMemory, at)
            ))[0],
            "Vosh stopped wait_full. Your scripts hold more than 128 MB."
        );
        assert_eq!(
            texts(&stop_lines(
                &owner,
                &Site::Entry,
                &stop(StopReason::Time, None)
            )),
            [
                "Vosh stopped wait_full after 100 ms.",
                "wait_full stays off until you save it under Scripts in Settings or restart Vosh.",
            ]
        );
        assert_eq!(
            cap_line(&owner, &Site::Entry),
            "wait_full queued more than 100 actions in one call. Vosh dropped the rest."
        );
    }

    #[test]
    fn the_other_stops_read_as_the_board_writes_them() {
        let time = stop(StopReason::Time, Some(("=trigger tells", 1)));
        let tells = Owner::Trigger("tells".into());
        // Each is one error, at the place the stop names.
        assert_eq!(
            stop_lines(&tells, &Site::Entry, &time),
            [Action::Error {
                owner: tells.clone(),
                text: "Vosh stopped the Lua in trigger tells after 100 ms. tells stays off until you save it or restart Vosh.".into(),
                at: Some(Place {
                    source: "trigger tells".into(),
                    line: 1,
                }),
            }]
        );
        assert_eq!(
            texts(&stop_lines(&Owner::Alias("heal".into()), &Site::Entry, &time)),
            ["Vosh stopped the Lua in alias heal after 100 ms. heal stays off until you save it or restart Vosh."]
        );
        assert_eq!(
            texts(&stop_lines(
                &Owner::Script("combat.lua".into()),
                &Site::Entry,
                &time
            )),
            ["Vosh stopped combat.lua after 100 ms. It stays off until #script reload."]
        );
        assert_eq!(
            texts(&stop_lines(&Owner::Typed, &Site::Entry, &time)),
            ["Vosh stopped your #lua line after 100 ms."]
        );
        let handler = Site::Gmcp {
            package: "Char.Vitals".into(),
            callback_id: 1,
        };
        assert_eq!(
            texts(&stop_lines(&Owner::Typed, &handler, &time)),
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
    fn a_budget_line_names_the_owner_and_the_event() {
        let owner = Owner::Plugin("vitals_alert".into());
        assert_eq!(
            budget_line(&owner, &Event::Line),
            "vitals_alert used its 100 ms for this line, so Vosh skipped the rest of its handlers."
        );
        assert_eq!(
            budget_line(&owner, &Event::Packet("Char.Vitals".into())),
            "vitals_alert used its 100 ms for this Char.Vitals packet, so Vosh skipped the rest of its handlers."
        );
        assert_eq!(
            budget_line(&Owner::Script("combat.lua".into()), &Event::Timers),
            "combat.lua used its 100 ms for this round of timers, so the rest of its timers wait for the next round."
        );
        assert_eq!(
            budget_line(&owner, &Event::Replay),
            "vitals_alert used its 100 ms on the last packets, so the rest of its new handlers wait for the next packet."
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

    #[test]
    fn an_error_carries_the_place_lua_names() {
        let owner = Owner::Plugin("vitals_alert".into());
        let place = |source: &str, line| {
            Some(Place {
                source: source.into(),
                line,
            })
        };
        let at = |err: &mlua::Error| match error(&owner, err) {
            Action::Error { at, .. } => at,
            other => panic!("no error: {other:?}"),
        };
        let runtime = mlua::Error::RuntimeError(
            "vitals_alert/main.lua:22: attempt to concatenate a nil value (field 'hp_pct')".into(),
        );
        assert_eq!(at(&runtime), place("vitals_alert/main.lua", 22));
        let callback = mlua::Error::CallbackError {
            traceback: "stack traceback:\n\t[C]: in function 'mud.send'\n\t#lua:2: in main chunk"
                .into(),
            cause: std::sync::Arc::new(mlua::Error::RuntimeError("bad argument #1".into())),
        };
        assert_eq!(at(&callback), place("#lua", 2));
        // A colon in the chunk name or the message is no place.
        let quoted = mlua::Error::RuntimeError("[string \"a:b\"]:3: near 'x': 12:30".into());
        assert_eq!(at(&quoted), place("[string \"a:b\"]", 3));
        let bare = mlua::Error::RuntimeError("the clock says 12:30 now".into());
        assert_eq!(at(&bare), None);
    }
}
