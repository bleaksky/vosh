//! `prompt_state_get` and `session://prompt-state`: the
//! catalog with each field's live state and source, the status, whether
//! the server is the new build, and the open row with its spans.

use serde::Serialize;

use crate::design::{FieldRef, Format};
use crate::engine::{PromptEngine, StatusReport};
use crate::render::Span;
use crate::values::format::{Resolved, Value};
use crate::values::{self, ClientValues, Entry, Group, Kind, Source, Values, Vars, CATALOG};

/// A field's state now.
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
    pub state: State,
    pub source: Option<Source>,
    /// The value as the picker shows it, an enum as its word. None
    /// unless the state is a value.
    pub value: Option<String>,
    /// A gauge's max, the same way.
    pub max: Option<String>,
    /// Its package has come this session, or it has none. A package
    /// older builds send too counts for a new build field only on the new
    /// build.
    pub sent: bool,
    /// Your prompt shows it: the capture reads it.
    pub in_prompt: bool,
}

/// The open row and where each piece of the design landed in it, with
/// the rows the design draws as plain text joined by `\n`, which the
/// webview wraps at its renderer's width to put each span on screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenRowState {
    pub gen: u64,
    pub spans: Vec<Span>,
    pub plain: String,
    /// The game's own lines the drawn prompt replaced, as plain text,
    /// which the row shows while the card reads your codes or drawing is
    /// off, so the card's marks can sit on them.
    pub raw_lines: Vec<String>,
    /// The index in the prompt the game sent of the first of them, so a
    /// mark that names a line of that prompt finds its row.
    pub raw_from: usize,
}

/// `prompt_state_get`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PromptState {
    pub catalog: Vec<FieldState>,
    pub status: StatusReport,
    pub new_build: bool,
    /// The Forsaken Lands rules hold: the host is The Forsaken
    /// Lands or the capture reads its codes.
    pub forsaken: bool,
    pub open_row: Option<OpenRowState>,
    /// The GMCP packages that came this session, for More from the game.
    pub packages: Vec<String>,
}

// The engine's state is what the card receives, so it is built here
// beside the types it fills, and the engine never imports them.
impl PromptEngine {
    /// Everything the card reads about your prompt now, for
    /// `prompt_state_get` and `session://prompt-state`: each field with its
    /// state and source, the status, the new build sign and the open row
    /// with where each piece of the design landed in it.
    pub fn state(&self, client: &ClientValues) -> PromptState {
        let reads = self
            .stage
            .recognizer()
            .map(crate::capture::Recognizer::reads)
            .unwrap_or_default();
        PromptState {
            catalog: catalog(&self.vars, client, &reads),
            status: self.status_report(),
            new_build: self.vars.new_build(),
            forsaken: self.forsaken(),
            open_row: self.stage.open_row().map(|open| {
                let block = self.stage.last_raw();
                let replaced = block.map(|b| b.replaced.clone()).unwrap_or_default();
                OpenRowState {
                    gen: open.gen,
                    spans: open.spans.clone(),
                    plain: open.plain.clone(),
                    raw_lines: replaced
                        .iter()
                        .filter_map(|i| block.and_then(|b| b.lines.get(*i)))
                        .map(|line| line.plain.clone())
                        .collect(),
                    raw_from: replaced.first().copied().unwrap_or(0),
                }
            }),
            packages: self.vars.gmcp().packages().map(str::to_string).collect(),
        }
    }
}

/// Every catalog field and every name only scripts set, with its state
/// now. `reads` holds the names the capture fills.
pub fn catalog(vars: &Vars, client: &ClientValues, reads: &[String]) -> Vec<FieldState> {
    let resolver = vars.resolver(client);
    let mut out: Vec<FieldState> = CATALOG
        .iter()
        .map(|e| field_state(e, vars, client, &resolver, reads))
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
            package: None,
            new_build: false,
            codes: &[],
            search: &[],
            param: false,
            listed: true,
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
    client: &ClientValues,
    resolver: &values::Resolver<'_>,
    reads: &[String],
) -> FieldState {
    let (state, value, max, source) = if e.param {
        // A field with a parameter is a form to fill in, not one value.
        (State::Missing, None, None, None)
    } else {
        let field = FieldRef::new(e.name);
        let (state, mut value, max) =
            shown(&resolver.resolve(&field), e.kind, &resolver.label(&field));
        // The game's prompt reads as text beside its name, not the color
        // codes it came with.
        if e.kind == Kind::Raw {
            value = value.map(|v| crate::stage::plain_text(&v));
        }
        (state, value, max, vars.source(e, client))
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
        package: e.package,
        new_build: e.new_build,
        codes: e.codes,
        search: e.search,
        param: e.param,
        listed: e.listed,
        state,
        source,
        value,
        max,
        // A package older builds send too feeds a new build field only
        // on the new build: your tank in Char.Combat, Exits from
        // Room.Info.
        sent: e.package.map_or(true, |p| {
            vars.gmcp().has(p) && (!e.new_build || vars.new_build())
        }),
        in_prompt: reads.iter().any(|r| values::feeds(r) == e.name),
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
