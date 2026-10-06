//! The worlds Vosh knows by name. A host on one of their domains shows
//! as the world it belongs to, and Vosh knows the port you connect on.

/// A world Vosh knows by name. Mirrors `KNOWN_WORLDS` in
/// src/lib/knownWorlds.ts, and a test here reads that list.
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

/// A host as Vosh compares it, trimmed, in lower case and without a
/// closing dot. Mirrors `hostKey` in src/lib/knownWorlds.ts.
pub(crate) fn host_key(host: &str) -> String {
    let lower = host.trim().to_ascii_lowercase();
    match lower.strip_suffix('.') {
        Some(clean) => clean.to_string(),
        None => lower,
    }
}

/// The known world a host belongs to, if any.
pub(crate) fn known_world(host: &str) -> Option<&'static KnownWorld> {
    let clean = host_key(host);
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
/// `worldName` in src/lib/knownWorlds.ts.
pub(crate) fn world_name(host: &str) -> String {
    known_world(host).map_or_else(|| host.trim().to_string(), |w| w.name.to_string())
}

/// The world a host and port play, like `The Forsaken Lands` on its own
/// port 1848 and `The Forsaken Lands 1825` on the build port. A known
/// world adds a port that is not its own, so two ports of one game read
/// apart. Any other host shows as typed. Mirrors `worldLabel` in
/// src/lib/knownWorlds.ts.
pub(crate) fn world_label(host: &str, port: u16) -> String {
    match known_world(host) {
        Some(world) if world.port == port => world.name.to_string(),
        Some(world) => format!("{} {port}", world.name),
        None => host.trim().to_string(),
    }
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
    fn world_label_adds_a_port_that_is_not_the_world_own() {
        assert_eq!(
            world_label("play.theforsakenlands.com", 1848),
            "The Forsaken Lands"
        );
        assert_eq!(
            world_label("play.theforsakenlands.com", 1825),
            "The Forsaken Lands 1825"
        );
        assert_eq!(world_label(" mud.example.org ", 4000), "mud.example.org");
        assert_eq!(host_key(" MUD.Example.org. "), "mud.example.org");
    }

    #[test]
    fn known_worlds_match_the_list_the_page_shows() {
        // The page keeps its own copy in src/lib/knownWorlds.ts, with
        // the host it dials for each world.
        let source = include_str!("../../../src/lib/knownWorlds.ts");
        let start = source
            .find("export const KNOWN_WORLDS")
            .expect("knownWorlds.ts declares KNOWN_WORLDS");
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
