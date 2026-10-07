//! The time each plugin and loose script may spend on one event, across
//! every handler it has. One game line, one GMCP packet, one round of
//! timers and one replay of the last packets are each an event, and each
//! owner gets [`EVENT_BUDGET`] for its Lua triggers, GMCP handlers and
//! timers together. Once an owner used it, the rest of its handlers skip
//! the event.
//!
//! A handler that starts runs under the limits of one call, as any call
//! does, so the budget never stops a call. It only keeps the next handler
//! of the owner from starting. Your `#lua` lines and the Lua in your
//! triggers and aliases, with the functions they hand over, keep the
//! limits of one call alone.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::owner::Owner;

/// How long the handlers of one plugin or loose script may run in all
/// for one event.
pub(crate) const EVENT_BUDGET: Duration = Duration::from_millis(100);

/// What the handlers running now answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Event {
    /// A line the game sent, which Lua triggers run on.
    Line,
    /// A packet of this GMCP package.
    Packet(String),
    /// The Lua timers one check of the session found due.
    Timers,
    /// The last packets, handed to the GMCP handlers a call just made.
    Replay,
}

/// The time each owner spent on one event, and whose handlers it skipped.
#[derive(Debug)]
pub(crate) struct Budget {
    pub(crate) event: Event,
    spent: HashMap<Owner, Duration>,
    /// The owners a handler of which skipped this event, which Vosh said
    /// once.
    skipped: HashSet<Owner>,
}

impl Budget {
    pub(crate) fn new(event: Event) -> Self {
        Self {
            event,
            spent: HashMap::new(),
            skipped: HashSet::new(),
        }
    }

    /// True when `owner` has a budget at all. A plugin and a loose script
    /// have one, and the rest keep the limits of one call alone.
    fn counts(owner: &Owner) -> bool {
        matches!(owner, Owner::Plugin(_) | Owner::Script(_))
    }

    /// True when a handler of `owner` may start, since `owner` has time
    /// left in this event or has no budget.
    pub(crate) fn allows(&self, owner: &Owner) -> bool {
        !Self::counts(owner) || self.spent.get(owner).copied().unwrap_or_default() < EVENT_BUDGET
    }

    /// Add `took`, the time one handler of `owner` ran, to what `owner`
    /// spent on this event.
    pub(crate) fn charge(&mut self, owner: &Owner, took: Duration) {
        if Self::counts(owner) {
            *self.spent.entry(owner.clone()).or_default() += took;
        }
    }

    /// Note that a handler of `owner` skipped this event. True the first
    /// time for each owner, when Vosh says so.
    pub(crate) fn skip(&mut self, owner: &Owner) -> bool {
        self.skipped.insert(owner.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{error_lines, said};
    use crate::testkit::engine_with_nap as engine;
    use crate::{Action, ScriptEngine, ScriptOutcome};

    /// How long each slow handler naps, on the wall clock. Four naps use
    /// the budget, so one owner runs at most four slow handlers for one
    /// event. A nap stays well inside the limit of one call.
    const SLOW_MS: u32 = 30;

    /// Load `code` as the plugin `name`.
    fn plugin(e: &mut ScriptEngine, name: &str, code: &str) -> ScriptOutcome {
        let outcome = e.load_script(
            Owner::Plugin(name.into()),
            &format!("@{name}/main.lua"),
            code,
        );
        assert!(!outcome.failed, "{:?}", outcome.actions);
        outcome
    }

    /// The echoes of an outcome.
    fn echoes(outcome: &ScriptOutcome) -> Vec<String> {
        outcome
            .actions
            .iter()
            .filter_map(|action| match action {
                Action::Echo(text) => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    /// The echoes of an outcome that start with `prefix`.
    fn echoes_of(outcome: &ScriptOutcome, prefix: &str) -> Vec<String> {
        echoes(outcome)
            .into_iter()
            .filter(|text| text.starts_with(prefix))
            .collect()
    }

    /// Check that the slow handlers that ran are the first ones, from one
    /// up to the four the budget allows, and hand back how many ran.
    #[track_caller]
    fn ran_the_first_few(ran: &[String], prefix: &str) -> usize {
        let count = ran.len();
        assert!((1..=4).contains(&count), "{ran:?}");
        let first: Vec<String> = (1..=count).map(|i| format!("{prefix}{i}")).collect();
        assert_eq!(ran, first);
        count
    }

    const DAY: &str = "The day has begun.";

    #[test]
    fn a_plugin_and_a_loose_script_each_spend_their_own_time() {
        let mut budget = Budget::new(Event::Line);
        let meals = Owner::Plugin("meals".into());
        let combat = Owner::Script("combat.lua".into());
        budget.charge(&meals, Duration::from_millis(60));
        assert!(budget.allows(&meals));
        budget.charge(&meals, Duration::from_millis(40));
        assert!(!budget.allows(&meals));
        assert!(budget.allows(&combat));
        budget.charge(&combat, Duration::from_millis(150));
        assert!(!budget.allows(&combat));
        // Vosh says so once for each owner.
        assert!(budget.skip(&meals));
        assert!(!budget.skip(&meals));
        assert!(budget.skip(&combat));
    }

    #[test]
    fn your_own_lua_keeps_the_limits_of_one_call_alone() {
        let mut budget = Budget::new(Event::Timers);
        for owner in [
            Owner::Typed,
            Owner::Trigger("tells".into()),
            Owner::Alias("heal".into()),
        ] {
            budget.charge(&owner, Duration::from_secs(1));
            assert!(budget.allows(&owner), "{owner:?}");
        }
    }

    #[test]
    fn a_plugin_with_many_slow_triggers_skips_the_rest_of_one_line() {
        let mut e = engine();
        plugin(
            &mut e,
            "slow",
            &format!(
                "for i = 1, 10 do \
                   mud.trigger('day' .. i, 'The day has begun', function() \
                     os.nap({SLOW_MS}) mud.echo('slow ' .. i) \
                   end) \
                 end"
            ),
        );
        plugin(
            &mut e,
            "quick",
            "mud.trigger('day', 'The day has begun', function() mud.echo('quick') end)",
        );
        // Your own Lua has no budget, so two triggers that run past it
        // between them both run.
        e.eval(
            &format!(
                "for i = 1, 2 do \
                   mud.trigger('mine' .. i, 'The day has begun', function() \
                     os.nap({SLOW_MS}) os.nap({SLOW_MS}) mud.echo('typed ' .. i) \
                   end) \
                 end"
            ),
            "=#lua",
        );
        let outcome = e.match_line(DAY);
        assert!(!outcome.failed, "{:?}", outcome.actions);
        ran_the_first_few(&echoes_of(&outcome, "slow "), "slow ");
        // Another plugin has a budget of its own.
        assert_eq!(echoes_of(&outcome, "quick"), ["quick"]);
        assert_eq!(echoes_of(&outcome, "typed "), ["typed 1", "typed 2"]);
        // One line says so, for the whole line, about the plugin.
        assert_eq!(
            said(&outcome),
            [(
                "error",
                Owner::Plugin("slow".into()),
                "slow used its 100 ms for this line, so Vosh skipped the rest of its handlers."
                    .to_string()
            )]
        );
        // The plugin stays on and keeps every trigger.
        assert!(!e.is_stopped(&Owner::Plugin("slow".into())));
        assert_eq!(e.lua_triggers().len(), 13);
    }

    #[test]
    fn a_plugin_and_a_loose_script_with_many_slow_handlers_skip_the_rest_of_one_packet() {
        let mut e = engine();
        let handlers = |name: &str| {
            format!(
                "for i = 1, 10 do \
                   mud.on_gmcp('Char.Vitals', function() \
                     os.nap({SLOW_MS}) mud.echo('{name} ' .. i) \
                   end) \
                 end"
            )
        };
        plugin(&mut e, "slow", &handlers("slow"));
        let loose = e.load_script(
            Owner::Script("combat.lua".into()),
            "@combat.lua",
            &handlers("combat"),
        );
        assert!(!loose.failed, "{:?}", loose.actions);
        let outcome = e.dispatch_gmcp("Char.Vitals", &serde_json::json!({}));
        assert!(!outcome.failed, "{:?}", outcome.actions);
        ran_the_first_few(&echoes_of(&outcome, "slow "), "slow ");
        ran_the_first_few(&echoes_of(&outcome, "combat "), "combat ");
        assert_eq!(
            error_lines(&outcome),
            [
                "slow used its 100 ms for this Char.Vitals packet, so Vosh skipped the rest of its handlers.",
                "combat.lua used its 100 ms for this Char.Vitals packet, so Vosh skipped the rest of its handlers.",
            ]
        );
    }

    #[test]
    fn the_budget_starts_again_with_the_next_event() {
        let mut e = engine();
        plugin(
            &mut e,
            "slow",
            &format!(
                "for i = 1, 6 do \
                   mud.trigger('day' .. i, 'The day has begun', function() \
                     os.nap({SLOW_MS}) mud.echo('slow ' .. i) \
                   end) \
                 end \
                 mud.on_gmcp('Room.Info', function() mud.echo('room') end)"
            ),
        );
        let line = "slow used its 100 ms for this line, so Vosh skipped the rest of its handlers.";
        let first = e.match_line(DAY);
        ran_the_first_few(&echoes_of(&first, "slow "), "slow ");
        assert_eq!(error_lines(&first), [line]);
        // A packet after the line runs the plugin's handler.
        let room = e.dispatch_gmcp("Room.Info", &serde_json::json!({}));
        assert_eq!(echoes(&room), ["room"]);
        let leftover = &error_lines(&room);
        assert!(leftover.is_empty(), "{leftover:?}");
        // The next line starts from the first trigger again, and says so
        // again once the plugin used its time.
        let second = e.match_line(DAY);
        ran_the_first_few(&echoes_of(&second, "slow "), "slow ");
        assert_eq!(error_lines(&second), [line]);
    }

    /// The callback id of each timer an outcome starts.
    fn timer_callbacks(outcome: &ScriptOutcome) -> Vec<i64> {
        outcome
            .actions
            .iter()
            .filter_map(|action| match action {
                Action::Timer { callback_id, .. } => Some(*callback_id),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn timers_past_the_budget_wait_for_the_next_round() {
        let mut e = engine();
        let loaded = plugin(
            &mut e,
            "slow",
            &format!(
                "for i = 1, 6 do \
                   mud.timer(0, function() os.nap({SLOW_MS}) mud.echo('slow ' .. i) end) \
                 end"
            ),
        );
        let ids = timer_callbacks(&loaded);
        assert_eq!(ids.len(), 6);
        let round = e.fire_timers(&ids);
        let ran = ran_the_first_few(&echoes_of(&round.outcome, "slow "), "slow ");
        assert_eq!(
            error_lines(&round.outcome),
            ["slow used its 100 ms for this round of timers, so the rest of its timers wait for the next round."]
        );
        // The rest never ran and come back, in their order.
        assert_eq!(round.held, ids[ran..]);
        // The next round runs them from where the last one stopped.
        let next = e.fire_timers(&round.held);
        let more = echoes_of(&next.outcome, "slow ");
        assert_eq!(more.first(), Some(&format!("slow {}", ran + 1)));
        // A timer that ran is gone, so it never runs twice.
        let leftover = &e.fire_timers(&ids[..ran]).outcome.actions;
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn a_replay_inside_a_line_has_its_own_budget_and_the_line_keeps_its_own() {
        let mut e = engine();
        e.dispatch_gmcp("Char.Vitals", &serde_json::json!({}));
        // The first trigger makes six slow handlers, and each new handler
        // gets the last packet as soon as that trigger ends.
        plugin(
            &mut e,
            "slow",
            &format!(
                "for i = 1, 10 do \
                   mud.trigger('day' .. i, 'The day has begun', function() \
                     if i == 1 then \
                       for j = 1, 6 do \
                         mud.on_gmcp('Char.Vitals', function() \
                           os.nap({SLOW_MS}) mud.echo('new ' .. j) \
                         end) \
                       end \
                     end \
                     os.nap({SLOW_MS}) mud.echo('slow ' .. i) \
                   end) \
                 end"
            ),
        );
        let outcome = e.match_line(DAY);
        assert!(!outcome.failed, "{:?}", outcome.actions);
        // The replay has a budget of its own, which its slow handlers use.
        ran_the_first_few(&echoes_of(&outcome, "new "), "new ");
        // The line gets back what the plugin had left of its budget, so the
        // rest of the triggers run until the plugin uses it.
        ran_the_first_few(&echoes_of(&outcome, "slow "), "slow ");
        // One line for each event, the replay first, as it ran inside the
        // first trigger.
        assert_eq!(
            error_lines(&outcome),
            [
                "slow used its 100 ms on the last packets, so the rest of its new handlers wait for the next packet.",
                "slow used its 100 ms for this line, so Vosh skipped the rest of its handlers.",
            ]
        );
    }

    #[test]
    fn a_replay_of_the_last_packets_is_an_event_of_its_own() {
        let mut e = engine();
        e.dispatch_gmcp("Char.Vitals", &serde_json::json!({}));
        // The load naps 90 ms inside the limit of one call. It is no
        // event, so its time leaves the replay the whole budget, and more
        // than one new handler runs. Were it charged, the first 10 ms
        // handler would use the rest and run alone. The short naps leave
        // a busy machine 90 ms to wake the first handler late.
        let loaded = plugin(
            &mut e,
            "slow",
            &format!(
                "os.nap({SLOW_MS}) os.nap({SLOW_MS}) os.nap({SLOW_MS}) \
                 for i = 1, 30 do \
                   mud.on_gmcp('Char.Vitals', function() \
                     os.nap(10) mud.echo('slow ' .. i) \
                   end) \
                 end"
            ),
        );
        let ran = echoes_of(&loaded, "slow ");
        assert!((2..30).contains(&ran.len()), "{ran:?}");
        let first: Vec<String> = (1..=ran.len()).map(|i| format!("slow {i}")).collect();
        assert_eq!(ran, first);
        assert_eq!(
            error_lines(&loaded),
            ["slow used its 100 ms on the last packets, so the rest of its new handlers wait for the next packet."]
        );
    }
}
