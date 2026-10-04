//! The Lua timers one poll of the session fires as a round.

use vosh_script::Owner;

use crate::profile::live::Profile;
use crate::script::{self, PendingTimer};
use crate::session::connection::Connection;
use crate::session::lua_timers::{fire_round, hold};

#[test]
fn timers_a_plugin_had_no_time_for_go_back_on_the_list() {
    let mut p = Profile::default();
    let mut c = Connection {
        script: vosh_script::testkit::engine_with_nap(),
        ..Connection::default()
    };
    let loaded = c.script.load_script(
        Owner::Plugin("slow".into()),
        "@slow/main.lua",
        "for i = 1, 6 do \
           mud.timer(0, function() os.nap(30) mud.echo('slow ' .. i) end) \
         end",
    );
    assert!(!loaded.failed, "{:?}", loaded.actions);
    let due = script::apply_actions(&mut p, &mut c, loaded).new_timers;
    assert_eq!(due.len(), 6);
    let (apply, held) = fire_round(&mut p, &mut c, due.clone());
    // Four naps use the budget, so one to four timers ran, the first ones.
    let ran = apply
        .echoes
        .iter()
        .filter(|line| line.starts_with("slow "))
        .count();
    assert!((1..=4).contains(&ran), "{:?}", apply.echoes);
    assert_eq!(
        apply.echoes.last().map(String::as_str),
        Some(
            "\x1b[90m[lua]\x1b[0m \x1b[31mslow used its 100 ms for this round of timers, \
             so the rest of its timers wait for the next round.\x1b[0m"
        )
    );
    // The rest come back as they were, deadline and id, for the next poll.
    let key = |t: &PendingTimer| (t.timer_id, t.callback_id, t.deadline);
    assert_eq!(
        held.iter().map(key).collect::<Vec<_>>(),
        due[ran..].iter().map(key).collect::<Vec<_>>()
    );
    // The next round runs the first of them.
    let (next, _) = fire_round(&mut p, &mut c, held);
    assert_eq!(next.echoes.first(), Some(&format!("slow {}", ran + 1)));
}

#[test]
fn held_timers_fire_before_a_later_timer_of_the_same_plugin() {
    let mut p = Profile::default();
    let mut c = Connection {
        script: vosh_script::testkit::engine_with_nap(),
        ..Connection::default()
    };
    let loaded = c.script.load_script(
        Owner::Plugin("slow".into()),
        "@slow/main.lua",
        "for i = 1, 6 do \
           mud.timer(0, function() os.nap(30) mud.echo('slow ' .. i) end) \
         end \
         mud.timer(1, function() mud.echo('later') end)",
    );
    assert!(!loaded.failed, "{:?}", loaded.actions);
    let mut timers = script::apply_actions(&mut p, &mut c, loaded).new_timers;
    assert_eq!(timers.len(), 7);
    // The first poll finds the six due, and the later timer stays on the
    // list.
    let mut list = timers.split_off(6);
    let (apply, held) = fire_round(&mut p, &mut c, timers);
    let ran = apply
        .echoes
        .iter()
        .filter(|line| line.starts_with("slow "))
        .count();
    assert!((1..=4).contains(&ran), "{:?}", apply.echoes);
    let key = |t: &PendingTimer| (t.timer_id, t.callback_id, t.deadline);
    // The held timers were due before anything still on the list, so
    // they go back ahead of the later timer.
    let mut want = held.iter().map(key).collect::<Vec<_>>();
    want.extend(list.iter().map(key));
    hold(&mut list, held);
    assert_eq!(list.iter().map(key).collect::<Vec<_>>(), want);
    // The later timer comes due before the next poll, which still runs
    // the held timers first.
    let (next, _) = fire_round(&mut p, &mut c, list);
    assert_eq!(next.echoes.first(), Some(&format!("slow {}", ran + 1)));
    let last_slow = next
        .echoes
        .iter()
        .rposition(|line| line.starts_with("slow "));
    if let Some(at) = next.echoes.iter().position(|line| line == "later") {
        assert!(last_slow < Some(at), "{:?}", next.echoes);
    }
}
