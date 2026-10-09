//! Your 0.7 vitals template in today's codes, by a code map, so a code
//! 0.7 printed as typed stays as typed. Text starts from it when your
//! 0.7 vitals had a template on. Nothing reads the 0.7 template after that.

/// Rewrite a 0.7 vitals template in today's codes. `bar_width` is the
/// 0.7 bar width the `%bar_` codes drew at.
///
/// 0.7 read `%%` as a percent sign, `%{name}` and a greedy `%name` of
/// letters, digits and underscores, in any case. A code the map renames
/// keeps its braces, so the text after it stays text. `%{c:...}`, and
/// any code the map does not know, stay as typed.
pub fn rewrite_07_vitals(template: &str, bar_width: u32) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(at) = rest.find('%') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let (raw, name, braced) = code_at(rest);
        let new = name.and_then(|name| mapped(&name.to_ascii_lowercase(), braced, bar_width));
        out.push_str(new.as_deref().unwrap_or(raw));
        rest = &rest[raw.len()..];
    }
    out.push_str(rest);
    out
}

/// The code `text` starts with, as 0.7's tokenizer read it: its raw text,
/// its name when it has one, and whether it was braced. `text` starts
/// with `%`.
fn code_at(text: &str) -> (&str, Option<&str>, bool) {
    let after = &text[1..];
    if after.starts_with('%') {
        return (&text[..2], None, false);
    }
    if let Some(body) = after.strip_prefix('{') {
        let named = body.find('}').filter(|&end| {
            end > 0
                && body[..end]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | ',' | '#'))
        });
        return match named {
            Some(end) => (&text[..end + 3], Some(&body[..end]), true),
            None => (&text[..1], None, false),
        };
    }
    let len = after
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(after.len());
    if len == 0 {
        return (&text[..1], None, false);
    }
    (&text[..=len], Some(&after[..len]), false)
}

/// Today's text for the 0.7 code `name`, or None to keep it as typed.
fn mapped(name: &str, braced: bool, bar_width: u32) -> Option<String> {
    let renamed = |to: &str| {
        Some(if braced {
            format!("%{{{to}}}")
        } else {
            format!("%{to}")
        })
    };
    // 0.7 printed the sign after its percents, and today's prints none.
    let pct = |of: &str| renamed(&format!("pct_{of}")).map(|code| code + "%%");
    let bar = |of: &str| Some(format!("%{{{of}:bar:{bar_width}}}"));
    match name {
        "mn" => renamed("mana"),
        "mmn" => renamed("maxmana"),
        "mv" => renamed("move"),
        "mmv" => renamed("maxmove"),
        "pct_hp" => pct("hp"),
        "pct_mn" => pct("mana"),
        "pct_mv" => pct("move"),
        "bar_hp" => bar("hp"),
        "bar_mn" => bar("mana"),
        "bar_mv" => bar("move"),
        // 0.7 drew the change since the tick began, and today's change
        // over the last tick holds until the next one.
        "dhp" => renamed("hp_tick"),
        "dmn" => renamed("mana_tick"),
        "dmv" => renamed("move_tick"),
        "tick" => Some("%{tick:unit}".into()),
        // The game hour, as 0.7 printed it.
        "time" => Some("%{hour:ampm}".into()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::NaiveDate;
    use serde_json::Value as Json;

    use super::rewrite_07_vitals;
    use crate::render::{render_str, RenderOptions};
    use crate::values::overrides::{Overridden, Preview};
    use crate::values::Samples;

    const SHIPPED: &str = "%hp(%pct_hp)h %mn(%pct_mn)m %mv(%pct_mv)v - (%tick) - %time";

    fn rewrite(template: &str) -> String {
        rewrite_07_vitals(template, 20)
    }

    #[test]
    fn each_row_of_the_map() {
        let rows = [
            ("%hp, %mhp", "%hp, %mhp"),
            ("%mn, %mmn", "%mana, %maxmana"),
            ("%mv, %mmv", "%move, %maxmove"),
            (
                "%pct_hp, %pct_mn, %pct_mv",
                "%pct_hp%%, %pct_mana%%, %pct_move%%",
            ),
            (
                "%bar_hp, %bar_mn, %bar_mv",
                "%{hp:bar:20}, %{mana:bar:20}, %{move:bar:20}",
            ),
            ("%dhp, %dmn, %dmv", "%hp_tick, %mana_tick, %move_tick"),
            ("%tick", "%{tick:unit}"),
            ("%time", "%{hour:ampm}"),
            (
                "%{c:196}x%{c:255,128,0}y%{c:#ff8800}",
                "%{c:196}x%{c:255,128,0}y%{c:#ff8800}",
            ),
            ("%gold %{qp} %maxmn %nope", "%gold %{qp} %maxmn %nope"),
        ];
        for (old, new) in rows {
            assert_eq!(rewrite(old), new, "{old}");
        }
        assert_eq!(rewrite_07_vitals("%bar_hp", 12), "%{hp:bar:12}");
    }

    #[test]
    fn reads_codes_the_way_0_7_did() {
        // A braced code keeps its braces, so the text after it stays text.
        assert_eq!(rewrite("%{mn}mn %{MV}"), "%{mana}mn %{move}");
        assert_eq!(rewrite("%{pct_mn}"), "%{pct_mana}%%");
        assert_eq!(rewrite("%MN"), "%mana");
        // A percent sign, a bare %, and braces 0.7 did not read stay.
        assert_eq!(rewrite("100%% % %{} %{a b} %"), "100%% % %{} %{a b} %");
        assert_eq!(rewrite("%%mn"), "%%mn");
        assert_eq!(rewrite("hp %dhp%nl"), "hp %hp_tick%nl");
        assert_eq!(rewrite("é%mné %{é}"), "é%manaé %{é}");
        assert_eq!(rewrite(""), "");
    }

    #[test]
    fn the_two_fixture_templates() {
        assert_eq!(
            rewrite("%hp/%maxhp %mn/%maxmn %mv/%maxmv"),
            "%hp/%maxhp %mana/%maxmn %move/%maxmv"
        );
        assert_eq!(
            rewrite(SHIPPED),
            "%hp(%pct_hp%%)h %mana(%pct_mana%%)m %move(%pct_move%%)v - (%{tick:unit}) - %{hour:ampm}"
        );
    }

    #[test]
    fn the_shipped_template_draws_as_0_7_did() {
        let now = NaiveDate::from_ymd_opt(2026, 10, 5)
            .unwrap()
            .and_hms_opt(21, 40, 0)
            .unwrap();
        let samples = Samples { now };
        let mut fight = Preview::Fight.overrides(&samples);
        let extra: BTreeMap<String, Json> = [("hp", "765"), ("tick", "17/60"), ("hour", "6")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), Json::from(v)))
            .collect();
        fight.values.extend(extra);
        let values = Overridden::new(&samples, &fight, now);
        let drawn = render_str(&rewrite(SHIPPED), &values, RenderOptions::default());
        assert_eq!(drawn.plain, "765(75%)h 800(100%)m 930(100%)v - (17s) - 6AM");
    }
}
