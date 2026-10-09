//! Who a piece of Lua belongs to, so a stop can name it and Vosh knows
//! what to turn off.

/// Who a piece of Lua belongs to. Every call runs for one owner, and a
/// function a call hands to `mud.trigger`, `mud.on_gmcp` or `mud.timer`
/// keeps the owner of the call that made it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Owner {
    /// A plugin, by the name in its manifest.
    Plugin(String),
    /// A loose file from the scripts folder, by its path inside the
    /// folder with `.lua` on the end.
    Script(String),
    /// A `#lua` line you typed.
    Typed,
    /// The Script action of the trigger of this name in this group, None
    /// for the one in no group. Two groups may each hold a trigger of one
    /// name.
    Trigger { name: String, group: Option<String> },
    /// The Lua body of the alias of this name in this group, None for
    /// the one in no group. Two groups may each hold an alias of one
    /// name.
    Alias { name: String, group: Option<String> },
}

impl Owner {
    /// The owner of the Script action of the trigger `name` in no group.
    pub fn trigger(name: impl Into<String>) -> Self {
        Owner::Trigger {
            name: name.into(),
            group: None,
        }
    }

    /// The owner of the body of the alias `name` in no group.
    pub fn alias(name: impl Into<String>) -> Self {
        Owner::Alias {
            name: name.into(),
            group: None,
        }
    }

    /// How the lines name a trigger or an alias: its name, and its group
    /// after it when it has one, like `ds in Tolliver`. Empty for any
    /// other owner.
    pub(crate) fn item_label(&self) -> String {
        match self {
            Owner::Trigger {
                name,
                group: Some(group),
            }
            | Owner::Alias {
                name,
                group: Some(group),
            } => format!("{name} in {group}"),
            Owner::Trigger { name, group: None } | Owner::Alias { name, group: None } => {
                name.clone()
            }
            _ => String::new(),
        }
    }

    /// The tag the alerts of this owner carry, the name `#scripts` lists
    /// it under, such as `plugin:vitals_alert`.
    pub fn tag(&self) -> String {
        self.listed_name()
    }

    /// The name `#scripts` lists a loaded script under.
    pub(crate) fn listed_name(&self) -> String {
        match self {
            Owner::Plugin(name) => format!("plugin:{name}"),
            Owner::Script(name) => name.clone(),
            Owner::Typed => "#lua".to_string(),
            Owner::Trigger { .. } => format!("trigger {}", self.item_label()),
            Owner::Alias { .. } => format!("alias {}", self.item_label()),
        }
    }

    /// True when a Lua trigger `self` registers shares its name with one
    /// `other` registers, so it replaces it or `mud.untrigger` removes
    /// it. Each plugin and loose script has names of its own, and your
    /// `#lua` lines and the bodies of your triggers and aliases share
    /// theirs.
    pub(crate) fn shares_names_with(&self, other: &Owner) -> bool {
        match (self, other) {
            (Owner::Plugin(a), Owner::Plugin(b)) | (Owner::Script(a), Owner::Script(b)) => a == b,
            (Owner::Plugin(_) | Owner::Script(_), _) | (_, Owner::Plugin(_) | Owner::Script(_)) => {
                false
            }
            _ => true,
        }
    }

    /// The chunk name a body of this owner runs under, which Lua puts
    /// before the line number of an error in it.
    pub(crate) fn body_chunk(&self) -> String {
        match self {
            Owner::Trigger { .. } => format!("=trigger {}", self.item_label()),
            Owner::Alias { .. } => format!("=alias {}", self.item_label()),
            Owner::Typed => "=#lua".to_string(),
            Owner::Plugin(name) | Owner::Script(name) => format!("={name}"),
        }
    }
}

/// What part of an owner's Lua a call runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Site {
    /// The owner's own code: a load, a body or a `#lua` line.
    Entry,
    /// The function of the Lua trigger of this name.
    LuaTrigger { name: String, callback_id: i64 },
    /// A handler of this GMCP package.
    Gmcp { package: String, callback_id: i64 },
    /// A timer.
    Timer { callback_id: i64 },
}

impl Site {
    /// The callback the call runs, when it runs one.
    pub(crate) fn callback_id(&self) -> Option<i64> {
        match self {
            Site::Entry => None,
            Site::LuaTrigger { callback_id, .. }
            | Site::Gmcp { callback_id, .. }
            | Site::Timer { callback_id } => Some(*callback_id),
        }
    }
}
