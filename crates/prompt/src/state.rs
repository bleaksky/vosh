//! `prompt_state_get` and `session://prompt-state` (section 6): the
//! catalog with each field's live state and source, the status, whether
//! the server is the new build, and the open row with its spans.

use serde::Serialize;

use crate::engine::StatusReport;
use crate::format::{Resolved, Value};
use crate::render::{Span, Values};
use crate::template::{FieldRef, Format};
use crate::vars::{self, Entry, FormatId, Group, Kind, Source, Vars, Vosh, CATALOG};

/// A field's state now (D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Value,
    /// The game hides it now.
    Hidden,
    /// It does not apply now, such as no tank out of a fight.
    Absent,
    /// No source has it yet.
    Missing,
}

/// One field of the catalog with its live state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldState {
    pub name: String,
    pub label: String,
    pub aliases: &'static [&'static str],
    pub kind: Kind,
    pub group: Group,
    /// The GMCP source as the picker shows it.
    pub gmcp: Option<&'static str>,
    pub package: Option<&'static str>,
    /// Only the new server build sends its package.
    pub new_build: bool,
    /// The PROMPT codes that feed it.
    pub codes: &'static [&'static str],
    pub search: &'static [&'static str],
    /// Written with a parameter, `%{aff:sanctuary}`.
    pub param: bool,
    /// Shown as its own row in the picker.
    pub listed: bool,
    /// The formats it offers, the value first.
    pub formats: &'static [FormatId],
    pub state: State,
    pub source: Option<Source>,
    /// The value as the picker shows it, an enum as its word. None
    /// unless the state is a value.
    pub value: Option<String>,
    /// A gauge's max, the same way.
    pub max: Option<String>,
    /// Its package has come this session, or it has none.
    pub sent: bool,
    /// Your prompt shows it: the capture reads it.
    pub in_prompt: bool,
}

/// The open row and where each piece of the design landed in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenRowState {
    pub gen: u64,
    pub spans: Vec<Span>,
}

/// `prompt_state_get`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PromptState {
    pub catalog: Vec<FieldState>,
    pub status: StatusReport,
    pub new_build: bool,
    pub open_row: Option<OpenRowState>,
    /// The GMCP packages that came this session, for More from the game.
    pub packages: Vec<String>,
}

/// Every catalog field and every name only scripts set, with its state
/// now. `reads` holds the names the capture fills.
pub fn catalog(vars: &Vars, vosh: &Vosh, reads: &[String]) -> Vec<FieldState> {
    let resolver = vars.resolver(vosh);
    let mut out: Vec<FieldState> = CATALOG
        .iter()
        .map(|e| field_state(e, vars, vosh, &resolver, reads))
        .collect();
    for name in vars.script_names() {
        let field = FieldRef::new(name);
        let resolved = resolver.resolve(&field);
        let (state, value, max) = shown(&resolved, Kind::Text, name);
        out.push(FieldState {
            name: name.to_string(),
            label: name.to_string(),
            aliases: &[],
            kind: Kind::Text,
            group: Group::Scripts,
            gmcp: None,
            package: None,
            new_build: false,
            codes: &[],
            search: &[],
            param: false,
            listed: true,
            formats: Kind::Text.formats(),
            state,
            source: Some(Source::Script),
            value,
            max,
            sent: true,
            in_prompt: false,
        });
    }
    out
}

fn field_state(
    e: &'static Entry,
    vars: &Vars,
    vosh: &Vosh,
    resolver: &vars::Resolver<'_>,
    reads: &[String],
) -> FieldState {
    let (state, value, max, source) = if e.param {
        // A field with a parameter is a form to fill in, not one value.
        (State::Missing, None, None, None)
    } else {
        let field = FieldRef::new(e.name);
        let (state, value, max) = shown(&resolver.resolve(&field), e.kind, &resolver.label(&field));
        (state, value, max, vars.source(e, vosh))
    };
    FieldState {
        name: e.name.to_string(),
        label: if e.param {
            e.label.to_string()
        } else {
            resolver.label(&FieldRef::new(e.name))
        },
        aliases: e.aliases,
        kind: e.kind,
        group: e.group,
        gmcp: e.gmcp,
        package: e.package,
        new_build: e.new_build,
        codes: e.codes,
        search: e.search,
        param: e.param,
        listed: e.listed,
        formats: e.kind.formats(),
        state,
        source,
        value,
        max,
        sent: e.package.map_or(true, |p| vars.gmcp().has(p)),
        in_prompt: reads.iter().any(|r| vars::feeds(r) == e.name),
    }
}

/// A resolved field as a state and the text the picker shows.
fn shown(resolved: &Resolved, kind: Kind, label: &str) -> (State, Option<String>, Option<String>) {
    match resolved {
        Resolved::Value(value) => {
            let format = match kind {
                Kind::Position | Kind::Moon => Format::Word,
                _ => Format::Value,
            };
            let max = match value {
                Value::Gauge { max: Some(_), .. }
                | Value::Seconds { max: Some(_), .. }
                | Value::Decimal { max: Some(_), .. } => value.text(&Format::Max, label),
                _ => None,
            };
            (State::Value, value.text(&format, label), max)
        }
        Resolved::Hidden => (State::Hidden, None, None),
        Resolved::Absent => (State::Absent, None, None),
        Resolved::Missing | Resolved::Unknown => (State::Missing, None, None),
    }
}
