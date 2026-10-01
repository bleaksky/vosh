//! Designs Vosh ships (section 7.1 of the build spec).

/// At a glance, the design Vosh draws for a profile that has none of its
/// own.
///
/// In a fight it draws two rows. The top row names your opponent, then a
/// gauge ten cells wide, the percent and the game's own condition words,
/// and in a group the tank, colored by the tank's health. The name and
/// the gauge never move, and the percent always starts in the same cell.
/// The bottom row, the only row out of a fight, holds your health, mana
/// and moves as current over max, then your position and language, the
/// exits, your gold, Wizi and Incog out of a fight, and any tracked
/// affect you are missing.
///
/// Only your three numbers, the gauge, the tank and the missing affects
/// carry color, by one scale: green above two thirds, yellow above one
/// third, red below. Every label, max and tag is 256 color 245, a middle
/// gray. Since the design reads the tank, the game's tank line folds into
/// the fight row, so the prompt takes two rows at most. It ends in a
/// space, as the game's own prompt does, so your echo never touches it.
pub const AT_A_GLANCE: &str = concat!(
    // The fight row.
    "%{if:fight}",
    "%opponent %{opponent_hp:bar:10} %{opponent_hp:pct}%% ",
    "%{c:245}%opponent_cond%c_default",
    // Alone you are always the tank, and Group.Info is {}.
    "%{if:group_size}%{if:tank}  %{c:245}tank %c_tank_hp%tank%c_default%{end}%{end}",
    "%nl%{end}",
    // Your vitals.
    "%c_hp%hp%{c:245}/%{maxhp}hp%c_default ",
    "%c_mana%mana%{c:245}/%{maxmana}mn%c_default ",
    "%c_move%move%{c:245}/%{maxmove}mv%c_default",
    // Position and language, one space apart when both show.
    "%{if:pos}  %{c:245}%pos%c_default%{end}",
    "%{if:lang}%{ifnot:pos} %{end} %{c:245}%lang%c_default%{end}",
    "%{if:exits}  %{c:245}[%c_default%exits%{c:245}]%c_default%{end}",
    "%{if:gold}  %{gold:grouped}%{c:245}g%c_default%{end}",
    // Wizi and Incog show only when the game prints the immortal
    // prefix, and only out of a fight.
    "%{ifnot:fight}",
    "%{if:wizi}  %{c:245}wizi %wizi%c_default%{end}",
    "%{if:incog}%{ifnot:wizi} %{end} %{c:245}incog %incog%c_default%{end}",
    "%{end}",
    "%{if:missing}  %{c:245}missing %c_yellow%{missing:names}%c_default%{end}",
    " ",
);

/// Vosh's default design.
pub const DEFAULT_DESIGN: &str = AT_A_GLANCE;
