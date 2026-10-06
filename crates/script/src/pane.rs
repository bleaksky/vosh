//! `mud.pane`, the pane a plugin draws. Lua hands Vosh typed blocks,
//! never markup, and Rust checks each one and caps them before they
//! reach the page.

use mlua::{Lua, Result as LuaResult, Table, Value};

use crate::actions::{Action, PaneBlock};
use crate::api::{capped, with_state};
use crate::limits::{NAME_BYTES, PANE_BLOCKS, PANE_LINE_CHARS};
use crate::owner::Owner;

/// A Lua error with `text`.
fn fail<T>(text: impl Into<String>) -> LuaResult<T> {
    Err(mlua::Error::RuntimeError(text.into()))
}

/// `mud.pane(id, title)`. Only a plugin's own `mud` table draws a pane,
/// so the pane belongs to the plugin and goes when the plugin turns off.
/// Returns a handle whose `set` and `meta` fill the pane.
pub(crate) fn mud_pane(
    lua: &Lua,
    owner: Option<&Owner>,
    (id, title): (mlua::String, mlua::String),
) -> LuaResult<Table> {
    let Some(Owner::Plugin(plugin)) = owner else {
        return fail("Only a plugin can draw a pane.");
    };
    let (Some(id), Some(title)) = (capped(&id, NAME_BYTES)?, capped(&title, NAME_BYTES)?) else {
        return fail(format!(
            "A pane id or title holds at most {NAME_BYTES} bytes."
        ));
    };
    if id.trim().is_empty() || title.trim().is_empty() {
        return fail("mud.pane needs an id and a title.");
    }
    with_state(lua, |s| {
        s.queue(Action::Pane {
            plugin: plugin.clone(),
            id: id.clone(),
            title,
        });
        Ok(())
    })?;
    handle(lua, plugin, &id)
}

/// The table `mud.pane` returns, whose `set` and `meta` fill the pane
/// `id` of `plugin`.
fn handle(lua: &Lua, plugin: &str, id: &str) -> LuaResult<Table> {
    let pane = lua.create_table()?;
    let (p, i) = (plugin.to_string(), id.to_string());
    pane.set(
        "set",
        lua.create_function(move |lua, (_, list): (Value, Value)| {
            let Value::Table(list) = list else {
                return fail("pane:set needs a list of blocks.");
            };
            let blocks = blocks(lua, &list)?;
            with_state(lua, |s| {
                s.queue(Action::PaneSet {
                    plugin: p.clone(),
                    id: i.clone(),
                    blocks,
                });
                Ok(())
            })
        })?,
    )?;
    let (p, i) = (plugin.to_string(), id.to_string());
    pane.set(
        "meta",
        lua.create_function(move |lua, (_, text): (Value, Value)| {
            let text = text_of(lua, &text)?.unwrap_or_default();
            with_state(lua, |s| {
                s.queue(Action::PaneMeta {
                    plugin: p.clone(),
                    id: i.clone(),
                    text,
                });
                Ok(())
            })
        })?,
    )?;
    Ok(pane)
}

/// The first [`PANE_BLOCKS`] blocks of `list`, each checked. A longer
/// list notes the drop, so the call ends with a line that says so.
fn blocks(lua: &Lua, list: &Table) -> LuaResult<Vec<PaneBlock>> {
    let len = list.raw_len();
    let blocks = (1..=len.min(PANE_BLOCKS))
        .map(|n| block(lua, n, list.raw_get(n)?))
        .collect::<LuaResult<_>>()?;
    if len > PANE_BLOCKS {
        with_state(lua, |s| {
            s.drop_blocks();
            Ok(())
        })?;
    }
    Ok(blocks)
}

/// Block `n` of a list, which holds one of `row`, `gauge`, `line` or
/// `rule`.
fn block(lua: &Lua, n: usize, value: Value) -> LuaResult<PaneBlock> {
    let shape = || format!("Pane block {n} needs one of row, gauge, line or rule.");
    let Value::Table(block) = value else {
        return fail(shape());
    };
    let mut pairs = block.pairs::<String, Value>();
    let (Some(pair), None) = (pairs.next(), pairs.next()) else {
        return fail(shape());
    };
    let (kind, value) = pair.map_err(|_| mlua::Error::RuntimeError(shape()))?;
    match (kind.as_str(), value) {
        ("row", Value::Table(row)) => {
            let label = label(lua, n, "row", &row)?;
            let value = text_of(lua, &row.raw_get(2)?)?.unwrap_or_default();
            Ok(PaneBlock::Row { label, value })
        }
        ("gauge", Value::Table(gauge)) => {
            let label = label(lua, n, "gauge", &gauge)?;
            match (number(&gauge.raw_get(2)?), number(&gauge.raw_get(3)?)) {
                (Some(value), Some(max)) => Ok(PaneBlock::Gauge { label, value, max }),
                _ => fail(format!(
                    "The gauge in pane block {n} needs a number for its value and its max."
                )),
            }
        }
        ("line", line) => match text_of(lua, &line)? {
            Some(line) => Ok(PaneBlock::Line(line)),
            None => fail(format!("The line in pane block {n} needs text.")),
        },
        ("rule", Value::Boolean(true)) => Ok(PaneBlock::Rule),
        _ => fail(shape()),
    }
}

/// The label of a row or gauge, the first item of its table.
fn label(lua: &Lua, n: usize, kind: &str, item: &Table) -> LuaResult<String> {
    match text_of(lua, &item.raw_get(1)?)? {
        Some(label) => Ok(label),
        None => fail(format!("The {kind} in pane block {n} needs a label.")),
    }
}

/// A finite number, from a Lua number. A meter has no use for the
/// precision an integer past 2^53 loses.
#[allow(clippy::cast_precision_loss)]
fn number(value: &Value) -> Option<f64> {
    let n = match value {
        Value::Integer(n) => *n as f64,
        Value::Number(n) => *n,
        _ => return None,
    };
    n.is_finite().then_some(n)
}

/// Text for a pane: a string, or a number as Lua writes it, cut to
/// [`PANE_LINE_CHARS`] characters. None for nil or false, which a row
/// shows as empty. Anything else is an error.
fn text_of(lua: &Lua, value: &Value) -> LuaResult<Option<String>> {
    let text = match value {
        Value::Nil | Value::Boolean(false) => return Ok(None),
        Value::String(text) => text.clone(),
        Value::Integer(_) | Value::Number(_) => match lua.coerce_string(value.clone())? {
            Some(text) => text,
            None => return Ok(None),
        },
        _ => return fail("A pane shows text or numbers."),
    };
    // A character holds four bytes at most, so the head holds the
    // characters the cut keeps, and Rust copies no more than that.
    let bytes = text.as_bytes();
    let head = &bytes[..bytes.len().min(PANE_LINE_CHARS * 4)];
    Ok(Some(
        String::from_utf8_lossy(head)
            .chars()
            .take(PANE_LINE_CHARS)
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::error_lines;
    use crate::{ScriptEngine, ScriptOutcome};

    const WEATHER: &str = "weather_pane";

    fn weather() -> Owner {
        Owner::Plugin(WEATHER.into())
    }

    /// Load `code` as the plugin `weather_pane` in `e`.
    fn load(e: &mut ScriptEngine, code: &str) -> ScriptOutcome {
        e.load_script(weather(), "@weather_pane/main.lua", code)
    }

    /// What `blocks`, Lua for a list of blocks, sets on a new pane.
    fn set(blocks: &str) -> ScriptOutcome {
        let mut e = ScriptEngine::new().unwrap();
        load(
            &mut e,
            &format!("local pane = mud.pane('weather', 'Weather') pane:set({blocks})"),
        )
    }

    /// The blocks the one `PaneSet` in `outcome` holds.
    fn blocks_set(outcome: &ScriptOutcome) -> Vec<PaneBlock> {
        let sets: Vec<&Vec<PaneBlock>> = outcome
            .actions
            .iter()
            .filter_map(|action| match action {
                Action::PaneSet { blocks, .. } => Some(blocks),
                _ => None,
            })
            .collect();
        assert_eq!(sets.len(), 1, "{:?}", outcome.actions);
        sets[0].clone()
    }

    fn row(label: &str, value: &str) -> PaneBlock {
        PaneBlock::Row {
            label: label.into(),
            value: value.into(),
        }
    }

    #[test]
    fn a_plugin_draws_a_pane_and_fills_it_in_order() {
        let mut e = ScriptEngine::new().unwrap();
        let loaded = load(
            &mut e,
            "local pane = mud.pane('weather', 'Weather') \
             pane:set({ { row = { 'Sky', 'rainy' } }, { gauge = { 'Health', 1020, 1200 } }, \
                        { line = '{red}It is pitch black ...{reset}' }, { rule = true } }) \
             pane:meta('Coastal North')",
        );
        assert!(!loaded.failed, "{:?}", loaded.actions);
        let (plugin, id) = (WEATHER.to_string(), "weather".to_string());
        assert_eq!(
            loaded.actions,
            vec![
                Action::DropPlugin(plugin.clone()),
                Action::Pane {
                    plugin: plugin.clone(),
                    id: id.clone(),
                    title: "Weather".into(),
                },
                Action::PaneSet {
                    plugin: plugin.clone(),
                    id: id.clone(),
                    blocks: vec![
                        row("Sky", "rainy"),
                        PaneBlock::Gauge {
                            label: "Health".into(),
                            value: 1020.0,
                            max: 1200.0,
                        },
                        PaneBlock::Line("{red}It is pitch black ...{reset}".into()),
                        PaneBlock::Rule,
                    ],
                },
                Action::PaneMeta {
                    plugin,
                    id,
                    text: "Coastal North".into(),
                },
            ]
        );
    }

    #[test]
    fn only_a_plugin_draws_a_pane() {
        const ONLY: &str = "Only a plugin can draw a pane.";
        let mut e = ScriptEngine::new().unwrap();
        let typed = e.eval("mud.pane('weather', 'Weather')", "=#lua");
        assert_eq!(error_lines(&typed), [format!("#lua:1: {ONLY}")]);
        let script = e.load_script(
            Owner::Script("weather.lua".into()),
            "@weather.lua",
            "mud.pane('weather', 'Weather')",
        );
        assert_eq!(error_lines(&script), [format!("weather.lua:1: {ONLY}")]);
        let body = e.run_body(
            &Owner::Trigger("rain".into()),
            "mud.pane('weather', 'Weather')",
            &[],
        );
        assert!(body.failed);
        assert!(error_lines(&body)[0].ends_with(ONLY), "{body:?}");
        for outcome in [typed, script, body] {
            assert!(
                !outcome
                    .actions
                    .iter()
                    .any(|action| matches!(action, Action::Pane { .. })),
                "{:?}",
                outcome.actions
            );
        }
    }

    #[test]
    fn a_pane_needs_an_id_and_a_title() {
        let mut e = ScriptEngine::new().unwrap();
        let blank = load(&mut e, "mud.pane(' ', 'Weather')");
        assert_eq!(
            error_lines(&blank),
            ["weather_pane/main.lua:1: mud.pane needs an id and a title."]
        );
        let long = load(&mut e, "mud.pane(string.rep('w', 4097), 'Weather')");
        assert_eq!(
            error_lines(&long),
            ["weather_pane/main.lua:1: A pane id or title holds at most 4096 bytes."]
        );
    }

    #[test]
    fn a_block_of_the_wrong_shape_is_an_error() {
        for (blocks, line) in [
            (
                "{ { cell = 'x' } }",
                "Pane block 1 needs one of row, gauge, line or rule.",
            ),
            (
                "{ { row = { 'Sky', 'rainy' }, line = 'x' } }",
                "Pane block 1 needs one of row, gauge, line or rule.",
            ),
            (
                "{ { rule = true }, 'Sky' }",
                "Pane block 2 needs one of row, gauge, line or rule.",
            ),
            (
                "{ { gauge = { 'Health', 1020 } } }",
                "The gauge in pane block 1 needs a number for its value and its max.",
            ),
            (
                "{ { gauge = { 'Health', 0/0, 1200 } } }",
                "The gauge in pane block 1 needs a number for its value and its max.",
            ),
            (
                "{ { row = { nil, 'rainy' } } }",
                "The row in pane block 1 needs a label.",
            ),
            (
                "{ { row = { 'Sky', {} } } }",
                "A pane shows text or numbers.",
            ),
            ("'rainy'", "pane:set needs a list of blocks."),
        ] {
            let outcome = set(blocks);
            assert!(outcome.failed, "{blocks}");
            assert_eq!(
                error_lines(&outcome),
                [format!("weather_pane/main.lua:1: {line}")],
                "{blocks}"
            );
        }
    }

    #[test]
    fn a_row_with_no_value_keeps_its_label_and_numbers_become_text() {
        let outcome = set(
            "{ { row = { 'Temperature', nil } }, { row = { 'Language', false } }, \
               { row = { 'Sky', 60 } }, { row = { 1.5, 60.0 } }, { line = 42 } }",
        );
        assert_eq!(
            blocks_set(&outcome),
            [
                row("Temperature", ""),
                row("Language", ""),
                row("Sky", "60"),
                row("1.5", "60.0"),
                PaneBlock::Line("42".into()),
            ]
        );
    }

    #[test]
    fn a_pane_shows_200_blocks_and_says_so_past_them() {
        let outcome = set("(function() local t = {} \
               for i = 1, 201 do t[i] = { line = 'Tolliver' } end return t end)()");
        assert!(!outcome.failed);
        assert_eq!(blocks_set(&outcome).len(), 200);
        assert_eq!(
            error_lines(&outcome),
            ["weather_pane gave a pane more than 200 blocks. Vosh shows the first 200."]
        );
        let full = set("(function() local t = {} \
               for i = 1, 200 do t[i] = { rule = true } end return t end)()");
        assert_eq!(blocks_set(&full).len(), 200);
        assert_eq!(error_lines(&full), Vec::<String>::new());
    }

    #[test]
    fn a_long_line_is_cut_to_500_characters() {
        let outcome = set(
            "{ { line = string.rep('a', 600) }, { line = string.rep('é', 600) }, \
               { row = { string.rep('ü', 501), string.rep('雨', 600) } } }",
        );
        assert_eq!(
            blocks_set(&outcome),
            [
                PaneBlock::Line("a".repeat(500)),
                PaneBlock::Line("é".repeat(500)),
                row(&"ü".repeat(500), &"雨".repeat(500)),
            ]
        );
    }

    #[test]
    fn a_pane_counts_toward_what_one_call_may_queue() {
        let outcome = set("(function() local t = {} \
               for i = 1, 200 do t[i] = { line = string.rep('雨', 500) } end return t end)()");
        // 200 lines of 1,500 bytes pass the 256 KB one call may queue.
        assert!(!outcome
            .actions
            .iter()
            .any(|action| matches!(action, Action::PaneSet { .. })));
        assert_eq!(
            error_lines(&outcome),
            ["weather_pane queued more text than one call may. Vosh dropped what went past the limit."]
        );
    }

    /// Where `DropPlugin` sits in `outcome`, after each timer cancel.
    #[track_caller]
    fn drops_after_release(outcome: &ScriptOutcome) {
        let at = |want: fn(&Action) -> bool| outcome.actions.iter().position(want);
        let drop = at(|a| matches!(a, Action::DropPlugin(name) if name == WEATHER))
            .unwrap_or_else(|| panic!("no DropPlugin in {:?}", outcome.actions));
        let cancel = at(|a| matches!(a, Action::CancelTimer(_)))
            .unwrap_or_else(|| panic!("no release in {:?}", outcome.actions));
        assert!(cancel < drop, "{:?}", outcome.actions);
    }

    #[test]
    fn a_plugin_s_panes_go_with_it_and_a_failed_load_keeps_them() {
        const CODE: &str = "pane = mud.pane('weather', 'Weather') mud.timer(60, function() end)";
        let mut e = ScriptEngine::new().unwrap();
        assert!(!load(&mut e, CODE).failed);
        // A reload that runs to its end ends the old pane first.
        let reloaded = load(&mut e, CODE);
        drops_after_release(&reloaded);
        // One that fails keeps the pane it had and draws none.
        let failed = load(&mut e, "mud.pane('worth', 'Worth') error('typo')");
        assert_eq!(error_lines(&failed), ["weather_pane/main.lua:1: typo"]);
        assert_eq!(failed.actions.len(), 1, "{:?}", failed.actions);
        drops_after_release(&e.unload(&weather()));
        // A stop ends it too.
        assert!(!load(&mut e, CODE).failed);
        let stopped = e.eval_in_plugin(WEATHER, "while true do end");
        assert_eq!(stopped.stopped, [weather()]);
        drops_after_release(&stopped);
    }
}
