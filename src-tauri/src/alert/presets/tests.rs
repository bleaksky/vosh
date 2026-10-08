//! The alert presets against the Aabahran fixture packets, the caps and
//! the low latch twin against fixtures/alerts/low-latch.json.

use serde_json::Value;
use tokio::time::Instant;
use vosh_protocol::gmcp::Message;

use super::*;
use crate::alert::Caps;

/// A packet from fixtures/gmcp/aabahran.
fn packet(file: &str) -> Message {
    let path = format!(
        "{}/../fixtures/gmcp/aabahran/{file}",
        env!("CARGO_MANIFEST_DIR")
    );
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    vosh_protocol::gmcp::parse(&bytes).expect("a packet")
}

/// A packet built from its package and JSON.
fn built(package: &str, json: &str) -> Message {
    vosh_protocol::gmcp::parse(format!("{package} {json}").as_bytes()).expect("a packet")
}

/// A profile with the presets `on` turned on.
fn with_on(on: &[&str]) -> Profile {
    let mut p = Profile::default();
    p.ui.enabled_presets = on.iter().map(|id| (*id).to_string()).collect();
    p
}

/// Orla logged in, as Char.Status says.
fn orla(p: &Profile) -> PresetWatch {
    let mut watch = PresetWatch::default();
    let status = built(
        "Char.Status",
        r#"{"name":"Orla","level":50,"race":"human","class":"dark-knight"}"#,
    );
    assert_eq!(watch.gmcp(p, &status, None, Instant::now()), None);
    watch
}

#[test]
fn every_preset_ships_off_and_raises_alerts_that_ring_nothing_until_you_turn_it_on() {
    let p = Profile::default();
    let mut watch = orla(&p);
    let now = Instant::now();
    // Each event still comes, with nothing on, so it marks the row of a
    // session behind.
    let quiet = |alert: Option<Alert>| alert.is_some_and(|alert| alert.parts.is_silent());
    assert!(quiet(watch.gmcp(&p, &packet("chat/tell.gmcp"), None, now)));
    assert!(quiet(watch.gmcp(
        &p,
        &packet("char-combat.gmcp"),
        None,
        now
    )));
    assert!(quiet(watch.line(&p, "Maren looks at Orla.")));
    assert!(quiet(watch.health(&p, 170, 900, false)));
    assert!(connection(&p, Link::Lost).parts.is_silent());
    // The marker that turns every preset off wins over a list.
    let p = with_on(&[TELLS, PRESETS_OFF]);
    assert_eq!(parts(&p, TELLS), None);
}

#[test]
fn a_tell_you_get_rings_with_its_sender_and_a_tell_of_another_shape_does_not() {
    let p = with_on(&[TELLS]);
    let mut watch = PresetWatch::default();
    let now = Instant::now();
    let alert = watch
        .gmcp(&p, &packet("chat/tell.gmcp"), None, now)
        .expect("a tell rings");
    assert_eq!(alert.title, "Tell from Tolliver");
    assert_eq!(alert.words.as_deref(), Some("are you still at the bank?"));
    assert_eq!(alert.cap, "preset:alert_tells:Tolliver");
    assert_eq!(alert.source, "preset:alert_tells");
    // A preset the [alerts] table names no parts for posts a banner.
    assert!(alert.parts.banner && alert.parts.background && !alert.parts.words);
    // A tell in a language you do not know is still a tell to you.
    let foreign = watch.gmcp(&p, &packet("chat/tell-foreign.gmcp"), None, now);
    assert_eq!(
        foreign.map(|a| a.title).as_deref(),
        Some("Tell from Tolliver")
    );
    // A say, a yell or a gtell is no tell.
    for other in [
        "chat/say.gmcp",
        "chat/yell.gmcp",
        "chat/gtell-disguised.gmcp",
    ] {
        assert_eq!(watch.gmcp(&p, &packet(other), None, now), None, "{other}");
    }
    // A tell you send comes with no direction from the game.
    let sent = built(
        "Comm.Channel",
        r#"{"channel":"tell","speaker":"Tolliver","text":"are you still at the bank?"}"#,
    );
    assert_eq!(watch.gmcp(&p, &sent, None, now), None);
}

#[test]
fn the_alerts_table_says_what_a_preset_does() {
    let mut p = with_on(&[TELLS]);
    let quiet = AlertParts {
        sound: Some("knock".into()),
        background: false,
        ..AlertParts::default()
    };
    p.alerts.insert(TELLS.into(), quiet.clone());
    let alert = PresetWatch::default()
        .gmcp(&p, &packet("chat/tell.gmcp"), None, Instant::now())
        .expect("a tell rings");
    assert_eq!(alert.parts, quiet);
}

#[test]
fn a_tell_rings_once_per_sender_in_ten_seconds() {
    let p = with_on(&[TELLS]);
    let mut watch = PresetWatch::default();
    let mut caps = Caps::default();
    let start = Instant::now();
    let tolliver = packet("chat/tell.gmcp");
    let maren = built(
        "Comm.Channel",
        r#"{"channel":"tell","speaker":"Maren","text":"are you still at the bank?","language":"common","understood":true,"direction":"received"}"#,
    );
    let mut rang = Vec::new();
    for (secs, msg) in [
        (0, &tolliver),
        (3, &tolliver),
        (4, &maren),
        (9, &tolliver),
        (10, &tolliver),
    ] {
        let at = start + std::time::Duration::from_secs(secs);
        let alert = watch.gmcp(&p, msg, None, at).expect("a tell");
        if caps.allow(&alert.cap, at) {
            rang.push((secs, alert.title));
        }
    }
    assert_eq!(
        rang,
        [
            (0, "Tell from Tolliver".to_string()),
            (4, "Tell from Maren".to_string()),
            (10, "Tell from Tolliver".to_string()),
        ]
    );
}

#[test]
fn being_attacked_rings_when_a_fight_starts_on_you() {
    let p = with_on(&[ATTACKED]);
    let mut watch = orla(&p);
    let now = Instant::now();
    let alert = watch
        .gmcp(&p, &packet("char-combat.gmcp"), None, now)
        .expect("a fight starts");
    assert_eq!(alert.title, "A Blackwatch guard attacked you");
    assert_eq!(alert.cap, "preset:alert_attacked");
    // The next round of the same fight rings nothing, and once the fight
    // ends the next one rings again.
    assert_eq!(watch.gmcp(&p, &packet("char-combat.gmcp"), None, now), None);
    assert_eq!(
        watch.gmcp(&p, &packet("char-combat-end.gmcp"), None, now),
        None
    );
    assert!(watch
        .gmcp(&p, &packet("char-combat.gmcp"), None, now)
        .is_some());
}

#[test]
fn being_attacked_stays_quiet_when_a_groupmate_tanks_or_you_began_the_fight() {
    let p = with_on(&[ATTACKED]);
    let now = Instant::now();
    // The fixture's tank is a groupmate, not Orla.
    let mut watch = orla(&p);
    assert_eq!(
        watch.gmcp(&p, &packet("char-combat-tank.gmcp"), None, now),
        None
    );
    // The same fight with Orla as the tank rings.
    let mut watch = orla(&p);
    let tanking = built(
        "Char.Combat",
        r#"{"target":"a Blackwatch guard","condition":"quite a few wounds","hp_pct":54,"tank":{"name":"Orla","hp_pct":78}}"#,
    );
    assert!(watch.gmcp(&p, &tanking, None, now).is_some());
    // A line of yours left a second before, so you likely began it.
    let mut watch = orla(&p);
    let sent = now - std::time::Duration::from_secs(1);
    assert_eq!(
        watch.gmcp(&p, &packet("char-combat.gmcp"), Some(sent), now),
        None
    );
    // Two seconds after your last line, a fight is the game's doing.
    let mut watch = orla(&p);
    let sent = now - std::time::Duration::from_secs(2);
    assert!(watch
        .gmcp(&p, &packet("char-combat.gmcp"), Some(sent), now)
        .is_some());
}

#[test]
fn being_attacked_alone_rings_whatever_the_game_calls_you() {
    let p = with_on(&[ATTACKED]);
    let now = Instant::now();
    // In shadowform the game calls you `a shadow` (act_info.c:198), and
    // a solo fighter is the tank of their own group.
    let shadow = built(
        "Char.Combat",
        r#"{"target":"a Blackwatch guard","condition":"quite a few wounds","hp_pct":54,"tank":{"name":"a shadow","hp_pct":78}}"#,
    );
    let mut watch = orla(&p);
    assert_eq!(
        watch.gmcp(&p, &packet("group-info-solo.gmcp"), None, now),
        None
    );
    let alert = watch.gmcp(&p, &shadow, None, now).expect("you are alone");
    assert_eq!(alert.title, "A Blackwatch guard attacked you");
    // In a group the same tank is a groupmate the list names.
    let group = built(
        "Group.Info",
        r#"{"leader":"Maren","members":[{"id":1,"name":"Maren","level":40,"class":"warrior","hp_pct":90,"mana_pct":100,"move_pct":95,"tnl":800},{"id":2,"name":"a shadow","level":38,"class":"thief","hp_pct":78,"mana_pct":60,"move_pct":88,"tnl":900},{"id":3,"name":"Orla","level":50,"class":"dark-knight","hp_pct":64,"mana_pct":71,"move_pct":90,"tnl":1250}]}"#,
    );
    let mut watch = orla(&p);
    assert_eq!(watch.gmcp(&p, &group, None, now), None);
    assert_eq!(
        watch.gmcp(&p, &shadow, None, now),
        None,
        "a groupmate tanks"
    );
    // Under lamented tears the game hides the group, and a tank by
    // another name stays someone else.
    let mut watch = orla(&p);
    assert_eq!(
        watch.gmcp(&p, &packet("group-info-hidden.gmcp"), None, now),
        None
    );
    assert_eq!(watch.gmcp(&p, &shadow, None, now), None);
}

#[test]
fn your_name_rings_as_a_whole_word_with_its_capital_once_char_status_names_you() {
    let p = with_on(&[NAME]);
    // `$n looks at $N.`, act_info.c:1054.
    let line = "Maren looks at Orla.";
    assert_eq!(PresetWatch::default().line(&p, line), None, "no name yet");
    let watch = orla(&p);
    let alert = watch.line(&p, line).expect("your name");
    assert_eq!(alert.title, "Someone named you");
    assert_eq!(alert.words.as_deref(), Some(line));
    assert!(names(line, "Orla"));
    assert!(!names(line, "orla"), "the capital counts");
    assert!(!names(line, "Orl"), "a whole word");
    // A quote opens right before a name you are called by, as in the
    // yell of act_comm.c:1983, `$n yells '$t'`.
    let yell = "Tolliver yells 'Orla, help!'";
    assert_eq!(
        watch.line(&p, yell).map(|a| a.words),
        Some(Some(yell.to_string()))
    );
    // A line that starts with You is about you, `You now follow $N.`
    // from act_comm.c:3515 among them.
    assert_eq!(watch.line(&p, "You now follow Orla."), None);
    let mut dropped = watch;
    dropped.reset();
    assert_eq!(dropped.line(&p, line), None, "a drop forgets the name");
}

#[test]
fn the_connection_preset_rings_each_turn_of_the_link_under_its_own_cap() {
    let p = with_on(&[CONNECTION]);
    let alerts: Vec<Alert> = [Link::Lost, Link::Ready, Link::Stopped]
        .into_iter()
        .map(|link| connection(&p, link))
        .collect();
    let titles: Vec<&str> = alerts.iter().map(|a| a.title.as_str()).collect();
    assert_eq!(
        titles,
        ["Connection lost", "Ready to log in", "Vosh stopped trying"]
    );
    let mut caps = Caps::default();
    let now = Instant::now();
    assert!(alerts.iter().all(|a| caps.allow(&a.cap, now)));
    assert!(
        connection(&Profile::default(), Link::Lost)
            .parts
            .is_silent(),
        "off at first"
    );
}

/// fixtures/alerts/low-latch.json.
fn latch_cases() -> Value {
    let text = include_str!("../../../../fixtures/alerts/low-latch.json");
    serde_json::from_str(text).expect("the latch cases")
}

#[test]
fn the_low_latch_steps_match_the_page() {
    let cases = latch_cases();
    let steps = cases["steps"].as_array().expect("steps");
    assert_ne!(steps.len(), 0);
    for step in steps {
        let n = |key: &str| step[key].as_i64().expect("a number");
        let was = step["was"].as_bool().expect("was");
        assert_eq!(
            next_low(was, n("current"), n("max")),
            step["low"].as_bool().expect("low"),
            "{step}"
        );
    }
}

#[test]
fn low_health_rings_as_the_latch_rises_and_never_while_hidden() {
    let p = with_on(&[LOW_HEALTH]);
    let cases = latch_cases();
    for run in cases["runs"].as_array().expect("runs") {
        let mut watch = PresetWatch::default();
        for vitals in run["vitals"].as_array().expect("vitals") {
            let n = |key: &str| vitals[key].as_i64().expect("a number");
            let hidden = vitals["hidden"].as_bool().expect("hidden");
            let rang = watch.health(&p, n("hp"), n("maxhp"), hidden);
            assert_eq!(watch.low, vitals["low"].as_bool().expect("low"), "{vitals}");
            assert_eq!(
                rang.is_some(),
                vitals["rings"].as_bool().expect("rings"),
                "{vitals}"
            );
            if let Some(alert) = rang {
                let pct = vital_percent(n("hp"), n("maxhp"));
                assert_eq!(alert.title, format!("Health at {pct}%"));
            }
        }
    }
}

#[test]
fn the_page_lists_the_presets_in_this_order() {
    let page = include_str!("../../../../src/automation/alertPresets.ts");
    let ids: Vec<&str> = regex::Regex::new(r"(?m)^\s*id: '([^']*)',$")
        .unwrap()
        .captures_iter(page)
        .map(|id| id.get(1).unwrap().as_str())
        .collect();
    assert_eq!(ids, PRESETS);
}
