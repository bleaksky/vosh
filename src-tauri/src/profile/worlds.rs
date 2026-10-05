//! The worlds Vosh knows by name. A host on one of their domains shows
//! as the world it belongs to, and Vosh knows the port you connect on.

/// A world Vosh knows by name. Mirrors `KNOWN_WORLDS` in
/// src/lib/useConnection.ts, and a test here reads that list.
pub(crate) struct KnownWorld {
    /// A host matches this domain or any subdomain of it.
    pub domain: &'static str,
    pub name: &'static str,
    /// The port you connect to it on.
    pub port: u16,
}

/// The domain of The Forsaken Lands.
const FORSAKEN_LANDS: &str = "theforsakenlands.com";

pub(crate) const KNOWN_WORLDS: &[KnownWorld] = &[KnownWorld {
    domain: FORSAKEN_LANDS,
    name: "The Forsaken Lands",
    port: 1848,
}];

/// The known world a host belongs to, if any.
pub(crate) fn known_world(host: &str) -> Option<&'static KnownWorld> {
    let lower = host.trim().to_ascii_lowercase();
    let clean = lower.strip_suffix('.').unwrap_or(&lower);
    KNOWN_WORLDS.iter().find(|w| {
        clean == w.domain
            || clean
                .strip_suffix(w.domain)
                .is_some_and(|rest| rest.ends_with('.'))
    })
}

/// True when the host is The Forsaken Lands, so the custom prompt
/// follows Aabahran's rules there, the hidden values of lamented tears
/// among them.
pub(crate) fn is_forsaken_lands(host: &str) -> bool {
    known_world(host).is_some_and(|w| w.domain == FORSAKEN_LANDS)
}

/// The name Vosh shows for a host, like `The Forsaken Lands` for
/// `play.theforsakenlands.com`. Unknown hosts show as typed. Mirrors
/// `worldName` in src/lib/useConnection.ts.
pub(crate) fn world_name(host: &str) -> String {
    known_world(host).map_or_else(|| host.trim().to_string(), |w| w.name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_forsaken_lands_rules_hold_on_its_hosts_alone() {
        assert!(is_forsaken_lands("play.theforsakenlands.com"));
        assert!(is_forsaken_lands(" TheForsakenLands.com. "));
        assert!(!is_forsaken_lands("127.0.0.1"));
        assert!(!is_forsaken_lands("nottheforsakenlands.com"));
    }

    #[test]
    fn world_name_knows_the_forsaken_lands_by_any_subdomain() {
        assert_eq!(
            world_name("play.theforsakenlands.com"),
            "The Forsaken Lands"
        );
        assert_eq!(world_name(" TheForsakenLands.com. "), "The Forsaken Lands");
        assert_eq!(world_name("mud.example.org"), "mud.example.org");
        assert_eq!(
            world_name("nottheforsakenlands.com"),
            "nottheforsakenlands.com"
        );
        let world = known_world("play.theforsakenlands.com").unwrap();
        assert_eq!(world.port, 1848);
    }

    #[test]
    fn known_worlds_match_the_list_the_page_shows() {
        // The page keeps its own copy in src/lib/useConnection.ts, with
        // the host it dials for each world.
        let source = include_str!("../../../src/stores/session/useConnection.ts");
        let start = source
            .find("export const KNOWN_WORLDS")
            .expect("useConnection.ts declares KNOWN_WORLDS");
        let list = &source[start..];
        let list = &list[..list.find("\n];").expect("the page list ends")];
        let field = |entry: &str, key: &str| -> String {
            let re =
                regex::Regex::new(&format!(r#"\b{key}: (?:'([^']*)'|"([^"]*)"|(\d+))"#)).unwrap();
            let caps = re
                .captures(entry)
                .unwrap_or_else(|| panic!("no {key} in {entry}"));
            let value = caps.get(1).or_else(|| caps.get(2)).or_else(|| caps.get(3));
            value.unwrap().as_str().to_string()
        };
        let entries: Vec<&str> = list.split("\n  {").skip(1).collect();
        assert_eq!(entries.len(), KNOWN_WORLDS.len(), "{list}");
        for (entry, world) in entries.iter().zip(KNOWN_WORLDS) {
            assert_eq!(field(entry, "domain"), world.domain);
            assert_eq!(field(entry, "name"), world.name);
            assert_eq!(field(entry, "port"), world.port.to_string());
            // The host the page dials belongs to the same world here.
            let host = field(entry, "host");
            assert_eq!(
                known_world(&host).map(|w| w.domain),
                Some(world.domain),
                "{host}"
            );
        }
    }
}
