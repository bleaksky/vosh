//! The synthetic pulses in `fixtures/prompt/aabahran/wire`.
//!
//! Each case is what the fake game writes for a few steps of play, as
//! the bytes of one socket read. [`play`] plays a case again, so a test
//! holds each `.bin` file to the fake that wrote it, and the session tests
//! read each one through the session at every split.

use super::mud::{self, telnet, Build, Mud, Options};

/// One fixture: its file name without `.bin`, the build that sends it,
/// and the PROMPT in force when its prompt prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Case {
    pub name: &'static str,
    pub build: Build,
    pub prompt: &'static str,
}

/// Every case.
pub const CASES: [Case; 10] = [
    Case {
        name: "quiet",
        build: Build::New,
        prompt: mud::PROMPT,
    },
    Case {
        name: "fight-tank",
        build: Build::New,
        prompt: mud::PROMPT,
    },
    Case {
        name: "lament-new",
        build: Build::New,
        prompt: mud::PROMPT,
    },
    Case {
        name: "lament-243cac5c",
        build: Build::Unflagged,
        prompt: mud::PROMPT,
    },
    Case {
        name: "lament-older",
        build: Build::Older,
        prompt: mud::PROMPT,
    },
    Case {
        name: "prompt-all-next",
        build: Build::Older,
        prompt: mud::PROMPT_ALL,
    },
    Case {
        name: "ga",
        build: Build::New,
        prompt: mud::PROMPT_ALL,
    },
    Case {
        name: "login-new",
        build: Build::New,
        prompt: mud::PROMPT,
    },
    Case {
        name: "prompt-x-new",
        build: Build::New,
        prompt: PROMPT_X,
    },
    Case {
        name: "prompts-off-new",
        build: Build::New,
        prompt: mud::PROMPT,
    },
];

/// The setting `prompt-x-new` types, and the PROMPT the game stores for
/// it.
pub const TYPED_X: &str = "<%h/%Hhp %m/%Mmn>";
pub const PROMPT_X: &str = "<%h/%Hhp %m/%Mmn> ";

/// What someone arriving prints, the output with no command before it.
pub const ARRIVES: &str = "A Blackwatch guard arrives from the south.";

/// The case named `name`.
pub fn case(name: &str) -> Option<Case> {
    CASES.iter().copied().find(|c| c.name == name)
}

/// Play the case named `name` again. None for a name no case has.
pub fn play(name: &str) -> Option<Vec<u8>> {
    let case = case(name)?;
    let options = Options {
        prompt: if case.prompt == PROMPT_X {
            mud::PROMPT.into()
        } else {
            case.prompt.into()
        },
        ga: name != "prompt-all-next",
        ..Options::new(case.build)
    };
    if name == "login-new" {
        let mut mud = Mud::new(options);
        return Some(
            mud.receive(&[telnet::IAC, telnet::DO, telnet::GMCP])
                .into_iter()
                .flat_map(|w| w.bytes)
                .collect(),
        );
    }
    let mut mud = Mud::playing(options);
    let run = |mud: &mut Mud, line: &str| -> Vec<u8> {
        mud.command(line)
            .into_iter()
            .flat_map(|w| w.bytes)
            .collect()
    };
    let bytes = match name {
        "quiet" | "ga" => run(&mut mud, "look"),
        "fight-tank" => run(&mut mud, "fight"),
        "lament-new" | "lament-243cac5c" | "lament-older" => {
            let _ = run(&mut mud, "fight");
            run(&mut mud, "lament")
        }
        "prompt-all-next" => {
            let mut bytes = run(&mut mud, "look");
            bytes.extend(mud.pulse_later(ARRIVES));
            bytes
        }
        "prompt-x-new" => run(&mut mud, &format!("prompt {TYPED_X}")),
        "prompts-off-new" => {
            let mut bytes = run(&mut mud, "prompt off");
            for n in 1..=3 {
                bytes.extend(mud.pulse_later(&format!("Pulse {n} of 3.")));
            }
            bytes
        }
        _ => return None,
    };
    Some(bytes)
}
