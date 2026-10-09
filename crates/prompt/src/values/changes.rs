//! How much health, mana and moves changed, two ways.
//!
//! - Over the last pulse, `%hp_change`: the value at this pulse less the
//!   value at the pulse before. A pulse is one Char.Vitals packet and the
//!   prompt it goes with, or one prompt on a game with no Char.Vitals, so
//!   it follows every gain and loss from a fight, regen or anything else.
//! - Over the last tick, `%hp_tick`: the value at this tick less the
//!   value at the tick before, which holds until the next tick. The tick
//!   is the one `%tick` counts, and the session says when it turns.
//!
//! Vosh reads the vitals once each pulse ends: at the prompt that ends
//! it, or at the end of the socket read when no prompt comes. Aabahran
//! queues GMCP with the text, so the World.Time that turns the tick can
//! come before or after the Char.Vitals of the same pulse. Both orders
//! give the same values, and a hit that lands in the same pulse counts
//! toward this tick either way.

use super::catalog::Pair;
use super::format::{Resolved, Value};
use super::{ClientValues, Vars};

/// Health, mana and moves at one reading, None for one Vosh did not know
/// or the game hid.
type Taken = [Option<i64>; 3];

/// Which change a field reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Over {
    /// Over the last pulse.
    Pulse,
    /// Over the last tick.
    Tick,
}

/// The vitals at the last two readings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Pair2 {
    before: Option<Taken>,
    at: Option<Taken>,
}

impl Pair2 {
    /// Take a new reading, the last one becoming the one before.
    fn shift(&mut self, now: Taken) {
        self.before = self.at.replace(now);
    }

    fn change(&self, pair: Pair) -> Option<i64> {
        let i = index(pair);
        let at = self.at?[i]?;
        let before = self.before?[i]?;
        Some(at.saturating_sub(before))
    }
}

/// The readings for both changes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Changes {
    pulse: Pair2,
    /// The pulse of the last pulse reading, and whether a prompt took it.
    read_at: Option<(u64, bool)>,
    tick: Pair2,
    /// A tick turned and its vitals wait for the end of its pulse. True
    /// when it is the tick the local timer just fired for, which the game
    /// confirmed, so it takes its vitals again in place of that one's.
    pending: Option<bool>,
}

fn index(pair: Pair) -> usize {
    match pair {
        Pair::Hp => 0,
        Pair::Mana => 1,
        Pair::Move => 2,
    }
}

impl Vars {
    /// The tick turned. `same` says it is the tick the local timer just
    /// fired for. Its vitals wait for the end of the pulse.
    pub fn tick_turned(&mut self, same: bool) {
        let c = &mut self.changes;
        c.pending = Some(c.pending.map_or(same, |was| was && same));
    }

    /// A prompt ended the pulse: read the vitals for both changes.
    pub fn prompt_ended(&mut self) {
        let now = self.vitals_now();
        let pulse = self.gmcp.pulse();
        let c = &mut self.changes;
        match c.read_at {
            // The end of a read already read this pulse, and the prompt
            // that ends it came in the next read.
            Some((at, false)) if at == pulse => c.pulse.at = Some(now),
            _ => c.pulse.shift(now),
        }
        c.read_at = Some((pulse, true));
        self.settle_tick(now);
    }

    /// A socket read ended. A pulse no prompt ended, as with the prompt
    /// turned off in the game, is read here, and so is a tick that waits.
    pub fn read_ended(&mut self) {
        let pulse = self.gmcp.pulse();
        let moved = self.changes.read_at.map_or(true, |(at, _)| at != pulse);
        if !moved && self.changes.pending.is_none() {
            return;
        }
        let now = self.vitals_now();
        if moved {
            self.changes.pulse.shift(now);
            self.changes.read_at = Some((pulse, false));
        }
        self.settle_tick(now);
    }

    /// Take the vitals of a tick that waits.
    fn settle_tick(&mut self, now: Taken) {
        let c = &mut self.changes;
        let Some(same) = c.pending.take() else {
            return;
        };
        if same && c.tick.at.is_some() {
            c.tick.at = Some(now);
        } else {
            c.tick.shift(now);
        }
    }

    /// Forget every reading, as a connect, a disconnect or a profile
    /// switch does.
    pub(super) fn forget_changes(&mut self) {
        self.changes = Changes::default();
    }

    /// The change for a pair, None before two readings, or when either
    /// did not know the value.
    pub(super) fn change(&self, pair: Pair, over: Over) -> Option<i64> {
        match over {
            Over::Pulse => self.changes.pulse.change(pair),
            Over::Tick => self.changes.tick.change(pair),
        }
    }

    fn vitals_now(&self) -> Taken {
        Pair::ALL.map(|pair| self.vital_now(pair))
    }

    /// A vital as the prompt reads it now, None when the game hides it or
    /// no source has a whole number for it.
    fn vital_now(&self, pair: Pair) -> Option<i64> {
        if self.hidden.pair(pair) {
            return None;
        }
        let client = ClientValues::default();
        match self.resolver(&client).gauge(pair, false) {
            Resolved::Value(Value::Gauge { cur, .. } | Value::Num(cur)) => Some(cur),
            _ => None,
        }
    }
}

/// The pair and the change a change field reads, `hp` over a pulse for
/// `hp_change`.
pub(crate) fn change_of(name: &str) -> Option<(Pair, Over)> {
    Some(match name {
        "hp_change" => (Pair::Hp, Over::Pulse),
        "mana_change" => (Pair::Mana, Over::Pulse),
        "move_change" => (Pair::Move, Over::Pulse),
        "hp_tick" => (Pair::Hp, Over::Tick),
        "mana_tick" => (Pair::Mana, Over::Tick),
        "move_tick" => (Pair::Move, Over::Tick),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::design::FieldRef;
    use crate::render::{render_str, RenderOptions};
    use crate::testkit::at;
    use crate::values::{Tick, Values};

    fn vitals(vars: &mut Vars, hp: i64, mana: i64, moves: i64) {
        vars.observe(
            "Char.Vitals",
            json!({"hp": hp, "maxhp": 1020, "mana": mana, "maxmana": 800,
                   "move": moves, "maxmove": 930}),
            at(),
        );
    }

    fn world_time(vars: &mut Vars, hour: i64) {
        vars.observe("World.Time", json!({"hour": hour}), at());
    }

    fn ticking() -> ClientValues {
        ClientValues {
            tick: Some(Tick {
                remaining: 20,
                interval: Some(30),
                since: Some(10),
            }),
            ..ClientValues::default()
        }
    }

    fn resolve(vars: &Vars, client: &ClientValues, name: &str) -> Resolved {
        vars.resolver(client).resolve(&FieldRef::new(name))
    }

    fn change(n: i64) -> Resolved {
        Resolved::Value(Value::Change(n))
    }

    /// One pulse as Aabahran sends it: its Char.Vitals, then its prompt.
    fn pulse(vars: &mut Vars, hp: i64, mana: i64, moves: i64) {
        vitals(vars, hp, mana, moves);
        vars.prompt_ended();
        vars.read_ended();
    }

    #[test]
    fn a_pulse_change_is_now_less_the_pulse_before() {
        let client = ticking();
        let mut vars = Vars::new(true);
        pulse(&mut vars, 1000, 800, 930);
        // One reading is not enough.
        assert_eq!(resolve(&vars, &client, "hp_change"), Resolved::Absent);
        pulse(&mut vars, 966, 812, 930);
        assert_eq!(resolve(&vars, &client, "hp_change"), change(-34));
        assert_eq!(resolve(&vars, &client, "mana_change"), change(12));
        assert_eq!(resolve(&vars, &client, "move_change"), change(0));
        // The next pulse with no change has nothing to show.
        pulse(&mut vars, 966, 812, 930);
        assert_eq!(resolve(&vars, &client, "hp_change"), change(0));
    }

    #[test]
    fn a_pulse_split_over_two_reads_is_one_reading() {
        let client = ticking();
        let mut vars = Vars::new(true);
        pulse(&mut vars, 1000, 800, 930);
        // Char.Vitals ends one read and its prompt starts the next.
        vitals(&mut vars, 990, 800, 930);
        vars.read_ended();
        assert_eq!(resolve(&vars, &client, "hp_change"), change(-10));
        vars.prompt_ended();
        vars.read_ended();
        assert_eq!(resolve(&vars, &client, "hp_change"), change(-10));
    }

    #[test]
    fn a_tick_change_holds_until_the_next_tick() {
        let client = ticking();
        let mut vars = Vars::new(true);
        world_time(&mut vars, 5);
        pulse(&mut vars, 900, 700, 900);
        vars.tick_turned(false);
        pulse(&mut vars, 940, 720, 930);
        // A single tick is not enough.
        assert_eq!(resolve(&vars, &client, "hp_tick"), Resolved::Absent);
        pulse(&mut vars, 920, 720, 930);
        vars.tick_turned(false);
        pulse(&mut vars, 974, 708, 930);
        assert_eq!(resolve(&vars, &client, "hp_tick"), change(34));
        assert_eq!(resolve(&vars, &client, "mana_tick"), change(-12));
        // Pulses between ticks leave it as it is.
        pulse(&mut vars, 500, 100, 100);
        assert_eq!(resolve(&vars, &client, "hp_tick"), change(34));
        assert_eq!(resolve(&vars, &client, "hp_change"), change(-474));
    }

    /// The tick turns in a read, and the Char.Vitals of the same pulse
    /// comes `before` or after it. Both read the vitals of that pulse.
    fn tick_in_one_read(vitals_first: bool) -> Resolved {
        let client = ticking();
        let mut vars = Vars::new(true);
        world_time(&mut vars, 5);
        pulse(&mut vars, 900, 700, 900);
        vars.tick_turned(false);
        pulse(&mut vars, 900, 700, 900);
        // The tick's pulse: a hit lands and the regen comes, in one read.
        if vitals_first {
            vitals(&mut vars, 960, 700, 900);
            vars.tick_turned(false);
        } else {
            vars.tick_turned(false);
            vitals(&mut vars, 960, 700, 900);
        }
        vars.prompt_ended();
        vars.read_ended();
        resolve(&vars, &client, "hp_tick")
    }

    #[test]
    fn the_tick_reads_the_same_vitals_in_either_packet_order() {
        assert_eq!(tick_in_one_read(true), change(60));
        assert_eq!(tick_in_one_read(false), change(60));
    }

    #[test]
    fn a_tick_with_no_prompt_reads_at_the_end_of_the_read() {
        let client = ticking();
        let mut vars = Vars::new(true);
        vitals(&mut vars, 900, 700, 900);
        vars.tick_turned(false);
        vars.read_ended();
        vitals(&mut vars, 950, 700, 900);
        vars.tick_turned(false);
        vars.read_ended();
        assert_eq!(resolve(&vars, &client, "hp_tick"), change(50));
    }

    #[test]
    fn the_game_tick_after_a_local_fire_is_the_same_tick() {
        let client = ticking();
        let mut vars = Vars::new(true);
        pulse(&mut vars, 900, 700, 900);
        vars.tick_turned(false);
        vars.read_ended();
        // The local timer fires a little early.
        pulse(&mut vars, 920, 700, 900);
        vars.tick_turned(false);
        vars.read_ended();
        assert_eq!(resolve(&vars, &client, "hp_tick"), change(20));
        // The game's tick lands a second later and takes its place.
        vars.tick_turned(true);
        pulse(&mut vars, 950, 700, 900);
        assert_eq!(resolve(&vars, &client, "hp_tick"), change(50));
    }

    #[test]
    fn nothing_shows_without_the_tick_or_after_a_disconnect() {
        let mut vars = Vars::new(true);
        pulse(&mut vars, 900, 700, 900);
        vars.tick_turned(false);
        pulse(&mut vars, 900, 700, 900);
        vars.tick_turned(false);
        pulse(&mut vars, 930, 700, 900);
        assert_eq!(resolve(&vars, &ticking(), "hp_tick"), change(30));
        // The tick timer is off.
        let off = ClientValues::default();
        assert_eq!(resolve(&vars, &off, "hp_tick"), Resolved::Absent);
        assert_eq!(resolve(&vars, &off, "hp_change"), change(30));
        vars.disconnect();
        assert_eq!(resolve(&vars, &ticking(), "hp_tick"), Resolved::Absent);
        assert_eq!(resolve(&vars, &ticking(), "hp_change"), Resolved::Absent);
        pulse(&mut vars, 900, 700, 900);
        assert_eq!(resolve(&vars, &ticking(), "hp_change"), Resolved::Absent);
    }

    #[test]
    fn a_profile_switch_starts_again() {
        let mut vars = Vars::new(true);
        pulse(&mut vars, 900, 700, 900);
        pulse(&mut vars, 930, 700, 900);
        assert_eq!(resolve(&vars, &ticking(), "hp_change"), change(30));
        vars.switch_profile(true);
        assert_eq!(resolve(&vars, &ticking(), "hp_change"), Resolved::Absent);
    }

    #[test]
    fn a_hidden_vital_has_no_change() {
        let client = ticking();
        let mut vars = Vars::new(true);
        pulse(&mut vars, 900, 700, 900);
        vars.observe(
            "Char.Vitals",
            json!({"hp": 0, "maxhp": 0, "mana": 0, "maxmana": 0, "move": 0,
                   "maxmove": 0, "hidden": true}),
            at(),
        );
        vars.prompt_ended();
        assert_eq!(resolve(&vars, &client, "hp_change"), Resolved::Hidden);
        pulse(&mut vars, 950, 700, 900);
        // The reading before was hidden.
        assert_eq!(resolve(&vars, &client, "hp_change"), Resolved::Absent);
    }

    #[test]
    fn a_gain_draws_green_a_loss_red_and_zero_as_chosen() {
        let client = ticking();
        let mut vars = Vars::new(true);
        let draw = |vars: &Vars, t: &str| {
            render_str(t, &vars.resolver(&client), RenderOptions::default()).ansi
        };
        pulse(&mut vars, 900, 700, 900);
        pulse(&mut vars, 934, 688, 900);
        assert_eq!(draw(&vars, "%hp_change"), "\x1b[32m+34\x1b[39m\x1b[0m");
        assert_eq!(draw(&vars, "%mana_change"), "\x1b[31m-12\x1b[39m\x1b[0m");
        // Zero shows nothing, 0 or ±0, and a condition on it fails.
        assert_eq!(draw(&vars, "[%move_change]"), "[]\x1b[0m");
        assert_eq!(draw(&vars, "[%{move_change:zero}]"), "[0]\x1b[0m");
        assert_eq!(draw(&vars, "[%{move_change:plusminus}]"), "[±0]\x1b[0m");
        assert_eq!(
            draw(&vars, "%{if:move_change}(%move_change)%{end}x"),
            "x\x1b[0m"
        );
        assert_eq!(
            draw(&vars, "%{if:hp_change}(%hp_change)%{end}"),
            "(\x1b[32m+34\x1b[39m)\x1b[0m"
        );
        // Before two readings every format draws nothing.
        vars.disconnect();
        assert_eq!(
            draw(&vars, "[%{hp_change:zero}%{hp_tick:plusminus}]"),
            "[]\x1b[0m"
        );
    }

    #[test]
    fn the_sign_color_shows_over_the_look_before_the_value() {
        let client = ticking();
        let mut vars = Vars::new(true);
        let draw = |vars: &Vars, t: &str| {
            render_str(t, &vars.resolver(&client), RenderOptions::default()).ansi
        };
        pulse(&mut vars, 900, 700, 900);
        pulse(&mut vars, 934, 688, 900);
        // A color the parts before it leave does not count, and the look
        // comes back after the value.
        assert_eq!(
            draw(&vars, "%c_gray(%hp_change)"),
            "\x1b[90m(\x1b[32m+34\x1b[90m)\x1b[0m"
        );
        assert_eq!(
            draw(&vars, "%c_cyan%s_bold<%hp_change x"),
            "\x1b[36m\x1b[1m<\x1b[32m+34\x1b[36m x\x1b[0m"
        );
        // Nor does a dim the parts before it leave, and it comes back too.
        assert_eq!(
            draw(&vars, "%s_dim%c_gray(%mana_change)"),
            "\x1b[2m\x1b[90m(\x1b[22;31m-12\x1b[2;90m)\x1b[0m"
        );
        assert_eq!(
            draw(&vars, "%s_bold%s_dim[%hp_change]"),
            "\x1b[1m\x1b[2m[\x1b[22;1;32m+34\x1b[2;39m]\x1b[0m"
        );
    }

    #[test]
    fn a_color_or_dim_on_the_value_itself_wins() {
        let client = ticking();
        let mut vars = Vars::new(true);
        let draw = |vars: &Vars, t: &str| {
            render_str(t, &vars.resolver(&client), RenderOptions::default()).ansi
        };
        pulse(&mut vars, 900, 700, 900);
        pulse(&mut vars, 934, 688, 900);
        // Its own color, over a color before it too.
        assert_eq!(draw(&vars, "%c_cyan%hp_change"), "\x1b[36m+34\x1b[0m");
        assert_eq!(
            draw(&vars, "%c_gray(%c_cyan%hp_change)"),
            "\x1b[90m(\x1b[36m+34)\x1b[0m"
        );
        // A By value color is its own color, red for a gain at low health.
        let mut low = Vars::new(true);
        pulse(&mut low, 100, 700, 900);
        pulse(&mut low, 134, 700, 900);
        assert_eq!(draw(&low, "%{c:hp}%hp_change"), "\x1b[31m+34\x1b[0m");
        // Its own dim keeps the sign color, dimmed.
        assert_eq!(
            draw(&vars, "(%s_dim%hp_change)"),
            "(\x1b[2m\x1b[32m+34\x1b[39m)\x1b[0m"
        );
        // The default color, or a reset, is no color of its own.
        assert_eq!(
            draw(&vars, "%c_gray(%c_default%hp_change"),
            "\x1b[90m(\x1b[39m\x1b[32m+34\x1b[39m\x1b[0m"
        );
        assert_eq!(
            draw(&vars, "%s_dim(%c_reset%hp_change"),
            "\x1b[2m(\x1b[0m\x1b[32m+34\x1b[39m\x1b[0m"
        );
        // A zero takes the look around it.
        assert_eq!(
            draw(&vars, "%c_gray(%{move_change:zero})"),
            "\x1b[90m(0)\x1b[0m"
        );
    }

    #[test]
    fn the_picker_offers_the_three_zero_forms() {
        let client = ticking();
        let mut vars = Vars::new(true);
        pulse(&mut vars, 900, 700, 900);
        pulse(&mut vars, 934, 700, 900);
        let resolver = vars.resolver(&client);
        let shown = |name: &str| -> Vec<(&str, String)> {
            crate::card::describe::forms(&FieldRef::new(name), &resolver)
                .into_iter()
                .map(|f| (f.label, f.sample.plain))
                .collect()
        };
        assert_eq!(
            shown("hp_change"),
            [
                ("Nothing at zero", "+34".to_string()),
                ("0 at zero", "+34".to_string()),
                ("±0 at zero", "+34".to_string()),
            ]
        );
        assert_eq!(
            shown("mana_change"),
            [
                ("Nothing at zero", String::new()),
                ("0 at zero", "0".to_string()),
                ("±0 at zero", "±0".to_string()),
            ]
        );
    }

    #[test]
    fn a_script_value_comes_first_and_keeps() {
        let client = ticking();
        let mut vars = Vars::new(true);
        vars.set_script("hp_tick", "+7");
        pulse(&mut vars, 900, 700, 900);
        assert_eq!(resolve(&vars, &client, "hp_tick"), change(7));
    }
}
