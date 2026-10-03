//! The `#tick` command, which shows and changes the tick timer settings.

use tokio::time::Instant;

use super::slash::parse_braced_pattern;
use super::{split_first_word, InputResult};
use crate::profile::live::Profile;

pub(super) fn slash_tick(profile: &mut Profile, args: &str) -> InputResult {
    let (cmd, rest) = split_first_word(args);
    let now = Instant::now();
    match cmd {
        "" => slash_tick_show(profile, now),
        "interval" => match rest.trim().parse::<u64>() {
            Ok(secs) if secs > 0 => {
                profile.tick.set_interval(secs, now);
                InputResult::echo_line(format!("tick interval set to {secs}s"))
            }
            _ => InputResult::error("usage #tick interval <secs>"),
        },
        "reset" => {
            profile.tick.reset(now);
            InputResult::echo_line("tick reset")
        }
        "on" => {
            let Some((pattern, _rest)) = parse_braced_pattern(rest) else {
                return InputResult::error("usage #tick on {pattern}");
            };
            match profile.tick.set_reset_pattern(Some(pattern.clone())) {
                Ok(()) => InputResult::echo_line(format!("tick will reset on /{pattern}/")),
                Err(e) => InputResult::error(format!("invalid regex: {e}")),
            }
        }
        "off" => {
            let _ = profile.tick.set_reset_pattern(None);
            InputResult::echo_line("tick reset pattern cleared")
        }
        "fire" => {
            let trimmed = rest.trim();
            if trimmed.is_empty() {
                profile.tick.config.auto_fire = None;
                InputResult::echo_line("tick auto-fire cleared")
            } else {
                profile.tick.config.auto_fire = Some(trimmed.to_string());
                InputResult::echo_line(format!("tick auto-fire set to: {trimmed}"))
            }
        }
        "nofire" => {
            profile.tick.config.auto_fire = None;
            InputResult::echo_line("tick auto-fire cleared")
        }
        "sound" => match rest.trim() {
            "on" => {
                profile.tick.config.sound = true;
                InputResult::echo_line("tick sound on")
            }
            "off" => {
                profile.tick.config.sound = false;
                InputResult::echo_line("tick sound off")
            }
            _ => InputResult::error("usage #tick sound on|off"),
        },
        "disable" => {
            profile.tick.disable();
            InputResult::echo_line("tick disabled")
        }
        "enable" => {
            profile.tick.enable(now);
            InputResult::echo_line("tick enabled")
        }
        "warn" => slash_tick_warn(profile, rest),
        other => InputResult::error(format!("unknown #tick subcommand `{other}`")),
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
            InputResult::echo_lines(lines)
        }
        "at" => match rest.trim().parse::<u64>() {
            Ok(secs) if secs > 0 => {
                profile.tick.config.warn_at_secs = Some(secs);
                InputResult::echo_line(format!("tick warn set to {secs}s before fire"))
            }
            _ => InputResult::error("usage #tick warn at <secs>"),
        },
        "off" => {
            profile.tick.config.warn_at_secs = None;
            InputResult::echo_line("tick warn disabled")
        }
        "message" => {
            let trimmed = rest.trim();
            if trimmed.is_empty() {
                profile.tick.config.warn_message = None;
                InputResult::echo_line("tick warn message cleared (default applies)")
            } else {
                profile.tick.config.warn_message = Some(trimmed.to_string());
                InputResult::echo_line(format!("tick warn message set to: {trimmed}"))
            }
        }
        "color" => {
            let trimmed = rest.trim();
            if trimmed.is_empty() {
                profile.tick.config.warn_color = None;
                InputResult::echo_line("tick warn color cleared (default bright-red)")
            } else {
                profile.tick.config.warn_color = Some(trimmed.to_string());
                InputResult::echo_line(format!("tick warn color set to: {trimmed}"))
            }
        }
        other => InputResult::error(format!(
            "unknown #tick warn subcommand `{other}`. usage: at <secs> | off | message <text> | color <name>"
        )),
    }
}

fn slash_tick_show(profile: &Profile, now: Instant) -> InputResult {
    let cfg = &profile.tick.config;
    let mut lines = Vec::new();
    let state = if cfg.enabled { "enabled" } else { "disabled" };
    lines.push(format!("tick {state}, interval {}s", cfg.interval_secs));
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
    InputResult::echo_lines(lines)
}
