//! Where Vosh last saw your prompt settings, for the card's first step.
//!
//! On the new build the latest Char.Prompt says it. Without one this
//! session, the setting the game showed after your own `prompt` does,
//! and then the newest such reply in your log that belongs to one of this
//! profile's characters. The log lookup runs on a blocking thread over
//! the read connection, never on the session task, and its query compares
//! sent lines only against the characters profiles claim.

use chrono::{DateTime, FixedOffset, Local, SecondsFormat};
use serde::Serialize;
use vosh_log::{CharacterScope, LogStore};
use vosh_prompt::aabahran::observer;

use crate::app::state::SharedState;
use crate::profile::set::ProfileEntry;
use crate::sessions::Session;

/// `prompt_last_seen`: your prompt settings and where Vosh saw them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct LastSeen {
    pub prompt: Option<String>,
    pub fprompt: Option<String>,
    /// Your text prompt is on, as Char.Prompt says. None from any other
    /// source.
    pub enabled: Option<bool>,
    /// When Vosh saw it, RFC 3339 local time.
    pub at: Option<String>,
    /// The first Char.Prompt since you connected, which the game sends
    /// at login.
    pub at_login: bool,
    /// `gmcp`, `session` or `log`.
    pub source: &'static str,
    /// The character it belongs to, when known.
    pub character: Option<String>,
}

/// Who the log lookup reads for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Characters {
    /// The characters the profile claims.
    pub mine: Vec<String>,
    /// Every character a profile claims on the host and port.
    pub claimed: Vec<String>,
    /// A profile plays the host and port with no character named.
    pub open: bool,
}

impl Characters {
    /// The lookup's scope on `host` and `port`.
    pub(crate) fn scope<'a>(&'a self, host: &'a str, port: u16) -> CharacterScope<'a> {
        CharacterScope {
            host,
            port,
            mine: &self.mine,
            claimed: &self.claimed,
            open: self.open,
        }
    }
}

/// Who the log lookup reads for: the characters `profile` claims, every
/// character a profile claims on `host` and `port`, and whether a
/// profile there claims none. That one could play anyone, so an older
/// session with no name never falls to the one character claimed there.
/// A world whose login toggle is off counts too, since you can still
/// connect with its profile.
pub(crate) fn character_scope(
    profiles: &[ProfileEntry],
    profile: &str,
    host: &str,
    port: u16,
) -> Characters {
    let host = host.trim().to_ascii_lowercase();
    let mine = profiles
        .iter()
        .find(|p| p.name == profile)
        .and_then(|p| p.auto_match.as_ref())
        .map(|am| am.characters.clone())
        .unwrap_or_default();
    let here: Vec<_> = profiles
        .iter()
        .filter_map(|p| p.auto_match.as_ref())
        .filter(|am| {
            am.host
                .as_deref()
                .is_some_and(|h| h.trim().to_ascii_lowercase() == host)
                && am.port.map_or(true, |p| p == port)
        })
        .collect();
    Characters {
        mine,
        claimed: here
            .iter()
            .flat_map(|am| am.characters.iter().cloned())
            .collect(),
        open: here.iter().any(|am| am.characters.is_empty()),
    }
}

/// The newest prompt reply in your log that belongs to one of `scope`'s
/// characters. None sends the card to its empty state.
pub(crate) fn logged(store: &LogStore, scope: &CharacterScope) -> Option<LastSeen> {
    let found = store.find_in_sessions(scope, &observer::PREFIXES, |session, lines| {
        let logged: Vec<observer::Logged<'_>> = lines
            .iter()
            .map(|line| observer::Logged {
                text: &line.text,
                raw: line.raw.as_deref(),
                ts_ms: line.ts_ms,
            })
            .collect();
        observer::latest(&logged).map(|found| (session.character.clone(), found))
    });
    let (character, found) = match found {
        Ok(found) => found?,
        Err(e) => {
            tracing::warn!(error = %e, "the prompt lookup failed");
            return None;
        }
    };
    Some(LastSeen {
        prompt: Some(found.prompt),
        fprompt: found.fprompt,
        enabled: None,
        at: DateTime::from_timestamp_millis(found.at_ms)
            .map(|at| stamp(at.with_timezone(&Local).fixed_offset())),
        at_login: false,
        source: "log",
        character: Some(character),
    })
}

fn stamp(at: DateTime<FixedOffset>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Secs, false)
}

/// The body of [`prompt_last_seen`], for `session`.
///
/// [`prompt_last_seen`]: crate::ipc::prompt::prompt_last_seen
pub(crate) async fn last_seen(state: &SharedState, session: &Session) -> Option<LastSeen> {
    let character = session
        .current_character
        .lock()
        .ok()
        .and_then(|g| g.clone());
    {
        let c = session.connection.lock();
        if let Some(packet) = c.prompt.vars.gmcp().char_prompt() {
            return Some(LastSeen {
                prompt: Some(packet.prompt.clone()),
                fprompt: Some(packet.fprompt.clone()),
                enabled: Some(packet.enabled),
                at: Some(stamp(packet.at)),
                at_login: packet.at_login,
                source: "gmcp",
                character,
            });
        }
        if let Some(seen) = c.prompt.session_setting().filter(|s| s.prompt.is_some()) {
            return Some(LastSeen {
                prompt: seen.prompt.clone(),
                fprompt: seen.fprompt.clone(),
                enabled: None,
                at: Some(stamp(seen.at)),
                at_login: false,
                source: "session",
                character,
            });
        }
    }
    let (host, port, characters) = {
        let connection = session
            .current_connection
            .lock()
            .ok()
            .and_then(|g| g.clone());
        let guard = state.profile_set.lock().await;
        let set = guard.as_ref()?;
        let active = set.get(set.active_name())?;
        let (host, port) = match connection {
            Some(connection) => connection,
            None => {
                let am = active.auto_match.as_ref()?;
                let host = am.host.clone()?;
                let port = am
                    .port
                    .or_else(|| crate::profile::worlds::known_world(&host).map(|w| w.port))?;
                (host, port)
            }
        };
        let characters = character_scope(set.list(), set.active_name(), &host, port);
        (host, port, characters)
    };
    if characters.mine.is_empty() {
        return None;
    }
    let reader = state.log_reader.clone();
    let writer = state.logs.clone();
    let lookup = tauri::async_runtime::spawn_blocking(move || {
        let scope = characters.scope(&host, port);
        {
            let guard = reader.blocking_lock();
            if let Some(store) = guard.as_ref() {
                return logged(store, &scope);
            }
        }
        let guard = writer.blocking_lock();
        guard.as_ref().and_then(|store| logged(store, &scope))
    });
    lookup.await.ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::login_match::AutoMatch;

    const HOST: &str = "play.example.com";

    fn entry(name: &str, host: &str, port: Option<u16>, characters: &[&str]) -> ProfileEntry {
        ProfileEntry {
            name: name.into(),
            description: None,
            auto_match: Some(AutoMatch {
                host: Some(host.into()),
                port,
                characters: characters.iter().map(|c| (*c).to_string()).collect(),
                enabled: true,
            }),
        }
    }

    /// Profiles shaped like James's: two claim Tester, one claims Healer,
    /// and a profile on another game claims someone else.
    fn profiles() -> Vec<ProfileEntry> {
        vec![
            entry("default", HOST, Some(1848), &["Tester"]),
            entry("Test-Prompt", HOST, None, &["Tester"]),
            entry("Healer", HOST, Some(1848), &["Healer"]),
            entry("Elsewhere", "other.example.com", Some(1848), &["Stranger"]),
        ]
    }

    fn lookup(store: &LogStore, profile: &str) -> Option<LastSeen> {
        lookup_in(&profiles(), store, profile)
    }

    fn lookup_in(profiles: &[ProfileEntry], store: &LogStore, profile: &str) -> Option<LastSeen> {
        let characters = character_scope(profiles, profile, HOST, 1848);
        logged(store, &characters.scope(HOST, 1848))
    }

    fn session(store: &mut LogStore, character: Option<&str>, lines: &[&str]) {
        let id = store.start_session(HOST, 1848, 1_000).unwrap();
        if let Some(name) = character {
            store.set_session_character(id, name).unwrap();
        }
        for (n, line) in lines.iter().enumerate() {
            store.append(id, 1_000 + n as i64, line, None).unwrap();
        }
    }

    #[test]
    fn the_scope_names_this_profiles_characters_and_every_claimed_one() {
        let characters = character_scope(&profiles(), "Healer", "PLAY.example.com ", 1848);
        assert_eq!(characters.mine, ["Healer"]);
        assert_eq!(characters.claimed, ["Tester", "Tester", "Healer"]);
        assert!(!characters.open);
        let characters = character_scope(&profiles(), "default", HOST, 4000);
        assert_eq!(
            characters.claimed,
            ["Tester"],
            "only the profile that pins no port"
        );
    }

    #[test]
    fn a_profile_that_names_no_character_opens_the_game_to_anyone() {
        let mut host_only = entry("default", HOST, None, &[]);
        let profiles = vec![
            host_only.clone(),
            entry("Healer", HOST, Some(1848), &["Healer"]),
        ];
        assert!(character_scope(&profiles, "Healer", HOST, 1848).open);
        // Its login toggle off, you can still connect with it.
        if let Some(am) = host_only.auto_match.as_mut() {
            am.enabled = false;
        }
        let off = vec![host_only, entry("Healer", HOST, Some(1848), &["Healer"])];
        assert!(character_scope(&off, "Healer", HOST, 1848).open);
        // Another game does not count.
        let elsewhere = vec![
            entry("default", "other.example.com", None, &[]),
            entry("Healer", HOST, Some(1848), &["Healer"]),
        ];
        assert!(!character_scope(&elsewhere, "Healer", HOST, 1848).open);
    }

    #[test]
    fn healer_never_takes_a_prompt_from_a_session_someone_else_played() {
        // The default profile plays the game by host alone, and Tester
        // logged in there before the log kept a character.
        let profiles = vec![
            entry("default", HOST, None, &[]),
            entry("Healer", HOST, Some(1848), &["Healer"]),
        ];
        let mut store = LogStore::in_memory().unwrap();
        session(
            &mut store,
            None,
            &["> Tester", "> prompt", "Current prompt: %h/%H "],
        );
        assert_eq!(lookup_in(&profiles, &store, "Healer"), None);
    }

    #[test]
    fn each_profile_finds_only_its_own_characters_reply() {
        let mut store = LogStore::in_memory().unwrap();
        session(
            &mut store,
            Some("Tester"),
            &[
                "> prompt",
                "Current prompt: %n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c",
            ],
        );
        let seen = lookup(&store, "default").expect("Tester's profile finds it");
        assert_eq!(
            seen.prompt.as_deref(),
            Some("%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c")
        );
        assert_eq!(seen.source, "log");
        assert_eq!(seen.character.as_deref(), Some("Tester"));
        assert!(seen.at.is_some());
        assert_eq!(
            lookup(&store, "Test-Prompt").and_then(|s| s.prompt),
            seen.prompt
        );
        assert_eq!(lookup(&store, "Healer"), None, "Healer's card starts empty");
    }

    #[test]
    fn an_older_session_goes_by_its_login_line() {
        let mut store = LogStore::in_memory().unwrap();
        session(
            &mut store,
            None,
            &["> Healer", "Prompt set to <%h%m %vmv> "],
        );
        assert_eq!(
            lookup(&store, "Healer").and_then(|s| s.prompt).as_deref(),
            Some("<%h%m %vmv> ")
        );
        assert_eq!(lookup(&store, "default"), None);
    }

    #[test]
    fn an_older_session_with_no_login_line_counts_only_for_one_claimed_character() {
        let mut store = LogStore::in_memory().unwrap();
        session(&mut store, None, &["Current prompt: %h "]);
        // Tester and Healer both play here, so nothing says whose it was.
        assert_eq!(lookup(&store, "default"), None);
        assert_eq!(lookup(&store, "Healer"), None);
        // With one character claimed on the game, it is theirs.
        let one = [entry("default", HOST, Some(1848), &["Tester"])];
        let seen = lookup_in(&one, &store, "default");
        assert_eq!(seen.and_then(|s| s.prompt).as_deref(), Some("%h "));
    }

    #[tokio::test]
    async fn last_seen_prefers_char_prompt_then_the_session() {
        let state: SharedState = std::sync::Arc::new(crate::app::state::AppState::default());
        let session = state.selected_session();
        assert_eq!(last_seen(&state, &session).await, None, "nothing anywhere");
        {
            let mut c = session.connection.lock();
            c.prompt.connect(true);
            c.prompt
                .note_send("prompt %h\r\n", chrono::Local::now().timestamp_millis());
            let now = chrono::Local::now().fixed_offset();
            c.prompt
                .observe_line(b"Prompt set to %h ", "Prompt set to %h ", now);
        }
        let seen = last_seen(&state, &session)
            .await
            .expect("the session saw it");
        assert_eq!(seen.source, "session");
        assert_eq!(seen.prompt.as_deref(), Some("%h "));
        {
            let mut c = session.connection.lock();
            c.prompt.observe(
                "Char.Prompt",
                serde_json::json!({"enabled": false, "prompt": "%m ", "fprompt": ""}),
                chrono::Local::now().fixed_offset(),
            );
        }
        let seen = last_seen(&state, &session).await.expect("the game sent it");
        assert_eq!(seen.source, "gmcp");
        assert_eq!(seen.prompt.as_deref(), Some("%m "));
        assert_eq!(seen.enabled, Some(false));
        assert!(seen.at_login);
    }
}
