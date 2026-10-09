//! The Lua call a script alias or a trigger Script action queues.

/// One Lua body to run, with the captures it runs against. A script
/// alias fills `captures` with the words typed after its name, split on
/// whitespace. A trigger Script action fills it with the capture groups
/// of one match, the whole match first and then the numbered groups.
/// Either way Lua reads them as `captures[1]` onward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptCall {
    /// The name of the trigger or alias that holds the body, so a body
    /// Vosh stops turns that one off.
    pub source: String,
    /// The group of the alias that holds the body, since two groups may
    /// each hold an alias of one name. None for a trigger and for an
    /// alias in no group.
    pub group: Option<String>,
    pub body: String,
    pub captures: Vec<String>,
}
