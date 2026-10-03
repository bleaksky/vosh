//! The `#tick` command, which shows and changes the tick timer settings.

use tokio::time::Instant;

use super::{echo_one, error_echo, parse_braced_pattern, split_first_word, InputResult};
use crate::profile::Profile;

pub(super) fn slash_tick(profile: &mut Profile, args: &str) -> InputResult {
    let (cmd, rest) = split_first_word(args);
    let now = Instant::now();
    match cmd {
        "" => slash_tick_show(profile, now),
        "interval" => match rest.trim().parse::<u64>() {
            Ok(secs) if secs > 0 => {
                profile.tick.set_interval(secs, now);
                echo_one(format!("tick interval set to {secs}s"))
            }
            _ => error_echo("usage #tick interval <secs>".to_string()),
        },
        "reset" => {
            profile.tick.reset(now);
            echo_one("tick reset".to_string())
        }
        "on" => {
            let Some((pattern, _rest)) = parse_braced_pattern(rest) else {
                return error_echo("usage #tick on {pattern}".to_string());
            };
            match profile.tick.set_reset_pattern(Some(pattern.clone())) {
                Ok(()) => echo_one(format!("tick will reset on /{pattern}/")),
                Err(e) => error_echo(format!("invalid regex: {e}")),
            }
        }
        "off" => {
            let _ = profile.tick.set_reset_pattern(None);
            echo_one("tick reset pattern cleared".to_string())
        }
        "fire" => {
            let trimmed = rest.trim();
            if trimmed.is_empty() {
                profile.tick.config.auto_fire = None;
                echo_one("tick auto-fire cleared".to_string())
            } else {
                profile.tick.config.auto_fire = Some(trimmed.to_string());
                echo_one(format!("tick auto-fire set to: {trimmed}"))
            }
        }
        "nofire" => {
            profile.tick.config.auto_fire = None;
            echo_one("tick auto-fire cleared".to_string())
        }
        "sound" => match rest.trim() {
            "on" => {
                profile.tick.config.sound = true;
                echo_one("tick sound on".to_string())
            }
            "off" => {
                profile.tick.config.sound = false;
                echo_one("tick sound off".to_string())
            }
            _ => error_echo("usage #tick sound on|off".to_string()),
        },
        "disable" => {
            profile.tick.disable();
            echo_one("tick disabled".to_string())
        }
        "enable" => {
            profile.tick.enable(now);
            echo_one("tick enabled".to_string())
        }
        "warn" => slash_tick_warn(profile, rest),
        other => error_echo(format!("unknown #tick subcommand `{other}`")),
    }
}

fn slash_tick_warn(profile: &mut Profile, args: &str) -> InputResult {
    let (cmd, rest) = split_first_word(args);
    match cmd {
        "" => {
            let cfg = &profile.tick.config;
            let mut lines = Vec::new();
            match cfg.warn_at_secs {
                Some(s) => lines.push(format!("tick warn at {s}s before fire")),
                None => lines.push("tick warn: off".to_string()),
            }
            lines.push(format!(
                "  message: {}",
                cfg.warn_message.as_deref().unwrap_or("(default)")
            ));
            lines.push(format!(
                "  color:   {}",
                cfg.warn_color.as_deref().unwrap_or("bright-red")
            ));
            InputResult {
                bytes: Vec::new(),
                echo: lines,
                    }
        }
        "at" => match rest.trim().parse::<u64>() {
            Ok(secs) if secs > 0 => {
                profile.tick.config.warn_at_secs = Some(secs);
                echo_one(format!("tick warn set to {secs}s before fire"))
            }
            _ => error_echo("usage #tick warn at <secs>".to_string()),
        },
        "off" => {
            profile.tick.config.warn_at_secs = None;
            echo_one("tick warn disabled".to_string())
        }
        "message" => {
            let trimmed = rest.trim();
            if trimmed.is_empty() {
                profile.tick.config.warn_message = None;
                echo_one("tick warn message cleared (default applies)".to_string())
            } else {
                profile.tick.config.warn_message = Some(trimmed.to_string());
                echo_one(format!("tick warn message set to: {trimmed}"))
            }
        }
        "color" => {
            let trimmed = rest.trim();
            if trimmed.is_empty() {
                profile.tick.config.warn_color = None;
                echo_one("tick warn color cleared (default bright-red)".to_string())
            } else {
                profile.tick.config.warn_color = Some(trimmed.to_string());
                echo_one(format!("tick warn color set to: {trimmed}"))
            }
        }
        other => error_echo(format!(
            "unknown #tick warn subcommand `{other}`. usage: at <secs> | off | message <text> | color <name>"
        )),
    }
}

fn slash_tick_show(profile: &Profile, now: Instant) -> InputResult {
    let cfg = &profile.tick.config;
    let mut lines = Vec::new();
    let state = if cfg.enabled { "enabled" } else { "disabled" };
    lines.push(format!(
        "tick {state}, interval {}s",
        cfg.interval.as_secs()
    ));
    if let Some(remaining) = profile.tick.remaining(now) {
        lines.push(format!("  remaining {}s", remaining.as_secs()));
    } else {
        lines.push("  remaining (not running)".to_string());
    }
    if let Some(p) = &cfg.reset_pattern {
        lines.push(format!("  reset on /{p}/"));
    } else {
        lines.push("  no reset pattern".to_string());
    }
    if let Some(f) = &cfg.auto_fire {
        lines.push(format!("  auto-fire: {f}"));
    } else {
        lines.push("  auto-fire: (none)".to_string());
    }
    lines.push(format!("  sound {}", if cfg.sound { "on" } else { "off" }));
    match cfg.warn_at_secs {
        Some(s) => {
            let msg = cfg.warn_message.as_deref().unwrap_or("(default)");
            let color = cfg.warn_color.as_deref().unwrap_or("bright-red");
            lines.push(format!("  warn at {s}s | {color} | {msg}"));
        }
        None => lines.push("  warn (off)".to_string()),
    }
    InputResult {
        bytes: Vec::new(),
        echo: lines,
    }
}
